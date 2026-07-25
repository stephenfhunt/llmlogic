//! Front-end lowering: surface AST → core IR.
//!
//! Implements the §17 contract (2026-07-10/19): the engine core is
//! positional-only, so everything surface-level — named arguments, wildcards,
//! variable names — is resolved here, before evaluation. [`lower`] collects
//! *all* errors it can find rather than stopping at the first (structured
//! errors are a design pillar, `spec.md` §12).
//!
//! Pass order:
//!
//! 1. **Schema & predicate collection** — walk declares, imports, and every
//!    atom; intern [`ir::PredId`]s in first-appearance order; report arity
//!    conflicts. (Mixed positional/named needs no check: [`ast::Args`] makes
//!    it unrepresentable.)
//! 2. **Named → positional** — named arguments are resolved against the field
//!    schema collected in pass 1 from `declare` statements and explicit import
//!    schemas. Argument order within the literal is irrelevant (position comes
//!    from the schema); omitted fields become fresh anonymous variables
//!    (partial selection), and a head written in named form must supply every
//!    field (§4). A named literal and the positional literal it denotes lower
//!    to identical IR — named arguments are surface syntax only.
//! 3. **Wildcard elimination + variable numbering** — per-clause scope; named
//!    variables get dense slots in first-occurrence order, each `_` becomes a
//!    fresh slot; `var_names` records `Some(name)` / `None` accordingly.
//! 4. **Safety / range restriction** (§10) — every variable in the head, in a
//!    negated atom, or occurring only in comparisons must be bound by the body:
//!    a positive atom, an `=`-assignment, or an aggregate result. Facts must be
//!    ground. Stated against the [`crate::schedule`], not source order.
//! 5. **Stratification** (§7) — number predicates by Ullman relaxation over
//!    the dependency graph (negative edges strictly up), bucket rules by their
//!    head predicate's stratum. Recursion through negation is a structured
//!    error naming a concrete cycle; positive programs form a single stratum.
//!
//! Body literal order is preserved exactly — never reordered — because
//! provenance references body positions ([`ir::BodyIdx`]).
//!
//! Type inference (§4/§8) is deliberately *not* part of lowering; it runs as a
//! later pass over the IR (which retains spans for exactly that reason).

use std::collections::HashMap;

use crate::ast;
use crate::error::{Error, Warning};
use crate::ir;
use crate::schedule;

/// Lowers a surface program to the core IR, or reports every error found.
///
/// Data imports contribute no facts on this path: schema-less imports have no
/// arity, and named access to them is an error. It is the entry for
/// hand-constructed ASTs and tests; the pipeline uses
/// [`lower_with_sources`].
pub fn lower(program: &ast::Program) -> Result<ir::Program, Vec<Error>> {
    lower_with_sources(program, &[])
}

/// Lowers a surface program with data imports already loaded (§13).
///
/// `tables` is aligned 1:1 with the program's **data-import statements** in
/// source order (the same order [`crate::sources::load_imports`] produces).
/// Each loaded table supplies a schema-less import's arity and header field
/// names before any clause lowers — so named access to imported relations
/// works — and its rows become base facts. An empty `tables` reproduces
/// [`lower`]'s no-sources behavior exactly.
pub fn lower_with_sources(
    program: &ast::Program,
    tables: &[crate::sources::LoadedTable],
) -> Result<ir::Program, Vec<Error>> {
    let mut lowerer = Lowerer::default();
    lowerer.collect_predicates(program, tables);
    lowerer.attach_field_names();
    let mut out = ir::Program::default();

    let mut data_import = 0usize;
    for statement in &program.statements {
        match &statement.kind {
            ast::StatementKind::Import(import) => match &import.kind {
                // Module imports are spliced away by resolution before
                // lowering; one reaching this point means the caller skipped
                // that stage (§13).
                ast::ImportKind::Module => lowerer.errors.push(Error::semantic(format!(
                    "module import \"{}\" must be resolved before lowering",
                    import.path
                ))),
                ast::ImportKind::Data { relation, .. } => {
                    let pred = lowerer.pred_id(&relation.name);
                    out.imports.push(ir::ImportSpec {
                        pred,
                        path: import.path.clone(),
                        span: statement.span,
                    });
                    // Imported rows are ordinary base facts (§13): the leaves
                    // of provenance, typed by the same facts-pin-columns rule
                    // as in-program facts.
                    if let Some(table) = tables.get(data_import) {
                        for row in &table.rows {
                            out.facts.push(ir::Fact {
                                pred,
                                tuple: ir::Tuple(row.clone()),
                            });
                        }
                    }
                    data_import += 1;
                }
            },
            // Declarations contribute schema/arity only (collected above);
            // nothing survives into the IR itself.
            ast::StatementKind::Declare(_) => {}
            ast::StatementKind::Clause(clause) => lowerer.lower_clause(clause, &mut out),
            ast::StatementKind::Query(query) => lowerer.lower_query(query, &mut out),
        }
    }

    // The table is taken only now because pass 2 can still intern: a
    // schema-less import never used in a clause first appears at its
    // `pred_id` call above, and snapshotting earlier would leave its
    // `ImportSpec` pointing past the end of the table.
    out.predicates = std::mem::take(&mut lowerer.predicates);

    match stratify(&out.rules, &out.predicates) {
        Ok(strata) => out.strata = strata,
        Err(error) => {
            lowerer.errors.push(error);
            // Fall back to the single-stratum shape so the IR stays
            // well-formed on the error path (`lower` returns `Err` anyway).
            if !out.rules.is_empty() {
                out.strata = vec![(0..out.rules.len() as u32).map(ir::RuleId).collect()];
            }
        }
    }

    if lowerer.errors.is_empty() {
        Ok(out)
    } else {
        Err(lowerer.errors)
    }
}

/// Lowering state: the interning tables, the field-name registry, and
/// accumulated errors.
#[derive(Default)]
struct Lowerer {
    predicates: Vec<ir::PredicateInfo>,
    by_name: HashMap<String, ir::PredId>,
    /// Field names per predicate, from `declare` statements and explicit import
    /// schemas — what makes named-argument access possible (§4). Collected over
    /// the whole program in pass 1, so a `declare` may appear *after* the rule
    /// that uses the named form.
    schemas: HashMap<String, FieldSchema>,
    errors: Vec<Error>,
}

/// A predicate's field names and declared types, positionally ordered.
struct FieldSchema {
    /// Position → field name.
    fields: Vec<String>,
    /// Position → declared type, `None` where the field was named without a
    /// type. Same length as [`fields`](Self::fields).
    field_types: Vec<Option<ast::TypeName>>,
    /// Field name → position.
    by_field: HashMap<String, usize>,
    origin: SchemaOrigin,
}

/// Where a schema came from, so conflicts can name both sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaOrigin {
    Declare,
    Import,
    /// Field names inferred from a loaded source's header (§13).
    SourceHeader,
}

impl SchemaOrigin {
    fn label(self) -> &'static str {
        match self {
            SchemaOrigin::Declare => "`declare`",
            SchemaOrigin::Import => "the import schema",
            SchemaOrigin::SourceHeader => "the source header",
        }
    }
}

/// Whether an atom occupies a clause head or a body position — heads written in
/// named form must supply every field (§4), bodies may partially select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomPos {
    Head,
    Body,
}

/// How a compound (inline-arithmetic) atom argument is resolved (spec §17,
/// Phase D). Rules and queries hoist it to an `=`-assignment; facts, having no
/// body, constant-fold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArgMode {
    Hoist,
    Fold,
}

/// Per-clause variable numbering state.
#[derive(Default)]
struct VarScope {
    names: Vec<Option<String>>,
    by_name: HashMap<String, ir::Var>,
}

impl VarScope {
    fn slot(&mut self, name: &str) -> ir::Var {
        if let Some(&var) = self.by_name.get(name) {
            return var;
        }
        let var = ir::Var(self.names.len() as u32);
        self.names.push(Some(name.to_string()));
        self.by_name.insert(name.to_string(), var);
        var
    }

    fn fresh(&mut self) -> ir::Var {
        let var = ir::Var(self.names.len() as u32);
        self.names.push(None);
        var
    }
}

impl Lowerer {
    /// Pass 1: intern every predicate in first-appearance order and check
    /// arity consistency across declares, imports, and use sites. `tables`
    /// (aligned with data-import statements) supplies a schema-less import's
    /// arity and header field names.
    fn collect_predicates(
        &mut self,
        program: &ast::Program,
        tables: &[crate::sources::LoadedTable],
    ) {
        let mut data_import = 0usize;
        for statement in &program.statements {
            match &statement.kind {
                ast::StatementKind::Import(import) => {
                    // Module imports are spliced away before lowering; only
                    // data imports contribute schema/arity here.
                    if let ast::ImportKind::Data {
                        relation, schema, ..
                    } = &import.kind
                    {
                        match schema {
                            // An explicit schema fixes arity and field names.
                            Some(schema) => {
                                self.intern_checked(&relation.name, schema.len() as u32);
                                self.collect_schema(&relation.name, schema, SchemaOrigin::Import);
                            }
                            // A schema-less import takes both from its loaded
                            // header (§13). Without a table (the no-sources
                            // `lower`), arity is left to use sites and named
                            // access stays an error.
                            None => {
                                if let Some(table) = tables.get(data_import) {
                                    self.intern_checked(&relation.name, table.fields.len() as u32);
                                    self.collect_schema_from_header(&relation.name, &table.fields);
                                }
                            }
                        }
                        data_import += 1;
                    }
                }
                ast::StatementKind::Declare(declaration) => {
                    self.intern_checked(
                        &declaration.relation.name,
                        declaration.fields.len() as u32,
                    );
                    self.collect_schema(
                        &declaration.relation.name,
                        &declaration.fields,
                        SchemaOrigin::Declare,
                    );
                }
                ast::StatementKind::Clause(clause) => {
                    self.collect_atom(&clause.head);
                    for literal in &clause.body {
                        self.collect_literal(literal);
                    }
                }
                ast::StatementKind::Query(query) => {
                    for literal in &query.body {
                        self.collect_literal(literal);
                    }
                }
            }
        }
    }

    fn collect_literal(&mut self, literal: &ast::Literal) {
        if let ast::LiteralKind::Atom { atom, .. } = &literal.kind {
            self.collect_atom(atom);
        }
    }

    fn collect_atom(&mut self, atom: &ast::Atom) {
        match &atom.args {
            ast::Args::Positional(terms) => {
                self.intern_checked(&atom.predicate.name, terms.len() as u32);
            }
            // Named atoms may partially select, so their argument count does
            // not establish arity; the declare/import schema does. Interning
            // happens if the schema was seen; the named-args error is
            // reported during clause lowering.
            ast::Args::Named(_) => {}
        }
    }

    /// Copies collected schemas onto the interned predicate table, so field
    /// names survive lowering (§17, 2026-07-20). Named arguments are still
    /// fully resolved — these names exist for type inference and provenance
    /// rendering, which run over the IR with no access to the AST.
    ///
    /// Only attaches when the schema length matches the interned arity: a
    /// mismatch means an arity clash was already reported, and leaving the
    /// entry `None` keeps `PredicateInfo`'s invariant intact on the error path.
    fn attach_field_names(&mut self) {
        for (name, schema) in &self.schemas {
            let Some(&id) = self.by_name.get(name) else {
                continue;
            };
            let info = &mut self.predicates[id.0 as usize];
            if schema.fields.len() == info.arity as usize {
                info.fields = Some(schema.fields.clone());
                info.field_types = Some(schema.field_types.clone());
            }
        }
    }

    /// Records the field names of `name`, reporting duplicates within the
    /// schema and conflicts with a schema already recorded for the predicate.
    fn collect_schema(&mut self, name: &str, fields: &[ast::FieldDecl], origin: SchemaOrigin) {
        let names: Vec<String> = fields.iter().map(|f| f.name.name.clone()).collect();
        let types: Vec<Option<ast::TypeName>> = fields.iter().map(|f| f.ty).collect();
        self.collect_schema_parts(name, names, types, origin);
    }

    /// Records a source header's field names (§13): a header asserts names, not
    /// types (those come from the loaded values as facts), so every declared
    /// type is `None`.
    fn collect_schema_from_header(&mut self, name: &str, header: &[String]) {
        let types = vec![None; header.len()];
        self.collect_schema_parts(name, header.to_vec(), types, SchemaOrigin::SourceHeader);
    }

    fn collect_schema_parts(
        &mut self,
        name: &str,
        positions: Vec<String>,
        field_types: Vec<Option<ast::TypeName>>,
        origin: SchemaOrigin,
    ) {
        let mut by_field = HashMap::with_capacity(positions.len());
        for (position, field_name) in positions.iter().enumerate() {
            if by_field.insert(field_name.clone(), position).is_some() {
                self.errors.push(Error::semantic(format!(
                    "duplicate field `{field_name}` in the schema for `{name}`"
                )));
            }
        }

        if let Some(existing) = self.schemas.get(name) {
            if existing.fields != positions {
                let conflict = format!(
                    "conflicting schemas for `{name}`: {} gives ({}), {} gives ({})",
                    existing.origin.label(),
                    existing.fields.join(", "),
                    origin.label(),
                    positions.join(", ")
                );
                self.errors.push(Error::semantic(conflict));
            } else if existing.field_types != field_types {
                // Same field names, disagreeing declared types (§17): name both
                // origins so the mismatch is traceable.
                let conflict = format!(
                    "conflicting declared types for `{name}`: {} and {} give different \
                     column types",
                    existing.origin.label(),
                    origin.label(),
                );
                self.errors.push(Error::semantic(conflict));
            }
            return;
        }

        self.schemas.insert(
            name.to_string(),
            FieldSchema {
                fields: positions,
                field_types,
                by_field,
                origin,
            },
        );
    }

    /// Interns `name` at `arity`, reporting a semantic error if the predicate
    /// was already interned at a different arity.
    fn intern_checked(&mut self, name: &str, arity: u32) -> ir::PredId {
        if let Some(&id) = self.by_name.get(name) {
            let known = self.predicates[id.0 as usize].arity;
            if known != arity {
                self.errors.push(Error::semantic(format!(
                    "predicate `{name}` used with arity {arity}, but previously with arity {known}"
                )));
            }
            return id;
        }
        let id = ir::PredId(self.predicates.len() as u32);
        self.predicates.push(ir::PredicateInfo {
            name: name.to_string(),
            arity,
            // Filled in by `attach_field_names` once pass 1 completes.
            fields: None,
            field_types: None,
        });
        self.by_name.insert(name.to_string(), id);
        id
    }

    /// Looks up an already interned predicate. Only valid after
    /// `collect_predicates`; unseen names (possible for schema-less imports
    /// never used in a clause) are interned at arity 0 pending source load.
    fn pred_id(&mut self, name: &str) -> ir::PredId {
        if let Some(&id) = self.by_name.get(name) {
            id
        } else {
            self.intern_checked(name, 0)
        }
    }

    fn lower_clause(&mut self, clause: &ast::Clause, out: &mut ir::Program) {
        let mut scope = VarScope::default();

        if clause.body.is_empty() {
            self.lower_fact(clause, &mut scope, out);
            return;
        }

        // Inline arithmetic in the head hoists to `=`-assignments *after* the
        // body (spec §17, Phase D): head variables are bound by the body, so
        // the assignment must follow. A bare-term head (the common case) yields
        // no hoisted literals.
        let mut head_hoisted = Vec::new();
        let Some(head) = self.lower_atom(
            &clause.head,
            &mut scope,
            AtomPos::Head,
            ArgMode::Hoist,
            &mut head_hoisted,
        ) else {
            return;
        };

        let mut body = self.lower_body(&clause.body, &mut scope);
        body.extend(head_hoisted);

        let rule = ir::Rule {
            head,
            body,
            var_names: scope.names,
            span: clause.span,
        };
        self.check_rule_safety(&rule, &clause.head.predicate.name);
        out.rules.push(rule);
    }

    /// Lowers an empty-body clause to a ground fact (§10). Named arguments
    /// resolve as for any head; inline arithmetic is *constant-folded* rather
    /// than hoisted (a fact has no body to hold an assignment), so `p(1+1).`
    /// becomes `p(2).`. Any variable or a non-ground expression makes the fact
    /// non-ground — a structured error, checked against the *lowered* head so
    /// the named and positional paths differ only in wording.
    fn lower_fact(&mut self, clause: &ast::Clause, scope: &mut VarScope, out: &mut ir::Program) {
        let mut discard = Vec::new();
        let Some(head) = self.lower_atom(
            &clause.head,
            scope,
            AtomPos::Head,
            ArgMode::Fold,
            &mut discard,
        ) else {
            return;
        };
        // Fold mode constant-folds arithmetic rather than hoisting, so the only
        // thing that can land in `discard` is an aggregate (§9) — which a ground
        // fact has no body to compute over.
        if !discard.is_empty() {
            self.errors.push(Error::semantic(format!(
                "an aggregate cannot appear in a fact; `{}` has no body to aggregate over",
                clause.head.predicate.name
            )));
            return;
        }

        let mut values = Vec::with_capacity(head.args.len());
        let mut ground = true;
        for (position, arg) in head.args.iter().enumerate() {
            match arg {
                ir::Term::Const(value) => values.push(value.clone()),
                ir::Term::Var(var) => {
                    ground = false;
                    let name = scope.names[var.0 as usize].as_deref().unwrap_or("_");
                    let place = self.describe_arg(&clause.head, position);
                    self.errors.push(Error::semantic(format!(
                        "fact `{}` is not ground: variable `{name}` in {place}",
                        clause.head.predicate.name,
                    )));
                }
            }
        }
        if ground {
            out.facts.push(ir::Fact {
                pred: head.pred,
                tuple: ir::Tuple(values),
            });
        }
    }

    fn lower_query(&mut self, query: &ast::Query, out: &mut ir::Program) {
        let mut scope = VarScope::default();
        let body = self.lower_body(&query.body, &mut scope);
        // The answer variables are the *named* slots the body binds at the top
        // level (§14). `safe_bound_vars` is the same notion the head of a rule is
        // checked against, and it deliberately does not descend into aggregate
        // goals: a goal-local variable is existential to the aggregate (§9) and
        // has no value outside it, so projecting it would be meaningless — and
        // used to leave the answer row with an unbound slot.
        let bound = safe_bound_vars(&body);
        let projection: Vec<u32> = scope
            .names
            .iter()
            .enumerate()
            .filter(|(slot, name)| name.is_some() && bound.contains(&(*slot as u32)))
            .map(|(slot, _)| slot as u32)
            .collect();
        let lowered = ir::Query {
            body,
            var_names: scope.names,
            projection,
            span: query.span,
        };
        self.check_body_safety(&lowered.body, &lowered.var_names, "query");
        out.queries.push(lowered);
    }

    fn lower_body(&mut self, body: &[ast::Literal], scope: &mut VarScope) -> Vec<ir::BodyLiteral> {
        let mut lowered = Vec::with_capacity(body.len());
        for literal in body {
            match &literal.kind {
                ast::LiteralKind::Atom { negated, atom } => {
                    // Inline arithmetic in a body atom hoists to `=`-assignments
                    // placed immediately *before* the atom, so the computed
                    // value is bound when the atom is matched.
                    let mut hoisted = Vec::new();
                    let Some(lowered_atom) =
                        self.lower_atom(atom, scope, AtomPos::Body, ArgMode::Hoist, &mut hoisted)
                    else {
                        continue;
                    };
                    lowered.extend(hoisted);
                    let kind = if *negated {
                        ir::BodyLiteralKind::NegAtom(lowered_atom)
                    } else {
                        ir::BodyLiteralKind::Atom(lowered_atom)
                    };
                    lowered.push(ir::BodyLiteral {
                        kind,
                        span: literal.span,
                    });
                }
                ast::LiteralKind::Comparison(comparison) => {
                    // An aggregate operand hoists to a preceding literal (§9), so
                    // lower into a local buffer and place it before the comparison.
                    let mut hoisted = Vec::new();
                    let lhs = self.lower_expr(&comparison.lhs, scope, &mut hoisted);
                    let rhs = self.lower_expr(&comparison.rhs, scope, &mut hoisted);
                    lowered.extend(hoisted);
                    lowered.push(ir::BodyLiteral {
                        kind: ir::BodyLiteralKind::Compare {
                            op: comparison.op,
                            lhs,
                            rhs,
                        },
                        span: literal.span,
                    });
                }
                ast::LiteralKind::Presence { expr, negated } => {
                    let mut hoisted = Vec::new();
                    let expr = self.lower_expr(expr, scope, &mut hoisted);
                    lowered.extend(hoisted);
                    lowered.push(ir::BodyLiteral {
                        kind: ir::BodyLiteralKind::Presence {
                            expr,
                            negated: *negated,
                        },
                        span: literal.span,
                    });
                }
            }
        }
        lowered
    }

    /// Lowers an atom to positional form, or reports an error and returns
    /// `None`. `mode` decides how a non-term (inline-arithmetic) argument is
    /// handled: hoisted to a fresh `=`-assignment appended to `hoisted`
    /// ([`ArgMode::Hoist`], rules/queries) or constant-folded ([`ArgMode::Fold`],
    /// facts).
    fn lower_atom(
        &mut self,
        atom: &ast::Atom,
        scope: &mut VarScope,
        pos: AtomPos,
        mode: ArgMode,
        hoisted: &mut Vec<ir::BodyLiteral>,
    ) -> Option<ir::Atom> {
        match &atom.args {
            ast::Args::Positional(exprs) => {
                let pred = self.pred_id(&atom.predicate.name);
                let args = exprs
                    .iter()
                    .map(|expr| self.lower_arg_expr(expr, scope, pos, mode, hoisted))
                    .collect();
                Some(ir::Atom { pred, args })
            }
            ast::Args::Named(named) => {
                self.lower_named_atom(atom, named, scope, pos, mode, hoisted)
            }
        }
    }

    /// Lowers one atom argument, which is a full [`ast::Expr`] since the
    /// inline-arithmetic widening (spec §17, Phase D). A bare term lowers
    /// directly; a compound expression is either hoisted or constant-folded per
    /// `mode`.
    fn lower_arg_expr(
        &mut self,
        expr: &ast::Expr,
        scope: &mut VarScope,
        pos: AtomPos,
        mode: ArgMode,
        hoisted: &mut Vec<ir::BodyLiteral>,
    ) -> ir::Term {
        if let ast::ExprKind::Term(term) = &expr.kind {
            // A bare `absent` literal cannot be *matched* in a body atom
            // argument (§4): unification fails on absent, so this is a mistake —
            // steer to the presence test. Producing absent (a fact, a rule head,
            // an arithmetic operand, an `=` right-hand side) is fine; only a
            // literal in a body match position is an error.
            if pos == AtomPos::Body
                && matches!(term.kind, ast::TermKind::Constant(ast::Constant::Absent))
            {
                self.errors.push(Error::semantic(
                    "`absent` cannot be matched in a body atom argument (a value never \
                     unifies with absent); test presence with `X is absent` / \
                     `X is not absent` instead"
                        .to_string(),
                ));
                // Recover with a fresh slot so the rest of the clause still
                // lowers (lowering already failed; this is never emitted).
                return ir::Term::Var(scope.fresh());
            }
            return self.lower_term(term, scope);
        }
        let ir_expr = self.lower_expr(expr, scope, hoisted);
        match mode {
            ArgMode::Hoist => {
                // Fresh var V, plus `V = <expr>` for the caller to place. The
                // fresh (`None`-named) slot and `=`-assignment are exactly what a
                // hand-written `V = <expr>` produces, so the IR is engine-identical.
                let var = scope.fresh();
                hoisted.push(ir::BodyLiteral {
                    kind: ir::BodyLiteralKind::Compare {
                        op: ast::CmpOp::Eq,
                        lhs: ir::Expr::Term(ir::Term::Var(var)),
                        rhs: ir_expr,
                    },
                    span: expr.span,
                });
                ir::Term::Var(var)
            }
            ArgMode::Fold => {
                // Fact context: fold a ground expression through the engine's §8
                // arithmetic (single source of truth). A variable operand makes
                // the fact non-ground; returning a fresh slot lets the caller's
                // ground check report it.
                if expr_vars(&ir_expr).is_empty() {
                    match crate::engine::eval_expr(&ir_expr, &[]) {
                        Ok(value) => ir::Term::Const(value),
                        Err(error) => {
                            self.errors.push(error);
                            // Placeholder; lowering already failed, so it is
                            // never emitted in a successful result.
                            ir::Term::Const(ir::Value::Int(0))
                        }
                    }
                } else {
                    ir::Term::Var(scope.fresh())
                }
            }
        }
    }

    /// Pass 2: resolves a named-argument atom against its field schema (§4).
    ///
    /// The result is indistinguishable from lowering the equivalent positional
    /// atom: fields land at their schema positions regardless of the order they
    /// were written in, and omitted fields become fresh anonymous variables.
    fn lower_named_atom(
        &mut self,
        atom: &ast::Atom,
        named: &[ast::NamedArg],
        scope: &mut VarScope,
        pos: AtomPos,
        mode: ArgMode,
        hoisted: &mut Vec<ir::BodyLiteral>,
    ) -> Option<ir::Atom> {
        let predicate = &atom.predicate.name;
        let Some(schema) = self.schemas.get(predicate) else {
            self.errors.push(Error::semantic(format!(
                "named arguments require known field names for `{predicate}`: add a \
                 `declare {predicate}(...)` or an explicit import schema"
            )));
            return None;
        };

        // Position -> the index in `named` that supplied it.
        let mut assigned: Vec<Option<usize>> = vec![None; schema.fields.len()];
        let mut reported = Vec::new();
        for (index, arg) in named.iter().enumerate() {
            let Some(&position) = schema.by_field.get(&arg.field.name) else {
                reported.push(Error::semantic(format!(
                    "unknown field `{}` for predicate `{predicate}`; known fields: {}",
                    arg.field.name,
                    schema.fields.join(", ")
                )));
                continue;
            };
            if assigned[position].is_some() {
                reported.push(Error::semantic(format!(
                    "field `{}` is given twice in one `{predicate}` literal",
                    arg.field.name
                )));
                continue;
            }
            assigned[position] = Some(index);
        }

        // §4: a head cannot leave columns unbound, so the named form must
        // supply every field there. Bodies may partially select.
        if pos == AtomPos::Head {
            let missing: Vec<&str> = assigned
                .iter()
                .enumerate()
                .filter(|(_, supplied)| supplied.is_none())
                .map(|(position, _)| schema.fields[position].as_str())
                .collect();
            if !missing.is_empty() {
                reported.push(Error::semantic(format!(
                    "head `{predicate}` uses named arguments and must supply every field; \
                     missing: {}",
                    missing.join(", ")
                )));
            }
        }

        let failed = !reported.is_empty();
        self.errors.extend(reported);
        if failed {
            return None;
        }

        let pred = self.pred_id(predicate);
        let args = assigned
            .into_iter()
            .map(|supplied| match supplied {
                Some(index) => self.lower_arg_expr(&named[index].value, scope, pos, mode, hoisted),
                // Partial selection: an omitted field binds a fresh anonymous
                // variable, exactly as a positional `_` would.
                None => ir::Term::Var(scope.fresh()),
            })
            .collect();
        Some(ir::Atom { pred, args })
    }

    /// Names an argument position for error messages: by field name when the
    /// atom was written in named form, by index otherwise.
    fn describe_arg(&self, atom: &ast::Atom, position: usize) -> String {
        if matches!(atom.args, ast::Args::Named(_))
            && let Some(schema) = self.schemas.get(&atom.predicate.name)
            && let Some(field) = schema.fields.get(position)
        {
            return format!("field `{field}`");
        }
        format!("argument {}", position + 1)
    }

    fn lower_term(&mut self, term: &ast::Term, scope: &mut VarScope) -> ir::Term {
        match &term.kind {
            ast::TermKind::Constant(constant) => ir::Term::Const(self.lower_constant(constant)),
            ast::TermKind::Variable(name) => ir::Term::Var(scope.slot(name)),
            ast::TermKind::Wildcard => ir::Term::Var(scope.fresh()),
        }
    }

    fn lower_constant(&mut self, constant: &ast::Constant) -> ir::Value {
        match constant {
            ast::Constant::Symbol(s) => ir::Value::Symbol(s.clone()),
            ast::Constant::String(s) => ir::Value::String(s.clone()),
            ast::Constant::Int(i) => ir::Value::Int(*i),
            ast::Constant::Bool(b) => ir::Value::Bool(*b),
            ast::Constant::Absent => ir::Value::Absent,
            ast::Constant::Float(f) => match ir::F64::new(*f) {
                Ok(v) => ir::Value::Float(v),
                Err(e) => {
                    // Unreachable from parsed source (§3 has no NaN token) but
                    // hand-constructed ASTs can contain anything.
                    self.errors.push(e);
                    ir::Value::Float(ir::F64::new(0.0).expect("0.0 is not NaN"))
                }
            },
        }
    }

    /// Lowers a surface expression to core [`ir::Expr`]. A [set-builder
    /// aggregate](ast::ExprKind::Aggregate) has no `ir::Expr` form — it is hoisted
    /// into `hoisted` as a dedicated [`ir::BodyLiteralKind::Aggregate`] literal and
    /// replaced by the fresh result variable it binds, so callers must place
    /// `hoisted` in the body ahead of where the value is used.
    fn lower_expr(
        &mut self,
        expr: &ast::Expr,
        scope: &mut VarScope,
        hoisted: &mut Vec<ir::BodyLiteral>,
    ) -> ir::Expr {
        match &expr.kind {
            ast::ExprKind::Term(term) => ir::Expr::Term(self.lower_term(term, scope)),
            ast::ExprKind::Binary { op, lhs, rhs } => ir::Expr::Binary {
                op: *op,
                lhs: Box::new(self.lower_expr(lhs, scope, hoisted)),
                rhs: Box::new(self.lower_expr(rhs, scope, hoisted)),
            },
            ast::ExprKind::Aggregate(agg) => self.lower_aggregate(agg, expr.span, scope, hoisted),
        }
    }

    /// Hoists a set-builder aggregate (§9): it becomes a dedicated
    /// [`ir::BodyLiteralKind::Aggregate`] literal binding a fresh result slot
    /// (appended to `hoisted`), and the enclosing expression is left referring to
    /// that slot — exactly the shape a hand-written `V = op { … }` produces. The
    /// goal shares the enclosing scope, so a variable also occurring outside the
    /// aggregate resolves to the same slot (a group key) while a goal-only
    /// variable takes a fresh slot (existential to the goal). Any inner hoist from
    /// the collected expression (a nested aggregate) is placed at the end of the
    /// goal, where the goal's bindings are in scope.
    fn lower_aggregate(
        &mut self,
        agg: &ast::Aggregate,
        span: ast::Span,
        scope: &mut VarScope,
        hoisted: &mut Vec<ir::BodyLiteral>,
    ) -> ir::Expr {
        let mut goal = self.lower_body(&agg.goal, scope);
        // The collected expression is evaluated per witness of the goal, so a
        // nested aggregate inside it hoists into the goal, not the outer body.
        let expr = self.lower_expr(&agg.expr, scope, &mut goal);
        // Parameters (none for the v1 five) are evaluated once per group in the
        // outer scope.
        let params = agg
            .params
            .iter()
            .map(|param| self.lower_expr(param, scope, hoisted))
            .collect();
        let result = scope.fresh();
        hoisted.push(ir::BodyLiteral {
            kind: ir::BodyLiteralKind::Aggregate {
                result,
                op: agg.op,
                params,
                expr,
                goal,
            },
            span,
        });
        ir::Expr::Term(ir::Term::Var(result))
    }

    /// Pass 4 for rules: head variables must be bound by the body — a positive
    /// atom, an `=`-assignment, or an aggregate result (spec §17, 2026-07-21;
    /// scheduled rather than source-ordered since 2026-07-25).
    fn check_rule_safety(&mut self, rule: &ir::Rule, head_name: &str) {
        // A body that cannot be scheduled binds nothing reliably, so every head
        // variable would be reported unbound — cascading noise on top of the one
        // error that matters. `check_body_safety` reports that one.
        let schedulable =
            schedule::schedule_body_with(&rule.body, &std::collections::HashSet::new()).is_ok();
        if schedulable {
            let bound = safe_bound_vars(&rule.body);
            for arg in &rule.head.args {
                if let ir::Term::Var(var) = arg
                    && !bound.contains(&var.0)
                {
                    let name = rule.var_names[var.0 as usize].as_deref().unwrap_or("_");
                    self.errors.push(Error::semantic(format!(
                        "unsafe rule for `{head_name}`: head variable `{name}` is not bound \
                         by the body — it must occur in a positive body atom, or be bound by \
                         an `=`-assignment or an aggregate result"
                    )));
                }
            }
        }
        self.check_body_safety(&rule.body, &rule.var_names, head_name);
    }

    /// Pass 4 shared by rules and queries: variables in negated atoms or
    /// occurring only in comparisons must be bound by the body — a positive
    /// atom, an `=`-assignment target, or an aggregate result (spec §17,
    /// 2026-07-21; negated atoms relaxed from *positively* bound 2026-07-25).
    fn check_body_safety(
        &mut self,
        body: &[ir::BodyLiteral],
        var_names: &[Option<String>],
        context: &str,
    ) {
        self.check_body_safety_seeded(body, var_names, &std::collections::HashSet::new(), context);
    }

    /// [`Self::check_body_safety`] with a `seed` of variables treated as already
    /// bound — the group keys available to an aggregate's goal (§9). The
    /// top-level call seeds nothing, so ordinary rules and queries are
    /// unaffected.
    ///
    /// Safety is now stated against the **schedule** (`crate::schedule`), not
    /// source order: a body is safe when there *exists* an order in which every
    /// literal's inputs are bound before it runs. That subsumes the old
    /// per-literal "is it bound yet?" walk — the scheduler proves it for every
    /// comparison, presence test and aggregate at once — and leaves three checks
    /// that scheduling does not cover.
    fn check_body_safety_seeded(
        &mut self,
        body: &[ir::BodyLiteral],
        var_names: &[Option<String>],
        seed: &std::collections::HashSet<u32>,
        context: &str,
    ) {
        let seed_vars: std::collections::HashSet<ir::Var> =
            seed.iter().map(|slot| ir::Var(*slot)).collect();
        let bound: std::collections::HashSet<u32> =
            match schedule::schedule_body_with(body, &seed_vars) {
                Ok((_, bound)) => bound.iter().map(|var| var.0).collect(),
                Err(failure) => {
                    self.errors
                        .push(schedule_error(&failure, body, var_names, context));
                    // Without a valid order the remaining checks would report
                    // cascading nonsense about variables that are simply unreachable.
                    return;
                }
            };
        let positive = {
            let mut positive = positive_vars(body);
            positive.extend(seed.iter().copied());
            positive
        };

        for literal in body {
            match &literal.kind {
                ir::BodyLiteralKind::Atom(_) => {}
                ir::BodyLiteralKind::NegAtom(atom) => {
                    // A negated atom's variables must be bound by *something* —
                    // a positive atom, an `=`-assignment or an aggregate result
                    // all make it ground before the anti-join runs, and the
                    // scheduler places the negation after whichever it is (§7/§10).
                    //
                    // The scheduler cannot make this call itself: a slot bound
                    // nowhere is, to it, a wildcard — open and existential under
                    // the negation (§7), which is right for `_` and wrong for a
                    // named variable. `var_names` is what separates them, and
                    // keeping it here is what keeps `crate::schedule` free of it.
                    // Note the test is `bound`, not "has a name": hoisting mints
                    // unnamed slots that are nonetheless bound (`bugs/001`).
                    for arg in &atom.args {
                        if let ir::Term::Var(var) = arg
                            && var_names[var.0 as usize].is_some()
                            && !bound.contains(&var.0)
                        {
                            push_unsafe(
                                &mut self.errors,
                                var.0,
                                var_names,
                                "negated atom",
                                context,
                            );
                        }
                    }
                }
                ir::BodyLiteralKind::Compare { op, lhs, rhs } => {
                    // A bare `absent` literal *compared* against anything is
                    // false whatever the operator (§8) — `V = absent` and `V !=
                    // absent` are *both* false — so writing one is always a
                    // mistake. Steer to the presence test, the same way a literal
                    // `absent` in a body atom argument does. Exempt: the producer
                    // form `X = absent`, an assignment binding an unbound `X`,
                    // which is how §4 says to produce the value.
                    let produces = *op == ast::CmpOp::Eq
                        && [lhs, rhs].iter().any(|side| {
                            matches!(side, ir::Expr::Term(ir::Term::Var(var))
                                if !positive.contains(&var.0))
                        });
                    if !produces && (is_literal_absent(lhs) || is_literal_absent(rhs)) {
                        self.errors.push(Error::semantic(format!(
                            "`{}` against the literal `absent` in `{context}` is always \
                             false (every comparison with absent is false, so `=` and `!=` \
                             are both false); test presence with `X is absent` / \
                             `X is not absent` instead",
                            cmp_symbol(*op),
                        )));
                    }
                }
                ir::BodyLiteralKind::Presence { .. } => {}
                ir::BodyLiteralKind::Aggregate { expr, goal, .. } => {
                    // The goal is a sub-body evaluated with the group keys — the
                    // enclosing body's bindings — available, so seed its check
                    // with `bound`. Its own literals bind the goal-local
                    // (existential) variables (§9/§10).
                    self.check_body_safety_seeded(goal, var_names, &bound, context);
                    // The collected expression is evaluated per witness, so its
                    // variables must be bound by the goal or be a group key. The
                    // schedule covers the group keys (it treats them as the
                    // aggregate's reads); this covers the goal-local ones, which
                    // it does not see.
                    let mut goal_bound = safe_bound_vars(goal);
                    goal_bound.extend(bound.iter().copied());
                    for var in expr_vars(expr) {
                        if !goal_bound.contains(&var.0) {
                            push_unsafe(
                                &mut self.errors,
                                var.0,
                                var_names,
                                "aggregate expression",
                                context,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Renders a scheduling failure as a §12 error. The two causes want genuinely
/// different advice: an unbound variable needs a binder added, while a cycle
/// cannot be fixed by moving anything — there is no valid order at all.
fn schedule_error(
    failure: &schedule::ScheduleError,
    body: &[ir::BodyLiteral],
    var_names: &[Option<String>],
    context: &str,
) -> Error {
    let name = var_names
        .get(failure.variable.0 as usize)
        .and_then(|name| name.as_deref())
        .unwrap_or("_");
    let place = match &body[failure.literal].kind {
        ir::BodyLiteralKind::Compare { .. } => "comparison",
        ir::BodyLiteralKind::Presence { .. } => "presence test",
        ir::BodyLiteralKind::Aggregate { .. } => "aggregate",
        // A negation can be the stuck literal now that it waits for its binders
        // in the dependency phase (§7/§10, 2026-07-25).
        ir::BodyLiteralKind::NegAtom(_) => "negated atom",
        ir::BodyLiteralKind::Atom(_) => "body literal",
    };
    match failure.cause {
        schedule::ScheduleFailure::Unbound => Error::semantic(format!(
            "unsafe {place} in `{context}`: variable `{name}` is never bound — it must \
             occur in a positive body atom, or be bound by an `=`-assignment or an \
             aggregate result"
        )),
        schedule::ScheduleFailure::Cycle => Error::semantic(format!(
            "circular dependency in `{context}`: the {place} needs `{name}`, which is only \
             bound by a literal that in turn needs this one; no order of the body can run"
        )),
    }
}

/// Pass 5: stratification (§7). Numbers predicates by Ullman relaxation over
/// the dependency graph — a head predicate's stratum is leveled up to each
/// positive body predicate's stratum and strictly above each negated one's —
/// then buckets rules by their head predicate's stratum, preserving source
/// order within each bucket (empty levels are dropped). A stratum number
/// reaching the predicate count witnesses recursion through negation; the
/// error names a concrete cycle. By the independence theorem (§7) every valid
/// stratification yields the same perfect model, so this particular numbering
/// carries no semantic weight.
///
/// Everything here iterates rules in `RuleId` order and body literals in
/// `BodyIdx` order — lowering is deterministic including its error list (A7).
fn stratify(
    rules: &[ir::Rule],
    predicates: &[ir::PredicateInfo],
) -> Result<Vec<Vec<ir::RuleId>>, Error> {
    // Dependency edges: (head, body predicate, kind). A negated (§7) or
    // aggregated (§9) dependency forces the body predicate strictly lower.
    let mut edges = Vec::new();
    for rule in rules {
        collect_stratum_edges(rule.head.pred, &rule.body, false, &mut edges);
    }

    let mut stratum = vec![0u32; predicates.len()];
    loop {
        let mut changed = false;
        for &(head, body, dep) in &edges {
            let required = stratum[body.0 as usize] + u32::from(dep.strict());
            if stratum[head.0 as usize] < required {
                if required as usize >= predicates.len() {
                    // With n predicates a stratifiable program needs at most
                    // n levels (0..n), so reaching n proves divergence.
                    return Err(negative_cycle_error(&edges, predicates));
                }
                stratum[head.0 as usize] = required;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let levels = stratum.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut buckets: Vec<Vec<ir::RuleId>> = vec![Vec::new(); levels];
    for (index, rule) in rules.iter().enumerate() {
        buckets[stratum[rule.head.pred.0 as usize] as usize].push(ir::RuleId(index as u32));
    }
    buckets.retain(|bucket| !bucket.is_empty());
    Ok(buckets)
}

/// How a rule's head depends on a body predicate, for stratification (§7/§9).
/// A `Positive` dependency may share a stratum; `Negated` and `Aggregated` both
/// force the body predicate strictly lower, and a cycle through either is
/// unstratifiable.
#[derive(Clone, Copy, PartialEq)]
enum Dep {
    Positive,
    Negated,
    Aggregated,
}

impl Dep {
    /// Does this dependency force a strictly-lower stratum?
    fn strict(self) -> bool {
        !matches!(self, Dep::Positive)
    }

    /// How the edge reads inside a cycle-error path.
    fn prefix(self) -> &'static str {
        match self {
            Dep::Positive => "",
            Dep::Negated => "not ",
            Dep::Aggregated => "agg ",
        }
    }
}

/// Collects dependency edges `(head, body-predicate, kind)` for stratification,
/// recursing into aggregate goals. A predicate reached under an aggregate is an
/// `Aggregated` (strictly-lower) dependency — §9 reads the aggregated relation
/// whole, exactly as negation does, so recursion through an aggregate is rejected
/// the same way. Iterates in body order for a deterministic edge list (A7).
fn collect_stratum_edges(
    head: ir::PredId,
    body: &[ir::BodyLiteral],
    under_aggregate: bool,
    edges: &mut Vec<(ir::PredId, ir::PredId, Dep)>,
) {
    let positive = if under_aggregate {
        Dep::Aggregated
    } else {
        Dep::Positive
    };
    for literal in body {
        match &literal.kind {
            ir::BodyLiteralKind::Atom(atom) => edges.push((head, atom.pred, positive)),
            ir::BodyLiteralKind::NegAtom(atom) => edges.push((head, atom.pred, Dep::Negated)),
            // Neither references a predicate (§4/§8).
            ir::BodyLiteralKind::Compare { .. } | ir::BodyLiteralKind::Presence { .. } => {}
            ir::BodyLiteralKind::Aggregate { goal, .. } => {
                collect_stratum_edges(head, goal, true, edges);
            }
        }
    }
}

/// Recovers a concrete cycle through a negative edge, for the stratification
/// error. Only called once relaxation has diverged, which proves such a cycle
/// exists: some negative edge `head → not body` closes back from `body` to
/// `head` through dependency edges.
fn negative_cycle_error(
    edges: &[(ir::PredId, ir::PredId, Dep)],
    predicates: &[ir::PredicateInfo],
) -> Error {
    let name = |pred: ir::PredId| predicates[pred.0 as usize].name.as_str();
    // Adjacency in edge order, so the recovered cycle is deterministic.
    let mut deps: Vec<Vec<(ir::PredId, Dep)>> = vec![Vec::new(); predicates.len()];
    for &(head, body, dep) in edges {
        deps[head.0 as usize].push((body, dep));
    }
    for &(head, body, dep) in edges {
        if !dep.strict() {
            continue;
        }
        let Some(steps) = dependency_path(&deps, body, head) else {
            continue;
        };
        let mut parts = vec![
            name(head).to_string(),
            format!("{}{}", dep.prefix(), name(body)),
        ];
        for (pred, step_dep) in steps {
            parts.push(format!("{}{}", step_dep.prefix(), name(pred)));
        }
        return Error::semantic(format!(
            "program is not stratifiable: recursion through negation or aggregation: {}",
            parts.join(" -> ")
        ));
    }
    unreachable!("stratification diverged, so a strict cycle must exist")
}

/// BFS over dependency edges from `start` to `target`; returns the traversed
/// predicates after `start`, each with the negation flag of the edge into it.
/// `Some(vec![])` when `start == target` (a self-loop needs no steps).
fn dependency_path(
    deps: &[Vec<(ir::PredId, Dep)>],
    start: ir::PredId,
    target: ir::PredId,
) -> Option<Vec<(ir::PredId, Dep)>> {
    if start == target {
        return Some(Vec::new());
    }
    let mut parent: Vec<Option<(ir::PredId, Dep)>> = vec![None; deps.len()];
    let mut visited = vec![false; deps.len()];
    visited[start.0 as usize] = true;
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(pred) = queue.pop_front() {
        for &(next, dep) in &deps[pred.0 as usize] {
            if visited[next.0 as usize] {
                continue;
            }
            visited[next.0 as usize] = true;
            parent[next.0 as usize] = Some((pred, dep));
            if next == target {
                let mut steps = Vec::new();
                let mut at = target;
                while at != start {
                    let (prev, edge_negated) = parent[at.0 as usize].expect("walked via parent");
                    steps.push((at, edge_negated));
                    at = prev;
                }
                steps.reverse();
                return Some(steps);
            }
            queue.push_back(next);
        }
    }
    None
}

/// The variable slots bound by positive body atoms.
fn positive_vars(body: &[ir::BodyLiteral]) -> std::collections::HashSet<u32> {
    let mut bound = std::collections::HashSet::new();
    for literal in body {
        if let ir::BodyLiteralKind::Atom(atom) = &literal.kind {
            for arg in &atom.args {
                if let ir::Term::Var(var) = arg {
                    bound.insert(var.0);
                }
            }
        }
    }
    bound
}

/// Records an unsafe-variable error: nothing in the body binds the variable —
/// no positive atom, no `=`-assignment, no aggregate result.
fn push_unsafe(
    errors: &mut Vec<Error>,
    slot: u32,
    var_names: &[Option<String>],
    place: &str,
    context: &str,
) {
    let name = var_names[slot as usize].as_deref().unwrap_or("_");
    errors.push(Error::semantic(format!(
        "unsafe {place} in `{context}`: variable `{name}` is never bound — it must occur in a \
         positive body atom, or be bound by an `=`-assignment or an aggregate result"
    )));
}

/// Every variable the body binds: positively bound (occurring in a positive
/// atom), plus everything the scheduled builtins bind — `=`-assignment targets
/// and aggregate results.
///
/// Delegates to [`schedule::schedule_body_with`] rather than walking source
/// order, so a rule head may reference a variable whose binder is written *after*
/// its use (`rev(X, M) :- a(X), M = N + 1, N = X + 1.`): the schedule finds the
/// order, so head safety must agree that `M` is bound. An unschedulable body
/// falls back to the positive variables — its own scheduling error is reported
/// separately, and this keeps the head check from piling on.
fn safe_bound_vars(body: &[ir::BodyLiteral]) -> std::collections::HashSet<u32> {
    match schedule::schedule_body_with(body, &std::collections::HashSet::new()) {
        Ok((_, bound)) => bound.iter().map(|var| var.0).collect(),
        Err(_) => positive_vars(body),
    }
}

/// Is `expr` the bare literal `absent`? Only a whole operand counts — `absent`
/// inside arithmetic is annihilation (§8), not a comparison mistake.
fn is_literal_absent(expr: &ir::Expr) -> bool {
    matches!(expr, ir::Expr::Term(ir::Term::Const(ir::Value::Absent)))
}

/// The source symbol of a comparison operator, for error messages.
fn cmp_symbol(op: ast::CmpOp) -> &'static str {
    match op {
        ast::CmpOp::Eq => "=",
        ast::CmpOp::Ne => "!=",
        ast::CmpOp::Lt => "<",
        ast::CmpOp::Le => "<=",
        ast::CmpOp::Gt => ">",
        ast::CmpOp::Ge => ">=",
    }
}

/// All variable slots occurring in an expression.
fn expr_vars(expr: &ir::Expr) -> Vec<ir::Var> {
    match expr {
        ir::Expr::Term(ir::Term::Var(var)) => vec![*var],
        ir::Expr::Term(ir::Term::Const(_)) => Vec::new(),
        ir::Expr::Binary { lhs, rhs, .. } => {
            let mut vars = expr_vars(lhs);
            vars.extend(expr_vars(rhs));
            vars
        }
    }
}

/// Post-lowering lint: flags every predicate that is *referenced* in a rule or
/// query body but never *defined* (no fact, no rule head, no import). Under
/// closed-world semantics such a predicate is simply the empty relation — valid,
/// so this is a [`Warning`], not an [`Error`] — but it is almost always a typo,
/// and its silent emptiness is indistinguishable from a legitimately-empty
/// answer. Each warning offers the nearest defined predicate name as a fix.
///
/// Warnings come out in referenced-predicate first-appearance order (rules
/// before queries, body order within each) — deterministic, matching the
/// crate's A7 stance.
pub fn check_program(program: &ir::Program) -> Vec<Warning> {
    // A predicate is "defined" if it can contribute tuples: an in-program fact,
    // a rule head (even one whose body never fires — definedness is syntactic,
    // not about the model), or an import.
    let mut defined = std::collections::HashSet::new();
    for fact in &program.facts {
        defined.insert(fact.pred.0);
    }
    for rule in &program.rules {
        defined.insert(rule.head.pred.0);
    }
    for import in &program.imports {
        defined.insert(import.pred.0);
    }

    // Referenced predicates in first-appearance order, deduplicated.
    let mut referenced = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for rule in &program.rules {
        collect_body_refs(&rule.body, &mut referenced, &mut seen);
    }
    for query in &program.queries {
        collect_body_refs(&query.body, &mut referenced, &mut seen);
    }

    referenced
        .into_iter()
        .filter(|pred| !defined.contains(&pred.0))
        .map(|pred| {
            let info = &program.predicates[pred.0 as usize];
            Warning::UndefinedPredicate {
                name: info.name.clone(),
                arity: info.arity,
                suggestion: nearest_defined(&info.name, info.arity, &defined, program),
            }
        })
        .collect()
}

/// Appends the predicate of every positive/negated body atom to `referenced`,
/// in body order, skipping ones already `seen`. Comparisons carry no predicate.
fn collect_body_refs(
    body: &[ir::BodyLiteral],
    referenced: &mut Vec<ir::PredId>,
    seen: &mut std::collections::HashSet<u32>,
) {
    for literal in body {
        let atom = match &literal.kind {
            ir::BodyLiteralKind::Atom(atom) | ir::BodyLiteralKind::NegAtom(atom) => atom,
            ir::BodyLiteralKind::Compare { .. } | ir::BodyLiteralKind::Presence { .. } => continue,
            // Predicates referenced only inside an aggregate goal still count (§9).
            ir::BodyLiteralKind::Aggregate { goal, .. } => {
                collect_body_refs(goal, referenced, seen);
                continue;
            }
        };
        if seen.insert(atom.pred.0) {
            referenced.push(atom.pred);
        }
    }
}

/// The nearest *defined* predicate name to `target` by Levenshtein distance,
/// offered only when within distance 2 (close enough to be a plausible typo).
/// Same-arity candidates are preferred over closer-but-wrong-arity ones only as
/// a tie-break on distance; ties beyond that break on name for determinism.
fn nearest_defined(
    target: &str,
    arity: u32,
    defined: &std::collections::HashSet<u32>,
    program: &ir::Program,
) -> Option<String> {
    defined
        .iter()
        .map(|&id| &program.predicates[id as usize])
        .filter_map(|info| {
            let distance = levenshtein(target, &info.name);
            (1..=2).contains(&distance).then_some((
                distance,
                info.arity != arity,
                info.name.as_str(),
            ))
        })
        .min()
        .map(|(_, _, name)| name.to_string())
}

/// Levenshtein edit distance over Unicode scalar values (two-row DP). Small
/// inputs (predicate names), so the quadratic table is not a concern.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Span;
    use crate::ast::fixtures as ast_fix;
    use crate::ir::fixtures as ir_fix;

    /// The contract test: lowering the hand-built §16.1 surface program
    /// produces exactly the hand-built §16.1 IR — interning order, variable
    /// numbering, fact/rule split, and the single stratum.
    #[test]
    fn lowering_16_1_matches_ir_fixture() {
        let lowered = lower(&ast_fix::example_16_1()).expect("16.1 lowers cleanly");
        assert_eq!(lowered, ir_fix::example_16_1());
    }

    #[test]
    fn arity_clash_is_reported() {
        let program = ast::Program {
            statements: vec![
                ast_fix::fact("p", vec![ast_fix::string_term("a")]),
                ast_fix::fact(
                    "p",
                    vec![ast_fix::string_term("a"), ast_fix::string_term("b")],
                ),
            ],
        };
        let errors = lower(&program).expect_err("arity clash");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("arity 2") && e.to_string().contains("arity 1")),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn unsafe_rule_is_reported() {
        // p(X) :- q(Y).
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("p", vec![ast_fix::var_term("X")]),
                vec![ast_fix::positive_literal(ast_fix::positional_atom(
                    "q",
                    vec![ast_fix::var_term("Y")],
                ))],
            )],
        };
        let errors = lower(&program).expect_err("unsafe rule");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("head variable `X`")),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn non_ground_fact_is_reported() {
        let program = ast::Program {
            statements: vec![ast_fix::fact("p", vec![ast_fix::var_term("X")])],
        };
        let errors = lower(&program).expect_err("non-ground fact");
        assert!(
            errors.iter().any(|e| e.to_string().contains("not ground")),
            "unexpected errors: {errors:?}"
        );
    }

    /// The contract test: lowering the hand-built §16.2 surface program
    /// produces exactly the hand-built §16.2 IR — the negated atom survives,
    /// its wildcard is a fresh `None`-named slot exempt from the safety check
    /// (existential under the negation, §7), and stratification yields the
    /// single stratum.
    #[test]
    fn lowering_16_2_matches_ir_fixture() {
        let lowered = lower(&ast_fix::example_16_2()).expect("16.2 lowers cleanly");
        assert_eq!(lowered, ir_fix::example_16_2());
    }

    /// A *named* variable under negation still needs a positive binder; only
    /// wildcard-fresh slots are exempt.
    #[test]
    fn named_var_only_in_negated_atom_is_unsafe() {
        // p(X) :- q(X), not r(Y).
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("p", vec![ast_fix::var_term("X")]),
                vec![
                    ast_fix::positive_literal(ast_fix::positional_atom(
                        "q",
                        vec![ast_fix::var_term("X")],
                    )),
                    ast_fix::negated_literal(ast_fix::positional_atom(
                        "r",
                        vec![ast_fix::var_term("Y")],
                    )),
                ],
            )],
        };
        let errors = lower(&program).expect_err("Y is unsafe");
        assert!(
            errors.iter().any(|e| {
                let msg = e.to_string();
                msg.contains("negated atom") && msg.contains("`Y`")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    /// Recursion through negation of the rule's own head predicate.
    #[test]
    fn self_negation_is_not_stratifiable() {
        // p(X) :- q(X), not p(X).
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("p", vec![ast_fix::var_term("X")]),
                vec![
                    ast_fix::positive_literal(ast_fix::positional_atom(
                        "q",
                        vec![ast_fix::var_term("X")],
                    )),
                    ast_fix::negated_literal(ast_fix::positional_atom(
                        "p",
                        vec![ast_fix::var_term("X")],
                    )),
                ],
            )],
        };
        let errors = lower(&program).expect_err("negative self-loop");
        assert!(
            errors.iter().any(|e| e.to_string().contains(
                "not stratifiable: recursion through negation or aggregation: p -> not p"
            )),
            "unexpected errors: {errors:?}"
        );
    }

    /// A two-predicate negative cycle; the error names the whole cycle.
    #[test]
    fn a_negative_cycle_is_reported_with_its_path() {
        // a(X) :- q(X), not b(X).    b(X) :- a(X).
        let program = ast::Program {
            statements: vec![
                ast_fix::rule(
                    ast_fix::positional_atom("a", vec![ast_fix::var_term("X")]),
                    vec![
                        ast_fix::positive_literal(ast_fix::positional_atom(
                            "q",
                            vec![ast_fix::var_term("X")],
                        )),
                        ast_fix::negated_literal(ast_fix::positional_atom(
                            "b",
                            vec![ast_fix::var_term("X")],
                        )),
                    ],
                ),
                ast_fix::rule(
                    ast_fix::positional_atom("b", vec![ast_fix::var_term("X")]),
                    vec![ast_fix::positive_literal(ast_fix::positional_atom(
                        "a",
                        vec![ast_fix::var_term("X")],
                    ))],
                ),
            ],
        };
        let errors = lower(&program).expect_err("negative cycle");
        assert!(
            errors.iter().any(|e| e
                .to_string()
                .contains("recursion through negation or aggregation: a -> not b -> a")),
            "unexpected errors: {errors:?}"
        );
    }

    /// A wildcard in a rule head is a fresh slot with no positive binder —
    /// unsafe, reported with the `_` fallback name.
    #[test]
    fn wildcard_in_a_rule_head_is_unsafe() {
        // p(_) :- q(X).
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("p", vec![ast_fix::wildcard_term()]),
                vec![ast_fix::positive_literal(ast_fix::positional_atom(
                    "q",
                    vec![ast_fix::var_term("X")],
                ))],
            )],
        };
        let errors = lower(&program).expect_err("head wildcard is unsafe");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("head variable `_`")),
            "unexpected errors: {errors:?}"
        );
    }

    /// A query with a negated literal lowers like a rule body: the `NegAtom`
    /// survives and the wildcard becomes a fresh `None`-named slot.
    #[test]
    fn a_negated_query_lowers_to_a_negatom_body() {
        // ?- person(X), not parent(_, X).
        let program = ast::Program {
            statements: vec![ast_fix::query(vec![
                ast_fix::positive_literal(ast_fix::positional_atom(
                    "person",
                    vec![ast_fix::var_term("X")],
                )),
                ast_fix::negated_literal(ast_fix::positional_atom(
                    "parent",
                    vec![ast_fix::wildcard_term(), ast_fix::var_term("X")],
                )),
            ])],
        };
        let lowered = lower(&program).expect("negated query lowers");
        assert_eq!(
            lowered.queries,
            vec![ir::Query {
                body: vec![
                    ir::BodyLiteral {
                        kind: ir::BodyLiteralKind::Atom(ir::Atom {
                            pred: ir::PredId(0),
                            args: vec![ir::Term::Var(ir::Var(0))],
                        }),
                        span: Span::DUMMY,
                    },
                    ir::BodyLiteral {
                        kind: ir::BodyLiteralKind::NegAtom(ir::Atom {
                            pred: ir::PredId(1),
                            args: vec![ir::Term::Var(ir::Var(1)), ir::Term::Var(ir::Var(0))],
                        }),
                        span: Span::DUMMY,
                    },
                ],
                var_names: vec![Some("X".to_string()), None],
                projection: vec![0],
                span: Span::DUMMY,
            }]
        );
    }

    /// The §10 safety rule applies to query bodies with the `query` context
    /// in the message — the negated-atom named-variable check included.
    #[test]
    fn named_var_only_in_a_negated_query_atom_is_unsafe() {
        // ?- q(X), not r(Y).
        let program = ast::Program {
            statements: vec![ast_fix::query(vec![
                ast_fix::positive_literal(ast_fix::positional_atom(
                    "q",
                    vec![ast_fix::var_term("X")],
                )),
                ast_fix::negated_literal(ast_fix::positional_atom(
                    "r",
                    vec![ast_fix::var_term("Y")],
                )),
            ])],
        };
        let errors = lower(&program).expect_err("Y is unsafe in the query");
        assert!(
            errors.iter().any(|e| {
                let msg = e.to_string();
                msg.contains("query") && msg.contains("`Y`")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    /// A chain of two negations forces three strata.
    #[test]
    fn a_negation_chain_yields_three_strata() {
        // a(X) :- d(X).    b(X) :- d(X), not a(X).    c(X) :- d(X), not b(X).
        let unary_rule = |head: &str, body: Vec<ast::Literal>| {
            ast_fix::rule(
                ast_fix::positional_atom(head, vec![ast_fix::var_term("X")]),
                body,
            )
        };
        let positive = |pred: &str| {
            ast_fix::positive_literal(ast_fix::positional_atom(pred, vec![ast_fix::var_term("X")]))
        };
        let negative = |pred: &str| {
            ast_fix::negated_literal(ast_fix::positional_atom(pred, vec![ast_fix::var_term("X")]))
        };
        let program = ast::Program {
            statements: vec![
                unary_rule("a", vec![positive("d")]),
                unary_rule("b", vec![positive("d"), negative("a")]),
                unary_rule("c", vec![positive("d"), negative("b")]),
            ],
        };
        let lowered = lower(&program).expect("stratifiable");
        assert_eq!(
            lowered.strata,
            vec![
                vec![ir::RuleId(0)],
                vec![ir::RuleId(1)],
                vec![ir::RuleId(2)],
            ]
        );
    }

    /// Negation over an IDB predicate forces a second stratum; rules keep
    /// source order within each stratum.
    #[test]
    fn negation_over_idb_yields_two_strata_in_order() {
        // p(X) :- q(X).    r(X) :- q(X), not p(X).    s(X) :- q(X).
        // p and s are stratum 0 (source order preserved); r is stratum 1.
        let program = ast::Program {
            statements: vec![
                ast_fix::rule(
                    ast_fix::positional_atom("p", vec![ast_fix::var_term("X")]),
                    vec![ast_fix::positive_literal(ast_fix::positional_atom(
                        "q",
                        vec![ast_fix::var_term("X")],
                    ))],
                ),
                ast_fix::rule(
                    ast_fix::positional_atom("r", vec![ast_fix::var_term("X")]),
                    vec![
                        ast_fix::positive_literal(ast_fix::positional_atom(
                            "q",
                            vec![ast_fix::var_term("X")],
                        )),
                        ast_fix::negated_literal(ast_fix::positional_atom(
                            "p",
                            vec![ast_fix::var_term("X")],
                        )),
                    ],
                ),
                ast_fix::rule(
                    ast_fix::positional_atom("s", vec![ast_fix::var_term("X")]),
                    vec![ast_fix::positive_literal(ast_fix::positional_atom(
                        "q",
                        vec![ast_fix::var_term("X")],
                    ))],
                ),
            ],
        };
        let lowered = lower(&program).expect("stratifiable");
        assert_eq!(
            lowered.strata,
            vec![vec![ir::RuleId(0), ir::RuleId(2)], vec![ir::RuleId(1)]]
        );
    }

    // --- Named-argument lowering (pass 2) ---

    /// A `declare person(name, age).` statement, the schema most of the
    /// named-argument tests below resolve against.
    fn declare_person() -> ast::Statement {
        ast_fix::declare(
            "person",
            vec![
                ast_fix::field_decl("name", Some(ast::TypeName::String)),
                ast_fix::field_decl("age", Some(ast::TypeName::Int)),
            ],
        )
    }

    /// The contract test: lowering the hand-built §16.7 surface program
    /// produces exactly the hand-built §16.7 IR — field-to-position mapping,
    /// partial selection into fresh slots, and both schema origins.
    #[test]
    fn lowering_16_7_matches_ir_fixture() {
        let lowered = lower(&ast_fix::example_16_7()).expect("16.7 lowers cleanly");
        assert_eq!(lowered, ir_fix::example_16_7());
    }

    /// The invariant behind the whole feature: a named literal and the
    /// positional literal it denotes lower to identical IR.
    #[test]
    fn named_and_positional_forms_lower_identically() {
        // adult(N) :- person(name: N, age: A).
        let named = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "person",
                        vec![
                            ast_fix::named_arg("name", ast_fix::var_term("N")),
                            ast_fix::named_arg("age", ast_fix::var_term("A")),
                        ],
                    ))],
                ),
            ],
        };
        // adult(N) :- person(N, A).
        let positional = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::positional_atom(
                        "person",
                        vec![ast_fix::var_term("N"), ast_fix::var_term("A")],
                    ))],
                ),
            ],
        };

        assert_eq!(
            lower(&named).expect("named form lowers"),
            lower(&positional).expect("positional form lowers")
        );
    }

    /// Argument order inside a named literal is irrelevant — position comes
    /// from the schema, not from how the literal was written (§4).
    #[test]
    fn named_argument_order_is_irrelevant() {
        let in_order = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "person",
                        vec![
                            ast_fix::named_arg("name", ast_fix::var_term("N")),
                            ast_fix::named_arg("age", ast_fix::var_term("A")),
                        ],
                    ))],
                ),
            ],
        };
        let reversed = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "person",
                        vec![
                            ast_fix::named_arg("age", ast_fix::var_term("A")),
                            ast_fix::named_arg("name", ast_fix::var_term("N")),
                        ],
                    ))],
                ),
            ],
        };

        // Variable *numbering* differs (slots follow written order), so compare
        // the resolved argument positions rather than whole programs.
        let a = lower(&in_order).expect("lowers");
        let b = lower(&reversed).expect("lowers");
        let args_of = |program: &ir::Program| match &program.rules[0].body[0].kind {
            ir::BodyLiteralKind::Atom(atom) => atom.args.clone(),
            other => panic!("expected a positive atom, got {other:?}"),
        };
        let name_slot = |program: &ir::Program| {
            program.rules[0]
                .var_names
                .iter()
                .position(|n| n.as_deref() == Some("N"))
                .expect("N is numbered") as u32
        };
        // In both programs, position 0 holds N and position 1 holds A.
        assert_eq!(args_of(&a)[0], ir::Term::Var(ir::Var(name_slot(&a))));
        assert_eq!(args_of(&b)[0], ir::Term::Var(ir::Var(name_slot(&b))));
    }

    /// Partial selection over a wide relation: omitted fields become distinct
    /// fresh slots, and the atom still carries the predicate's full arity.
    #[test]
    fn partial_selection_fills_omitted_fields_with_fresh_slots() {
        let program = ast::Program {
            statements: vec![
                ast_fix::declare(
                    "employee",
                    vec![
                        ast_fix::field_decl("id", None),
                        ast_fix::field_decl("name", None),
                        ast_fix::field_decl("dept", None),
                        ast_fix::field_decl("title", None),
                    ],
                ),
                ast_fix::rule(
                    ast_fix::positional_atom("manager_name", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "employee",
                        vec![
                            ast_fix::named_arg("name", ast_fix::var_term("N")),
                            ast_fix::named_arg("title", ast_fix::string_term("manager")),
                        ],
                    ))],
                ),
            ],
        };

        let lowered = lower(&program).expect("partial selection lowers");
        let rule = &lowered.rules[0];
        let args = match &rule.body[0].kind {
            ir::BodyLiteralKind::Atom(atom) => &atom.args,
            other => panic!("expected a positive atom, got {other:?}"),
        };
        assert_eq!(args.len(), 4, "atom carries full arity");
        assert_eq!(
            args[3],
            ir::Term::Const(ir::Value::String("manager".into()))
        );

        // id and dept are fresh, unnamed, and distinct from each other.
        let fresh: Vec<ir::Var> = [0, 2]
            .iter()
            .map(|&i| match args[i] {
                ir::Term::Var(var) => var,
                ref other => panic!("expected a fresh variable, got {other:?}"),
            })
            .collect();
        assert_ne!(fresh[0], fresh[1]);
        for var in fresh {
            assert_eq!(rule.var_names[var.0 as usize], None);
        }
    }

    /// Schemas are collected over the whole program in pass 1, so a `declare`
    /// may follow the rule that uses the named form.
    #[test]
    fn declare_may_follow_its_use() {
        let program = ast::Program {
            statements: vec![
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "person",
                        vec![ast_fix::named_arg("name", ast_fix::var_term("N"))],
                    ))],
                ),
                declare_person(),
            ],
        };
        lower(&program).expect("declaration order does not matter");
    }

    /// Field names survive lowering onto the interned table (§17, 2026-07-20),
    /// so type inference and provenance can name columns without the AST.
    #[test]
    fn field_names_reach_the_ir() {
        let lowered = lower(&ast_fix::example_16_7()).expect("16.7 lowers cleanly");

        let employee = lowered
            .predicates
            .iter()
            .find(|p| p.name == "employee")
            .expect("employee is interned");
        assert_eq!(
            employee.fields.as_deref(),
            Some(
                [
                    "id",
                    "name",
                    "age",
                    "dept",
                    "title",
                    "salary",
                    "city",
                    "start_date"
                ]
                .map(String::from)
                .as_slice()
            )
        );

        // Never declared, so no field names — and the `None` case must stay
        // representable rather than degrading into empty names.
        let manager_name = lowered
            .predicates
            .iter()
            .find(|p| p.name == "manager_name")
            .expect("manager_name is interned");
        assert_eq!(manager_name.fields, None);

        // The invariant every consumer relies on.
        for info in &lowered.predicates {
            if let Some(fields) = &info.fields {
                assert_eq!(fields.len(), info.arity as usize, "for `{}`", info.name);
            }
            // Declared types are Some exactly when field names are, same length.
            assert_eq!(
                info.fields.is_some(),
                info.field_types.is_some(),
                "for `{}`",
                info.name
            );
            if let Some(types) = &info.field_types {
                assert_eq!(types.len(), info.arity as usize, "for `{}`", info.name);
            }
        }
    }

    /// Declared column types survive lowering onto `PredicateInfo.field_types`,
    /// from both a `declare` (`person`) and an explicit import schema
    /// (`employee`); a predicate with no schema carries none.
    #[test]
    fn declared_types_reach_the_ir() {
        let lowered = lower(&ast_fix::example_16_7()).expect("16.7 lowers cleanly");

        let person = lowered
            .predicates
            .iter()
            .find(|p| p.name == "person")
            .expect("person is interned");
        assert_eq!(
            person.field_types.as_deref(),
            Some([Some(ast::TypeName::String), Some(ast::TypeName::Int)].as_slice())
        );

        let employee = lowered
            .predicates
            .iter()
            .find(|p| p.name == "employee")
            .expect("employee is interned");
        assert_eq!(
            employee.field_types.as_deref(),
            Some(
                [
                    Some(ast::TypeName::Int),
                    Some(ast::TypeName::String),
                    Some(ast::TypeName::Int),
                    Some(ast::TypeName::String),
                    Some(ast::TypeName::String),
                    Some(ast::TypeName::Int),
                    Some(ast::TypeName::String),
                    Some(ast::TypeName::String),
                ]
                .as_slice()
            )
        );

        let manager_name = lowered
            .predicates
            .iter()
            .find(|p| p.name == "manager_name")
            .expect("manager_name is interned");
        assert_eq!(manager_name.field_types, None);
    }

    /// Schemas are collected program-wide, so a late `declare` still lands in
    /// the IR — the same visibility rule as `declare_may_follow_its_use`, now
    /// observable downstream.
    #[test]
    fn a_late_declare_still_reaches_the_ir() {
        let program = ast::Program {
            statements: vec![
                ast_fix::fact(
                    "person",
                    vec![ast_fix::string_term("alice"), ast_fix::int_term(30)],
                ),
                declare_person(),
            ],
        };
        let lowered = lower(&program).expect("lowers");
        assert_eq!(
            lowered.predicates[0].fields.as_deref(),
            Some(["name".to_string(), "age".to_string()].as_slice())
        );
    }

    /// When a schema disagrees with the interned arity the clash is reported
    /// and *no* schema is attached, so `PredicateInfo`'s length invariant holds
    /// even on the error path.
    ///
    /// `lower()` returns `Err` here and never yields the table, so this drives
    /// the two passes directly — the only place the guard is observable.
    #[test]
    fn a_schema_conflicting_with_arity_is_not_attached() {
        // Use site first (arity 3), then a two-field declare.
        let program = ast::Program {
            statements: vec![
                ast_fix::fact(
                    "person",
                    vec![
                        ast_fix::string_term("alice"),
                        ast_fix::int_term(30),
                        ast_fix::string_term("eng"),
                    ],
                ),
                declare_person(),
            ],
        };

        let mut lowerer = Lowerer::default();
        lowerer.collect_predicates(&program, &[]);
        lowerer.attach_field_names();

        let person = &lowerer.predicates[0];
        assert_eq!(person.name, "person");
        assert_eq!(person.arity, 3, "arity comes from the first use site");
        assert_eq!(
            person.fields, None,
            "a two-field schema must not attach to an arity-3 predicate"
        );
        assert!(
            lowerer
                .errors
                .iter()
                .any(|e| e.to_string().contains("arity")),
            "unexpected errors: {:?}",
            lowerer.errors
        );
    }

    /// A schema-less import never used in a clause is first interned during
    /// pass 2 (`pred_id`), after the schema pass has run — the finished
    /// program's predicate table must still contain it, or its `ImportSpec`
    /// would hold a dangling `PredId`.
    #[test]
    fn a_schema_less_import_alone_still_reaches_the_predicate_table() {
        let program = ast::Program {
            statements: vec![ast::Statement {
                kind: ast::StatementKind::Import(ast::Import {
                    path: "data/employees.csv".to_string(),
                    path_span: Span::DUMMY,
                    kind: ast::ImportKind::Data {
                        table: None,
                        relation: ast_fix::ident("employee"),
                        schema: None,
                    },
                }),
                span: Span::DUMMY,
            }],
        };
        let lowered = lower(&program).expect("a lone schema-less import lowers");

        assert_eq!(lowered.imports.len(), 1);
        let info = &lowered.predicates[lowered.imports[0].pred.0 as usize];
        assert_eq!(info.name, "employee");
        assert_eq!(info.arity, 0, "arity is pending source load (§13)");
        assert_eq!(info.fields, None);
    }

    #[test]
    fn unknown_field_is_reported() {
        let program = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "person",
                        vec![ast_fix::named_arg("nickname", ast_fix::var_term("N"))],
                    ))],
                ),
            ],
        };
        let errors = lower(&program).expect_err("unknown field");
        assert!(
            errors.iter().any(|e| {
                let text = e.to_string();
                text.contains("unknown field `nickname`") && text.contains("name, age")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn duplicate_field_in_one_literal_is_reported() {
        let program = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::rule(
                    ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                    vec![ast_fix::positive_literal(ast_fix::named_atom(
                        "person",
                        vec![
                            ast_fix::named_arg("name", ast_fix::var_term("N")),
                            ast_fix::named_arg("name", ast_fix::var_term("M")),
                        ],
                    ))],
                ),
            ],
        };
        let errors = lower(&program).expect_err("duplicate field");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("`name` is given twice")),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn named_arguments_without_a_schema_are_reported() {
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("adult", vec![ast_fix::var_term("N")]),
                vec![ast_fix::positive_literal(ast_fix::named_atom(
                    "person",
                    vec![ast_fix::named_arg("name", ast_fix::var_term("N"))],
                ))],
            )],
        };
        let errors = lower(&program).expect_err("no schema");
        assert!(
            errors.iter().any(|e| {
                let text = e.to_string();
                text.contains("require known field names for `person`")
                    && text.contains("declare person(...)")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    /// §4: a head cannot leave columns unbound, so the named form must supply
    /// every field there.
    #[test]
    fn partial_selection_in_a_rule_head_is_reported() {
        let program = ast::Program {
            statements: vec![
                declare_person(),
                // person(name: N) :- adult(N).   -- `age` left unbound
                ast_fix::rule(
                    ast_fix::named_atom(
                        "person",
                        vec![ast_fix::named_arg("name", ast_fix::var_term("N"))],
                    ),
                    vec![ast_fix::positive_literal(ast_fix::positional_atom(
                        "adult",
                        vec![ast_fix::var_term("N")],
                    ))],
                ),
            ],
        };
        let errors = lower(&program).expect_err("partial head");
        assert!(
            errors.iter().any(|e| {
                let text = e.to_string();
                text.contains("must supply every field") && text.contains("missing: age")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    /// The same rule for facts — which, before the grounding path was checked
    /// against the lowered head, silently produced no fact and no error.
    #[test]
    fn partial_selection_in_a_fact_is_reported() {
        let program = ast::Program {
            statements: vec![
                declare_person(),
                ast::Statement {
                    kind: ast::StatementKind::Clause(ast::Clause {
                        head: ast_fix::named_atom(
                            "person",
                            vec![ast_fix::named_arg("name", ast_fix::string_term("alice"))],
                        ),
                        body: Vec::new(),
                        span: Span::DUMMY,
                    }),
                    span: Span::DUMMY,
                },
            ],
        };
        let errors = lower(&program).expect_err("partial fact");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("must supply every field")),
            "unexpected errors: {errors:?}"
        );
    }

    /// A non-ground named fact names the offending *field*, not an index.
    #[test]
    fn non_ground_named_fact_reports_the_field_name() {
        let program = ast::Program {
            statements: vec![
                declare_person(),
                ast::Statement {
                    kind: ast::StatementKind::Clause(ast::Clause {
                        head: ast_fix::named_atom(
                            "person",
                            vec![
                                ast_fix::named_arg("name", ast_fix::string_term("alice")),
                                ast_fix::named_arg("age", ast_fix::var_term("A")),
                            ],
                        ),
                        body: Vec::new(),
                        span: Span::DUMMY,
                    }),
                    span: Span::DUMMY,
                },
            ],
        };
        let errors = lower(&program).expect_err("non-ground named fact");
        assert!(
            errors.iter().any(|e| {
                let text = e.to_string();
                text.contains("not ground") && text.contains("field `age`")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn conflicting_schemas_are_reported() {
        let program = ast::Program {
            statements: vec![
                declare_person(),
                ast_fix::declare(
                    "person",
                    vec![
                        ast_fix::field_decl("name", None),
                        ast_fix::field_decl("years", None),
                    ],
                ),
            ],
        };
        let errors = lower(&program).expect_err("conflicting schemas");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("conflicting schemas for `person`")),
            "unexpected errors: {errors:?}"
        );
    }

    /// Two schemas agreeing on field names but disagreeing on declared types
    /// are a conflict naming both origins (§17), just like a name mismatch.
    #[test]
    fn schemas_conflicting_only_on_types_are_reported() {
        let program = ast::Program {
            statements: vec![
                // declare person(name: string, age: int).
                declare_person(),
                // declare person(name: string, age: string).  -- age type differs
                ast_fix::declare(
                    "person",
                    vec![
                        ast_fix::field_decl("name", Some(ast::TypeName::String)),
                        ast_fix::field_decl("age", Some(ast::TypeName::String)),
                    ],
                ),
            ],
        };
        let errors = lower(&program).expect_err("conflicting declared types");
        assert!(
            errors.iter().any(|e| e
                .to_string()
                .contains("conflicting declared types for `person`")),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn duplicate_field_within_a_schema_is_reported() {
        let program = ast::Program {
            statements: vec![ast_fix::declare(
                "person",
                vec![
                    ast_fix::field_decl("name", None),
                    ast_fix::field_decl("name", None),
                ],
            )],
        };
        let errors = lower(&program).expect_err("duplicate schema field");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("duplicate field `name`")),
            "unexpected errors: {errors:?}"
        );
    }

    // --- Phase A properties A6–A12 (testing.md) ---

    mod properties {
        use proptest::prelude::*;

        use super::super::lower;
        use crate::ast;
        use crate::ir;
        use crate::testgen::{arb_defect, arb_safe_program, inject_defect};

        /// The AST rule statements, in source order (the generator emits
        /// facts first, then rules — mirrored by `ir::Program::rules`).
        fn ast_rules(program: &ast::Program) -> Vec<&ast::Clause> {
            program
                .statements
                .iter()
                .filter_map(|s| match &s.kind {
                    ast::StatementKind::Clause(c) if !c.body.is_empty() => Some(c),
                    _ => None,
                })
                .collect()
        }

        fn ast_fact_count(program: &ast::Program) -> usize {
            program
                .statements
                .iter()
                .filter(|s| matches!(&s.kind, ast::StatementKind::Clause(c) if c.body.is_empty()))
                .count()
        }

        /// Every slot referenced by a rule's head and body atoms.
        fn referenced_slots(rule: &ir::Rule) -> std::collections::HashSet<u32> {
            let mut slots = std::collections::HashSet::new();
            let visit_atom = |atom: &ir::Atom, slots: &mut std::collections::HashSet<u32>| {
                for arg in &atom.args {
                    if let ir::Term::Var(var) = arg {
                        slots.insert(var.0);
                    }
                }
            };
            visit_atom(&rule.head, &mut slots);
            for literal in &rule.body {
                match &literal.kind {
                    ir::BodyLiteralKind::Atom(atom) | ir::BodyLiteralKind::NegAtom(atom) => {
                        visit_atom(atom, &mut slots);
                    }
                    // This generator emits no aggregates.
                    ir::BodyLiteralKind::Compare { .. }
                    | ir::BodyLiteralKind::Presence { .. }
                    | ir::BodyLiteralKind::Aggregate { .. } => {}
                }
            }
            slots
        }

        proptest! {
            /// A6 (no panic), A7 (determinism), A8 (dense numbering),
            /// A9 (body order preserved), A10 (interning closed),
            /// A12 (fact/rule split + single stratum) over safe-by-construction
            /// programs, which must always lower successfully (A11 accept side).
            #[test]
            fn a6_to_a12_safe_programs_lower_correctly(program in arb_safe_program()) {
                let first = lower(&program); // A6: no panic
                let second = lower(&program);
                prop_assert_eq!(&first, &second); // A7

                let lowered = match first {
                    Ok(p) => p,
                    Err(errors) => {
                        return Err(TestCaseError::fail(format!(
                            "safe-by-construction program failed to lower: {errors:?}"
                        )));
                    }
                };

                // A12: fact/rule split; strata partition the rules in source
                // order within each stratum, and a *positive* program is
                // exactly one stratum (negation may or may not force more).
                prop_assert_eq!(lowered.facts.len(), ast_fact_count(&program));
                let source_rules = ast_rules(&program);
                prop_assert_eq!(lowered.rules.len(), source_rules.len());
                let mut covered: Vec<ir::RuleId> =
                    lowered.strata.iter().flatten().copied().collect();
                covered.sort();
                let all: Vec<ir::RuleId> =
                    (0..lowered.rules.len() as u32).map(ir::RuleId).collect();
                prop_assert_eq!(&covered, &all, "strata must partition the rules");
                for stratum in &lowered.strata {
                    prop_assert!(!stratum.is_empty(), "empty strata are dropped");
                    prop_assert!(
                        stratum.windows(2).all(|w| w[0] < w[1]),
                        "source order within a stratum"
                    );
                }
                let has_negation = lowered.rules.iter().any(|rule| {
                    rule.body
                        .iter()
                        .any(|l| matches!(l.kind, ir::BodyLiteralKind::NegAtom(_)))
                });
                if !has_negation && !lowered.rules.is_empty() {
                    prop_assert_eq!(lowered.strata.len(), 1, "positive => single stratum");
                }

                for (rule, source) in lowered.rules.iter().zip(&source_rules) {
                    // A9: body preserved 1:1 in order, kind for kind.
                    prop_assert_eq!(rule.body.len(), source.body.len());
                    for (literal, source_literal) in rule.body.iter().zip(&source.body) {
                        let ast::LiteralKind::Atom { negated, .. } = &source_literal.kind
                        else {
                            return Err(TestCaseError::fail(
                                "generator emits only atom literals".to_string(),
                            ));
                        };
                        match &literal.kind {
                            ir::BodyLiteralKind::Atom(_) => prop_assert!(!negated),
                            ir::BodyLiteralKind::NegAtom(_) => prop_assert!(*negated),
                            ir::BodyLiteralKind::Compare { .. }
                            | ir::BodyLiteralKind::Presence { .. }
                            | ir::BodyLiteralKind::Aggregate { .. } => {
                                return Err(TestCaseError::fail(
                                    "unexpected comparison, presence test, or aggregate"
                                        .to_string(),
                                ));
                            }
                        }
                    }

                    // A8: numbering is dense — referenced slots are exactly
                    // 0..var_names.len(), and named variables are distinct.
                    let slots = referenced_slots(rule);
                    prop_assert_eq!(slots.len(), rule.var_names.len());
                    for slot in 0..rule.var_names.len() as u32 {
                        prop_assert!(slots.contains(&slot));
                    }
                    let mut names: Vec<&String> =
                        rule.var_names.iter().flatten().collect();
                    let total = names.len();
                    names.sort();
                    names.dedup();
                    prop_assert_eq!(names.len(), total);
                }

                // A10: interning is closed and atoms are at full arity.
                let check_atom = |atom: &ir::Atom| {
                    atom.pred.0 < lowered.predicates.len() as u32
                        && atom.args.len()
                            == lowered.predicates[atom.pred.0 as usize].arity as usize
                };
                for fact in &lowered.facts {
                    prop_assert!(fact.pred.0 < lowered.predicates.len() as u32);
                    prop_assert_eq!(
                        fact.tuple.0.len(),
                        lowered.predicates[fact.pred.0 as usize].arity as usize
                    );
                }
                for rule in &lowered.rules {
                    prop_assert!(check_atom(&rule.head));
                    for literal in &rule.body {
                        if let ir::BodyLiteralKind::Atom(atom)
                        | ir::BodyLiteralKind::NegAtom(atom) = &literal.kind
                        {
                            prop_assert!(check_atom(atom));
                        }
                    }
                }
            }

            /// A11 (reject side): one injected defect always fails lowering
            /// with the matching error kind, and never panics (A6).
            #[test]
            fn a11_injected_defects_are_rejected(
                program in arb_safe_program(),
                defect in arb_defect(),
            ) {
                let defective = inject_defect(program, defect);
                let errors = match lower(&defective) {
                    Err(errors) => errors,
                    Ok(_) => {
                        return Err(TestCaseError::fail(format!(
                            "program with injected {defect:?} lowered successfully"
                        )));
                    }
                };
                prop_assert!(
                    errors
                        .iter()
                        .any(|e| e.to_string().contains(defect.expected_error())),
                    "expected an error containing {:?}, got: {errors:?}",
                    defect.expected_error()
                );
            }

            /// A13: named arguments are invisible to the IR. Rewriting every
            /// named literal into the positional literal it denotes — fields at
            /// their schema positions, omitted fields as `_` — lowers to
            /// structurally identical IR.
            #[test]
            fn a13_named_and_positional_forms_agree(program in arb_safe_program()) {
                let named = lower(&program);
                let positional = lower(&crate::testgen::positionalize(&program));
                match (named, positional) {
                    (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                    (Err(a), Err(b)) => prop_assert_eq!(
                        a.len(), b.len(),
                        "the two forms disagreed on how many errors to report"
                    ),
                    (a, b) => {
                        return Err(TestCaseError::fail(format!(
                            "named and positional forms disagreed: {a:?} vs {b:?}"
                        )));
                    }
                }
            }

            /// A14: field names attach to exactly the predicates the program
            /// gives a schema, and match it in order — so `Some(f)` implies
            /// `f.len() == arity` for every predicate of every safe program.
            #[test]
            fn a14_field_names_attach_exactly_to_schema_predicates(
                program in arb_safe_program()
            ) {
                let mut schemas = std::collections::HashMap::new();
                for statement in &program.statements {
                    let (relation, fields) = match &statement.kind {
                        ast::StatementKind::Declare(d) => (&d.relation, &d.fields),
                        ast::StatementKind::Import(i) => match &i.kind {
                            ast::ImportKind::Data {
                                relation,
                                schema: Some(schema),
                                ..
                            } => (relation, schema),
                            _ => continue,
                        },
                        _ => continue,
                    };
                    let names: Vec<String> =
                        fields.iter().map(|f| f.name.name.clone()).collect();
                    schemas.insert(relation.name.clone(), names);
                }

                let lowered = lower(&program).expect("safe programs lower");
                for info in &lowered.predicates {
                    prop_assert_eq!(
                        info.fields.as_ref(),
                        schemas.get(&info.name),
                        "for `{}`", &info.name
                    );
                    if let Some(fields) = &info.fields {
                        prop_assert_eq!(fields.len(), info.arity as usize, "for `{}`", &info.name);
                    }
                }
            }

            /// C1 — stratification correctness, checked against a dependency
            /// graph this test recomputes from the lowered rules: every
            /// negated dependency's defining rules sit in a strictly lower
            /// stratum, every positive dependency's in the same or lower.
            /// (The reject side — negative cycles — is C1's other half,
            /// covered by A11's `NegativeCycle` defect.)
            #[test]
            fn c1_negated_dependencies_sit_strictly_lower(program in arb_safe_program()) {
                let lowered = lower(&program).expect("safe programs lower");
                // Predicate -> the stratum indices of its defining rules.
                let mut defined_in: Vec<Vec<usize>> =
                    vec![Vec::new(); lowered.predicates.len()];
                for (level, stratum) in lowered.strata.iter().enumerate() {
                    for &rule_id in stratum {
                        let head = lowered.rules[rule_id.0 as usize].head.pred;
                        defined_in[head.0 as usize].push(level);
                    }
                }
                for (level, stratum) in lowered.strata.iter().enumerate() {
                    for &rule_id in stratum {
                        for literal in &lowered.rules[rule_id.0 as usize].body {
                            match &literal.kind {
                                ir::BodyLiteralKind::NegAtom(atom) => {
                                    prop_assert!(
                                        defined_in[atom.pred.0 as usize]
                                            .iter()
                                            .all(|&def| def < level),
                                        "negated dependency not strictly lower"
                                    );
                                }
                                ir::BodyLiteralKind::Atom(atom) => {
                                    prop_assert!(
                                        defined_in[atom.pred.0 as usize]
                                            .iter()
                                            .all(|&def| def <= level),
                                        "positive dependency above its reader"
                                    );
                                }
                                // This generator emits no aggregates.
                                ir::BodyLiteralKind::Compare { .. }
                                | ir::BodyLiteralKind::Presence { .. }
                                | ir::BodyLiteralKind::Aggregate { .. } => {}
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn comparison_only_variable_is_unsafe() {
        // p(X) :- q(X), Y > 1.
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("p", vec![ast_fix::var_term("X")]),
                vec![
                    ast_fix::positive_literal(ast_fix::positional_atom(
                        "q",
                        vec![ast_fix::var_term("X")],
                    )),
                    ast::Literal {
                        kind: ast::LiteralKind::Comparison(ast::Comparison {
                            op: ast::CmpOp::Gt,
                            lhs: ast::Expr {
                                kind: ast::ExprKind::Term(ast_fix::var_term("Y")),
                                span: Span::DUMMY,
                            },
                            rhs: ast::Expr {
                                kind: ast::ExprKind::Term(ast::Term {
                                    kind: ast::TermKind::Constant(ast::Constant::Int(1)),
                                    span: Span::DUMMY,
                                }),
                                span: Span::DUMMY,
                            },
                        }),
                        span: Span::DUMMY,
                    },
                ],
            )],
        };
        let errors = lower(&program).expect_err("comparison-only var");
        assert!(
            errors.iter().any(|e| e.to_string().contains("`Y`")),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn assignment_target_is_safe_and_lowers() {
        // next_year(X, N) :- age(X, A), N = A + 1.
        // `N` is bound only by the `=`-assignment, yet is a head variable and a
        // comparison operand — it must be accepted (spec §17, 2026-07-21), and
        // the program must evaluate to next_year("alice", 31).
        let program = ast::Program {
            statements: vec![
                ast_fix::fact(
                    "age",
                    vec![ast_fix::string_term("alice"), ast_fix::int_term(30)],
                ),
                ast_fix::rule(
                    ast_fix::positional_atom(
                        "next_year",
                        vec![ast_fix::var_term("X"), ast_fix::var_term("N")],
                    ),
                    vec![
                        ast_fix::positive_literal(ast_fix::positional_atom(
                            "age",
                            vec![ast_fix::var_term("X"), ast_fix::var_term("A")],
                        )),
                        ast::Literal {
                            kind: ast::LiteralKind::Comparison(ast::Comparison {
                                op: ast::CmpOp::Eq,
                                lhs: ast::Expr {
                                    kind: ast::ExprKind::Term(ast_fix::var_term("N")),
                                    span: Span::DUMMY,
                                },
                                rhs: ast::Expr {
                                    kind: ast::ExprKind::Binary {
                                        op: ast::ArithOp::Add,
                                        lhs: Box::new(ast::Expr {
                                            kind: ast::ExprKind::Term(ast_fix::var_term("A")),
                                            span: Span::DUMMY,
                                        }),
                                        rhs: Box::new(ast::Expr {
                                            kind: ast::ExprKind::Term(ast_fix::int_term(1)),
                                            span: Span::DUMMY,
                                        }),
                                    },
                                    span: Span::DUMMY,
                                },
                            }),
                            span: Span::DUMMY,
                        },
                    ],
                ),
            ],
        };
        let ir = lower(&program).expect("assignment rule should lower");
        let model = crate::engine::eval(&ir).unwrap();
        let next_year = ir::PredId(
            ir.predicates
                .iter()
                .position(|p| p.name == "next_year")
                .expect("next_year predicate") as u32,
        );
        let expected: std::collections::BTreeSet<ir::Tuple> = std::iter::once(ir::Tuple(vec![
            ir::Value::String("alice".to_string()),
            ir::Value::Int(31),
        ]))
        .collect();
        assert_eq!(model.relation(next_year), &expected);
    }

    // --- Body scheduling (§8/§9/§10, `crate::schedule`) ---

    /// Parses and lowers `src`, returning the lowering errors.
    fn lower_errors(src: &str) -> Vec<Error> {
        let ast = crate::parser::parse(src).expect("parses");
        lower(&ast).expect_err("lowering should fail")
    }

    /// A body is a conjunction, so where a binder is *written* must not change
    /// what the clause means. A variable used both inside an aggregate and
    /// outside it is a group key; the scheduler runs whatever binds it first,
    /// whether that sits before or after the aggregate in source order.
    ///
    /// Before scheduling, the second form silently enumerated `Y` as a
    /// goal-local existential and counted everything.
    #[test]
    fn a_group_key_is_grouped_wherever_its_binder_is_written() {
        let facts = "q(1). q(2).\nr(2, \"a\"). r(3, \"b\"). r(3, \"c\").\n";
        let before =
            format!("{facts}g(X, N) :- q(X), Y = X + 1, N = count {{ C | r(Y, C) }}.\n?- g(X, N).");
        let after =
            format!("{facts}g(X, N) :- q(X), N = count {{ C | r(Y, C) }}, Y = X + 1.\n?- g(X, N).");
        let answers = |src: &str| {
            crate::api::run(src)
                .unwrap_or_else(|e| panic!("runs: {e:?}"))
                .answers
        };
        assert_eq!(answers(&before), vec![vec!["g(1, 1).", "g(2, 2)."]]);
        assert_eq!(answers(&before), answers(&after));
    }

    /// An `=`-chain likewise no longer depends on being written in dependency
    /// order — the wart the group-key rule was originally made consistent with.
    #[test]
    fn an_assignment_chain_may_be_written_in_any_order() {
        let ast = crate::parser::parse("a(1).\nrev(X, M) :- a(X), M = N + 1, N = X + 1.\n")
            .expect("parses");
        lower(&ast).expect("the scheduler finds the order");
    }

    /// A **cycle** is the case reordering cannot fix, and it gets its own
    /// message: the aggregate needs `Y`, but `Y` comes from the aggregate's own
    /// result. Telling the author to move the assignment earlier — which the
    /// pre-scheduler error did — would have been impossible advice.
    #[test]
    fn a_circular_dependency_is_rejected_as_circular() {
        let errors = lower_errors(
            "a(1).\nr(1, \"x\").\n\
             cyc(X, N) :- a(X), N = count { C | r(Y, C) }, Y = N.\n",
        );
        assert!(
            errors.iter().any(|e| {
                let msg = e.to_string();
                msg.contains("circular dependency") && msg.contains("`Y`")
            }),
            "unexpected errors: {errors:?}"
        );
        // And only that error — no cascading head-safety noise.
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    /// A variable nothing binds is still unsafe, and says so distinctly from a
    /// cycle: no order exists because the binder does not exist.
    #[test]
    fn a_never_bound_variable_is_rejected_as_unbound() {
        let errors = lower_errors("a(1).\nu(X) :- a(X), X > Z.\n");
        assert!(
            errors.iter().any(|e| {
                let msg = e.to_string();
                msg.contains("is never bound") && msg.contains("`Z`")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    /// Two aggregates in one body may each use the same *goal-local* name: they
    /// share a slot, but each sub-join binds and backtracks it independently, so
    /// neither is a group key of the other — and neither is a dependency of the
    /// other for scheduling.
    #[test]
    fn goal_local_names_may_repeat_across_aggregates() {
        let ast = crate::parser::parse(
            "q(\"k\").\nr(1).\ns(7).\n\
             both(K, N, M) :- q(K), N = count { C | r(C) }, M = count { C | s(C) }.\n",
        )
        .expect("parses");
        lower(&ast).expect("goal-local names are local to each aggregate");
    }

    // --- check_program: referenced-but-undefined predicates ---

    /// Parses + lowers `src`, then runs the undefined-predicate lint.
    fn warnings(src: &str) -> Vec<Warning> {
        let ast = crate::parser::parse(src).expect("parses");
        let program = lower(&ast).expect("lowers");
        check_program(&program)
    }

    #[test]
    fn levenshtein_matches_known_distances() {
        assert_eq!(levenshtein("ancestor", "ancester"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("same", "same"), 0);
    }

    #[test]
    fn undefined_body_predicate_warns_with_suggestion() {
        // `ancester` in the recursive rule is a typo for `ancestor`.
        let src = "\
parent(\"alice\", \"bob\").
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancester(Z, Y).
?- ancestor(\"alice\", Who).
";
        assert_eq!(
            warnings(src),
            vec![Warning::UndefinedPredicate {
                name: "ancester".to_string(),
                arity: 2,
                suggestion: Some("ancestor".to_string()),
            }]
        );
    }

    #[test]
    fn fully_defined_program_warns_nothing() {
        let src = "\
parent(\"alice\", \"bob\").
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
?- ancestor(\"alice\", Who).
";
        assert_eq!(warnings(src), Vec::new());
    }

    #[test]
    fn rule_head_counts_as_defined_even_if_never_derivable() {
        // `unreachable` heads a rule whose body predicate `never` is undefined:
        // `unreachable` itself is defined (a rule head), so only `never` warns.
        let src = "\
base(\"a\").
unreachable(X) :- never(X).
?- unreachable(Who).
";
        assert_eq!(
            warnings(src),
            vec![Warning::UndefinedPredicate {
                name: "never".to_string(),
                arity: 1,
                suggestion: None,
            }]
        );
    }

    #[test]
    fn query_only_undefined_predicate_warns() {
        let src = "person(\"alice\").\n?- ghost(Who).";
        assert_eq!(
            warnings(src),
            vec![Warning::UndefinedPredicate {
                name: "ghost".to_string(),
                arity: 1,
                suggestion: None,
            }]
        );
    }

    #[test]
    fn undefined_under_negation_still_warns() {
        // `not missing(X)` matches everything precisely because `missing` is
        // empty — usually not what was meant, so we still flag it.
        let src = "person(\"alice\").\n?- person(X), not missing(X).";
        assert_eq!(
            warnings(src),
            vec![Warning::UndefinedPredicate {
                name: "missing".to_string(),
                arity: 1,
                suggestion: None,
            }]
        );
    }

    #[test]
    fn suggestion_prefers_matching_arity_on_a_distance_tie() {
        // `usr` is edit-distance 1 from both `use/1` and `user/2` (insert one
        // char). The referenced arity is 2, so `user` wins the tie-break.
        let src = "\
use(\"x\").
user(\"a\", 1).
lookup(N) :- usr(N, _).
?- lookup(N).
";
        assert_eq!(
            warnings(src),
            vec![Warning::UndefinedPredicate {
                name: "usr".to_string(),
                arity: 2,
                suggestion: Some("user".to_string()),
            }]
        );
    }

    #[test]
    fn no_suggestion_when_nothing_is_close() {
        let src = "aardvark(\"x\").\n?- zzz(W).";
        assert_eq!(
            warnings(src),
            vec![Warning::UndefinedPredicate {
                name: "zzz".to_string(),
                arity: 1,
                suggestion: None,
            }]
        );
    }
}
