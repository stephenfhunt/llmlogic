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
//!    negated atom, or occurring only in comparisons must occur in a positive
//!    body atom; facts must be ground.
//! 5. **Stratification** — trivial for now: any negated literal is reported as
//!    "negation not yet supported" (roadmap step 3 replaces this body with the
//!    real dependency-graph algorithm — same signature), and all rules form a
//!    single stratum in source order.
//!
//! Body literal order is preserved exactly — never reordered — because
//! provenance references body positions ([`ir::BodyIdx`]).
//!
//! Type inference (§4/§8) is deliberately *not* part of lowering; it runs as a
//! later pass over the IR (which retains spans for exactly that reason).

use std::collections::HashMap;

use crate::ast;
use crate::error::Error;
use crate::ir;

/// Lowers a surface program to the core IR, or reports every error found.
pub fn lower(program: &ast::Program) -> Result<ir::Program, Vec<Error>> {
    let mut lowerer = Lowerer::default();
    lowerer.collect_predicates(program);
    let mut out = ir::Program {
        predicates: lowerer.predicates.clone(),
        ..ir::Program::default()
    };

    for statement in &program.statements {
        match &statement.kind {
            ast::StatementKind::Import(import) => {
                let pred = lowerer.pred_id(&import.relation.name);
                out.imports.push(ir::ImportSpec {
                    pred,
                    path: import.path.clone(),
                    span: statement.span,
                });
            }
            // Declarations contribute schema/arity only (collected above);
            // nothing survives into the IR itself.
            ast::StatementKind::Declare(_) => {}
            ast::StatementKind::Clause(clause) => lowerer.lower_clause(clause, &mut out),
            ast::StatementKind::Query(query) => lowerer.lower_query(query, &mut out),
        }
    }

    // Trivial stratification: negation is rejected above, so every rule lands
    // in one stratum in source order.
    if !out.rules.is_empty() {
        out.strata = vec![(0..out.rules.len() as u32).map(ir::RuleId).collect()];
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

/// A predicate's field names, positionally ordered.
struct FieldSchema {
    /// Position → field name.
    fields: Vec<String>,
    /// Field name → position.
    by_field: HashMap<String, usize>,
    origin: SchemaOrigin,
}

/// Where a schema came from, so conflicts can name both sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaOrigin {
    Declare,
    Import,
}

impl SchemaOrigin {
    fn label(self) -> &'static str {
        match self {
            SchemaOrigin::Declare => "`declare`",
            SchemaOrigin::Import => "the import schema",
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
    /// arity consistency across declares, imports (explicit schemas), and use
    /// sites.
    fn collect_predicates(&mut self, program: &ast::Program) {
        for statement in &program.statements {
            match &statement.kind {
                ast::StatementKind::Import(import) => {
                    // Without an explicit schema the arity comes from the
                    // source at load time; use sites will establish it below.
                    // Field names likewise — so named access to a schema-less
                    // import is an error until fact sources (§13) land.
                    if let Some(schema) = &import.schema {
                        self.intern_checked(&import.relation.name, schema.len() as u32);
                        self.collect_schema(&import.relation.name, schema, SchemaOrigin::Import);
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

    /// Records the field names of `name`, reporting duplicates within the
    /// schema and conflicts with a schema already recorded for the predicate.
    fn collect_schema(&mut self, name: &str, fields: &[ast::FieldDecl], origin: SchemaOrigin) {
        let mut positions = Vec::with_capacity(fields.len());
        let mut by_field = HashMap::with_capacity(fields.len());
        for (position, field) in fields.iter().enumerate() {
            positions.push(field.name.name.clone());
            if by_field.insert(field.name.name.clone(), position).is_some() {
                self.errors.push(Error::Semantic(format!(
                    "duplicate field `{}` in the schema for `{name}`",
                    field.name.name
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
                self.errors.push(Error::Semantic(conflict));
            }
            return;
        }

        self.schemas.insert(
            name.to_string(),
            FieldSchema {
                fields: positions,
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
                self.errors.push(Error::Semantic(format!(
                    "predicate `{name}` used with arity {arity}, but previously with arity {known}"
                )));
            }
            return id;
        }
        let id = ir::PredId(self.predicates.len() as u32);
        self.predicates.push(ir::PredicateInfo {
            name: name.to_string(),
            arity,
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
        let Some(head) = self.lower_atom(&clause.head, &mut scope, AtomPos::Head) else {
            return;
        };

        if clause.body.is_empty() {
            // A fact must be ground (§10). Checked against the *lowered* head
            // so the named and positional paths behave identically; only the
            // wording of the error distinguishes them.
            let mut values = Vec::with_capacity(head.args.len());
            let mut ground = true;
            for (position, arg) in head.args.iter().enumerate() {
                match arg {
                    ir::Term::Const(value) => values.push(value.clone()),
                    ir::Term::Var(var) => {
                        ground = false;
                        let name = scope.names[var.0 as usize].as_deref().unwrap_or("_");
                        let place = self.describe_arg(&clause.head, position);
                        self.errors.push(Error::Semantic(format!(
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
            return;
        }

        let body = self.lower_body(&clause.body, &mut scope);
        let rule = ir::Rule {
            head,
            body,
            var_names: scope.names,
            span: clause.span,
        };
        self.check_rule_safety(&rule, &clause.head.predicate.name);
        out.rules.push(rule);
    }

    fn lower_query(&mut self, query: &ast::Query, out: &mut ir::Program) {
        let mut scope = VarScope::default();
        let body = self.lower_body(&query.body, &mut scope);
        let lowered = ir::Query {
            body,
            var_names: scope.names,
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
                    let Some(lowered_atom) = self.lower_atom(atom, scope, AtomPos::Body) else {
                        continue;
                    };
                    let kind = if *negated {
                        self.errors.push(Error::Semantic(format!(
                            "negation not yet supported: `not {}`",
                            atom.predicate.name
                        )));
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
                    lowered.push(ir::BodyLiteral {
                        kind: ir::BodyLiteralKind::Compare {
                            op: comparison.op,
                            lhs: self.lower_expr(&comparison.lhs, scope),
                            rhs: self.lower_expr(&comparison.rhs, scope),
                        },
                        span: literal.span,
                    });
                }
            }
        }
        lowered
    }

    /// Lowers an atom to positional form, or reports an error and returns
    /// `None`.
    fn lower_atom(
        &mut self,
        atom: &ast::Atom,
        scope: &mut VarScope,
        pos: AtomPos,
    ) -> Option<ir::Atom> {
        match &atom.args {
            ast::Args::Positional(terms) => {
                let pred = self.pred_id(&atom.predicate.name);
                let args = terms
                    .iter()
                    .map(|term| self.lower_term(term, scope))
                    .collect();
                Some(ir::Atom { pred, args })
            }
            ast::Args::Named(named) => self.lower_named_atom(atom, named, scope, pos),
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
    ) -> Option<ir::Atom> {
        let predicate = &atom.predicate.name;
        let Some(schema) = self.schemas.get(predicate) else {
            self.errors.push(Error::Semantic(format!(
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
                reported.push(Error::Semantic(format!(
                    "unknown field `{}` for predicate `{predicate}`; known fields: {}",
                    arg.field.name,
                    schema.fields.join(", ")
                )));
                continue;
            };
            if assigned[position].is_some() {
                reported.push(Error::Semantic(format!(
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
                reported.push(Error::Semantic(format!(
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
                Some(index) => self.lower_term(&named[index].value, scope),
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

    fn lower_expr(&mut self, expr: &ast::Expr, scope: &mut VarScope) -> ir::Expr {
        match &expr.kind {
            ast::ExprKind::Term(term) => ir::Expr::Term(self.lower_term(term, scope)),
            ast::ExprKind::Binary { op, lhs, rhs } => ir::Expr::Binary {
                op: *op,
                lhs: Box::new(self.lower_expr(lhs, scope)),
                rhs: Box::new(self.lower_expr(rhs, scope)),
            },
        }
    }

    /// Pass 4 for rules: head variables must be bound by a positive body atom.
    fn check_rule_safety(&mut self, rule: &ir::Rule, head_name: &str) {
        let bound = positive_vars(&rule.body);
        for arg in &rule.head.args {
            if let ir::Term::Var(var) = arg
                && !bound.contains(&var.0)
            {
                let name = rule.var_names[var.0 as usize].as_deref().unwrap_or("_");
                self.errors.push(Error::Semantic(format!(
                    "unsafe rule for `{head_name}`: head variable `{name}` does not occur \
                     in a positive body atom"
                )));
            }
        }
        self.check_body_safety(&rule.body, &rule.var_names, head_name);
    }

    /// Pass 4 shared by rules and queries: variables in negated atoms or
    /// occurring only in comparisons must be bound by a positive body atom.
    fn check_body_safety(
        &mut self,
        body: &[ir::BodyLiteral],
        var_names: &[Option<String>],
        context: &str,
    ) {
        let bound = positive_vars(body);
        let mut check = |var: &ir::Var, place: &str| {
            if !bound.contains(&var.0) {
                let name = var_names[var.0 as usize].as_deref().unwrap_or("_");
                self.errors.push(Error::Semantic(format!(
                    "unsafe {place} in `{context}`: variable `{name}` does not occur in a \
                     positive body atom"
                )));
            }
        };
        for literal in body {
            match &literal.kind {
                ir::BodyLiteralKind::Atom(_) => {}
                ir::BodyLiteralKind::NegAtom(atom) => {
                    for arg in &atom.args {
                        if let ir::Term::Var(var) = arg {
                            check(var, "negated atom");
                        }
                    }
                }
                ir::BodyLiteralKind::Compare { lhs, rhs, .. } => {
                    for expr in [lhs, rhs] {
                        for var in expr_vars(expr) {
                            check(&var, "comparison");
                        }
                    }
                }
            }
        }
    }
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

    #[test]
    fn negation_lowers_structurally_but_is_rejected() {
        // root(X) :- person(X), not parent(_, X).   (§16.2 shape)
        let program = ast::Program {
            statements: vec![ast_fix::rule(
                ast_fix::positional_atom("root", vec![ast_fix::var_term("X")]),
                vec![
                    ast_fix::positive_literal(ast_fix::positional_atom(
                        "person",
                        vec![ast_fix::var_term("X")],
                    )),
                    ast::Literal {
                        kind: ast::LiteralKind::Atom {
                            negated: true,
                            atom: ast_fix::positional_atom(
                                "parent",
                                vec![
                                    ast::Term {
                                        kind: ast::TermKind::Wildcard,
                                        span: Span::DUMMY,
                                    },
                                    ast_fix::var_term("X"),
                                ],
                            ),
                        },
                        span: Span::DUMMY,
                    },
                ],
            )],
        };
        let errors = lower(&program).expect_err("negation unsupported");
        // The NegAtom path lowers structurally (the wildcard becomes a fresh
        // var) and stratification then rejects it; the fresh var under
        // negation also trips the safety check, which is acceptable noise
        // until §7 defines wildcard-under-negation semantics.
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("negation not yet supported")),
            "unexpected errors: {errors:?}"
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
                    ir::BodyLiteralKind::Compare { .. } => {}
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

                // A12: fact/rule split and the trivial stratification.
                prop_assert_eq!(lowered.facts.len(), ast_fact_count(&program));
                let source_rules = ast_rules(&program);
                prop_assert_eq!(lowered.rules.len(), source_rules.len());
                if lowered.rules.is_empty() {
                    prop_assert!(lowered.strata.is_empty());
                } else {
                    let expected: Vec<ir::RuleId> =
                        (0..lowered.rules.len() as u32).map(ir::RuleId).collect();
                    prop_assert_eq!(&lowered.strata, &vec![expected]);
                }

                for (rule, source) in lowered.rules.iter().zip(&source_rules) {
                    // A9: body preserved 1:1 in order (generator emits only
                    // positive atoms).
                    prop_assert_eq!(rule.body.len(), source.body.len());
                    for literal in &rule.body {
                        prop_assert!(matches!(literal.kind, ir::BodyLiteralKind::Atom(_)));
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
                        if let ir::BodyLiteralKind::Atom(atom) = &literal.kind {
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
}
