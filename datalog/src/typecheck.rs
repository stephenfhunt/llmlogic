//! Static type inference (`spec.md` §4).
//!
//! A separate pass over the resolved IR, run between [`crate::lower::lower`] and
//! [`crate::engine::eval`] — never inside `eval`, which stays type-agnostic so
//! the evaluator's laws hold over the full value space (spec §17, 2026-07-21).
//!
//! The language is statically typed with **full inference** — annotations are
//! never required. Inference is a simple unification pass over the five
//! primitive types ([`crate::ast::TypeName`]): symbol, string, int, float, bool.
//! No polymorphism, no type constructors, not Hindley–Milner. Type information
//! flows from three sources (§4):
//! 1. literals in facts,
//! 2. variable flow through rule bodies (a variable unifies the types of every
//!    position it occupies), and
//! 3. builtin operands (§8): arithmetic requires int or float, and every
//!    comparison requires both operands the same type — any type, since every
//!    primitive is ordered (§8).
//!
//! Any conflict — an int column joined against a string column, `age(X, "old")`
//! beside `age("bob", 30)`, or `1 + "a"` — is a structured **type error before
//! evaluation** (§12), never a silent empty result. Imported column types (§13)
//! and declared-signature verification (§4) are additional sources that land
//! once imports and IR-level declared types exist.

use crate::ast::{AggOp, TypeName};
use crate::error::Error;
use crate::ir;

/// The inferred type of every column, once a program type-checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEnv {
    /// `columns[pred][col]` — the inferred type, or `None` for a column no
    /// literal, join, or builtin ever constrained (by soundness such a column
    /// carries no facts, so its type is irrelevant).
    columns: Vec<Vec<Option<TypeName>>>,
}

impl TypeEnv {
    /// The inferred type of a predicate column, if inference constrained it.
    pub fn column_type(&self, pred: ir::PredId, col: usize) -> Option<TypeName> {
        self.columns
            .get(pred.0 as usize)
            .and_then(|cols| cols.get(col).copied().flatten())
    }
}

/// Infers every column's type, or returns all type conflicts. Runs after
/// lowering and before evaluation.
pub fn typecheck(program: &ir::Program) -> std::result::Result<TypeEnv, Vec<Error>> {
    let mut checker = TypeChecker::new(program);
    checker.gather();
    checker.finish()
}

/// The concrete type of a ground value, or `None` for [`absent`](ir::Value::Absent),
/// which is **type-neutral** (§4): it inhabits any column without joining that
/// column's type unification, so a numeric column with some missing cells still
/// infers `int`/`float`. Every caller skips constraint generation on `None`.
fn type_of(value: &ir::Value) -> Option<TypeName> {
    Some(match value {
        ir::Value::Absent => return None,
        ir::Value::Symbol(_) => TypeName::Symbol,
        ir::Value::String(_) => TypeName::String,
        ir::Value::Int(_) => TypeName::Int,
        ir::Value::Float(_) => TypeName::Float,
        ir::Value::Bool(_) => TypeName::Bool,
        ir::Value::Date(_) => TypeName::Date,
        ir::Value::Timestamp(_) => TypeName::Timestamp,
        ir::Value::Duration(_) => TypeName::Duration,
    })
}

fn is_numeric(ty: TypeName) -> bool {
    matches!(ty, TypeName::Int | TypeName::Float)
}

/// Whether an expression is a bare `absent` literal (§4) — the operand form that
/// makes a comparison unconditionally false and so exempt from type constraints.
/// Arithmetic that merely *produces* absent at runtime (`X + absent`) is not this
/// — it still types by its numeric operand.
fn is_absent_literal(expr: &ir::Expr) -> bool {
    matches!(expr, ir::Expr::Term(ir::Term::Const(ir::Value::Absent)))
}

/// A type's name in a diagnostic — the canonical source spelling, so a message
/// quotes the word the user would write.
fn type_label(ty: TypeName) -> &'static str {
    ty.keyword()
}

/// A union-find over "type slots" — one per predicate column, plus fresh slots
/// for each rule/query variable and each literal operand. Each class carries at
/// most one concrete [`TypeName`]; unifying two classes with different concrete
/// types is a type conflict.
struct TypeChecker<'a> {
    program: &'a ir::Program,
    parent: Vec<usize>,
    rank: Vec<u8>,
    ty: Vec<Option<TypeName>>,
    /// A human-readable description of each slot, for error messages.
    label: Vec<String>,
    /// Slots that must resolve to a numeric type (arithmetic / ordered
    /// comparison operands), checked after all unification.
    numeric: Vec<usize>,
    /// First slot index of each predicate's columns.
    col_base: Vec<usize>,
    errors: Vec<Error>,
}

impl<'a> TypeChecker<'a> {
    fn new(program: &'a ir::Program) -> Self {
        // One slot per column, laid out predicate by predicate.
        let mut col_base = Vec::with_capacity(program.predicates.len());
        let mut total = 0;
        for info in &program.predicates {
            col_base.push(total);
            total += info.arity as usize;
        }
        let mut checker = TypeChecker {
            program,
            parent: (0..total).collect(),
            rank: vec![0; total],
            ty: vec![None; total],
            label: Vec::with_capacity(total),
            numeric: Vec::new(),
            col_base,
            errors: Vec::new(),
        };
        for info in &program.predicates {
            for col in 0..info.arity as usize {
                checker.label.push(column_label(info, col));
            }
        }
        debug_assert_eq!(checker.label.len(), total);
        checker
    }

    /// Allocates a fresh, untyped slot with a description.
    fn fresh(&mut self, label: String) -> usize {
        let id = self.parent.len();
        self.parent.push(id);
        self.rank.push(0);
        self.ty.push(None);
        self.label.push(label);
        id
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    /// Constrains a slot's class to a concrete type, reporting a conflict if the
    /// class already resolved to a different one.
    fn set_type(&mut self, node: usize, t: TypeName) {
        let root = self.find(node);
        match self.ty[root] {
            None => self.ty[root] = Some(t),
            Some(existing) if existing == t => {}
            Some(existing) => self.errors.push(Error::semantic(format!(
                "type error: {} is used as both {} and {}",
                self.label[root],
                type_label(existing),
                type_label(t),
            ))),
        }
    }

    /// Unifies two slots' classes, reporting a conflict if they carry different
    /// concrete types.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let merged = match (self.ty[ra], self.ty[rb]) {
            (Some(x), Some(y)) if x != y => {
                self.errors.push(Error::semantic(format!(
                    "type error: {} has type {} but {} has type {}",
                    self.label[ra],
                    type_label(x),
                    self.label[rb],
                    type_label(y),
                )));
                Some(x)
            }
            (x, y) => x.or(y),
        };
        let (root, child) = if self.rank[ra] >= self.rank[rb] {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent[child] = root;
        if self.rank[ra] == self.rank[rb] {
            self.rank[root] += 1;
        }
        self.ty[root] = merged;
    }

    /// Gathers every constraint over the program.
    fn gather(&mut self) {
        // Facts pin their columns to concrete types.
        for fact in &self.program.facts {
            let base = self.col_base[fact.pred.0 as usize];
            for (col, value) in fact.tuple.0.iter().enumerate() {
                // `absent` is type-neutral: it pins no column type (§4).
                if let Some(ty) = type_of(value) {
                    self.set_type(base + col, ty);
                }
            }
        }
        for (rule_id, rule) in self.program.rules.iter().enumerate() {
            let ctx = format!("rule {rule_id}");
            let vars = self.var_slots(&rule.var_names, &ctx);
            self.atom_constraints(&rule.head, &vars);
            self.body_constraints(&rule.body, &vars);
        }
        for (query_id, query) in self.program.queries.iter().enumerate() {
            let ctx = format!("query {query_id}");
            let vars = self.var_slots(&query.var_names, &ctx);
            self.body_constraints(&query.body, &vars);
        }
    }

    /// Fresh slots for each variable of a clause, described for error messages.
    fn var_slots(&mut self, var_names: &[Option<String>], ctx: &str) -> Vec<usize> {
        var_names
            .iter()
            .map(|name| {
                let name = name.as_deref().unwrap_or("_");
                self.fresh(format!("variable `{name}` in {ctx}"))
            })
            .collect()
    }

    /// Unifies each argument of `atom` with its column: a constant pins the
    /// column's type, a variable ties the variable and column together.
    fn atom_constraints(&mut self, atom: &ir::Atom, vars: &[usize]) {
        let base = self.col_base[atom.pred.0 as usize];
        for (col, arg) in atom.args.iter().enumerate() {
            match arg {
                // `absent` pins no column type (§4); a use-site `p(absent)` in a
                // head/fact leaves the column to be typed by its other rows.
                ir::Term::Const(value) => {
                    if let Some(ty) = type_of(value) {
                        self.set_type(base + col, ty);
                    }
                }
                ir::Term::Var(var) => self.union(vars[var.0 as usize], base + col),
            }
        }
    }

    fn body_constraints(&mut self, body: &[ir::BodyLiteral], vars: &[usize]) {
        for literal in body {
            match &literal.kind {
                ir::BodyLiteralKind::Atom(atom) | ir::BodyLiteralKind::NegAtom(atom) => {
                    self.atom_constraints(atom, vars);
                }
                ir::BodyLiteralKind::Compare { lhs, rhs, .. } => {
                    let l = self.expr_slot(lhs, vars);
                    let r = self.expr_slot(rhs, vars);
                    // A comparison with a bare `absent` operand is
                    // unconditionally false (§8), so it constrains nothing — the
                    // operands need not share a type, and `<`/`<=`/`>`/`>=`
                    // against absent is *false*, not an error. Any arithmetic
                    // *inside* the other operand is still typed by `expr_slot`
                    // above.
                    if is_absent_literal(lhs) || is_absent_literal(rhs) {
                        continue;
                    }
                    // Every comparison unifies its operands' types (§8): `=`
                    // assignment gives the target the other side's type, and any
                    // filter requires both operands the same type. That is the
                    // *only* constraint — an ordered comparison uses the operand
                    // type's natural order and every primitive has one (§8), the
                    // same order `min`/`max` fold with, so `<` adds no numeric
                    // requirement.
                    self.union(l, r);
                }
                ir::BodyLiteralKind::Presence { expr, .. } => {
                    // A presence test constrains no type — its operand may be any
                    // type (§4). Still type any arithmetic inside the operand.
                    let _ = self.expr_slot(expr, vars);
                }
                ir::BodyLiteralKind::Aggregate {
                    result,
                    op,
                    params,
                    expr,
                    goal,
                } => {
                    // The goal is a sub-body over the same variable scope (§9);
                    // its atoms constrain the group-key and goal-local variables.
                    self.body_constraints(goal, vars);
                    // Type the collected expression (and any parameters) so that
                    // internal arithmetic errors surface even when the operator
                    // ignores the value's type.
                    let value = self.expr_slot(expr, vars);
                    for param in params {
                        let _ = self.expr_slot(param, vars);
                    }
                    let result_slot = vars[result.0 as usize];
                    match op {
                        // count : int, over any type (the collected value's type
                        // is unconstrained — a binding is a binding, §9).
                        AggOp::Count => self.set_type(result_slot, TypeName::Int),
                        // sum : the same numeric type as the collected value.
                        AggOp::Sum => {
                            self.union(result_slot, value);
                            self.numeric.push(value);
                        }
                        // avg : always float; the collected value must be numeric.
                        AggOp::Avg => {
                            self.set_type(result_slot, TypeName::Float);
                            self.numeric.push(value);
                        }
                        // min / max : the same type as the collected value, which
                        // may be any single ordered type (every primitive is
                        // ordered, §8) — so no numeric constraint.
                        AggOp::Min | AggOp::Max => self.union(result_slot, value),
                    }
                }
            }
        }
    }

    /// The slot representing an expression's type. Arithmetic unifies both
    /// operands and the result into one numeric class.
    fn expr_slot(&mut self, expr: &ir::Expr, vars: &[usize]) -> usize {
        match expr {
            ir::Expr::Term(ir::Term::Const(value)) => match type_of(value) {
                Some(ty) => {
                    let node = self.fresh(format!("literal {}", type_label(ty)));
                    self.set_type(node, ty);
                    node
                }
                // A bare `absent` literal is type-neutral (§4): a fresh
                // unconstrained node. Comparisons against it are skipped in
                // `body_constraints`, so this node never forces a type.
                None => self.fresh("literal absent".to_string()),
            },
            ir::Expr::Term(ir::Term::Var(var)) => vars[var.0 as usize],
            ir::Expr::Binary { lhs, rhs, .. } => {
                let l = self.expr_slot(lhs, vars);
                let r = self.expr_slot(rhs, vars);
                self.union(l, r);
                self.numeric.push(l);
                l
            }
            // `X as T` has type `T` **unconditionally** (§4/§8): a fresh node
            // fixed to `T`, deliberately *not* unioned with the operand, so
            // nothing about `T` flows back into `X`'s column and the cast both
            // satisfies and terminates inference for its subexpression. §9's
            // `Avg` arm above is the precedent — a result type independent of
            // the operand's.
            //
            // The operand is still typed, so arithmetic errors inside it
            // surface; which conversions are *defined* is a value-level question
            // §8 settles at evaluation, exactly as mixed-operand arithmetic is.
            ir::Expr::Cast { expr, ty } => {
                let _ = self.expr_slot(expr, vars);
                let node = self.fresh(format!("cast to {}", type_label(*ty)));
                self.set_type(node, *ty);
                node
            }
        }
    }

    /// Runs the deferred numeric checks and builds the [`TypeEnv`], or returns
    /// the deduplicated type errors.
    fn finish(mut self) -> std::result::Result<TypeEnv, Vec<Error>> {
        for node in std::mem::take(&mut self.numeric) {
            let root = self.find(node);
            if let Some(ty) = self.ty[root]
                && !is_numeric(ty)
            {
                self.errors.push(Error::semantic(format!(
                    "type error: {} has type {} but is used in arithmetic or a numeric \
                     aggregate (`sum`/`avg`), which requires int or float",
                    self.label[root],
                    type_label(ty),
                )));
            }
        }
        // Verify asserted declared types (§4) against the inferred ones. A
        // declared type that contradicts what inference derived is an error
        // naming the column; a declared column inference never constrained is
        // simply unrefuted (no error).
        for (p, info) in self.program.predicates.iter().enumerate() {
            let Some(declared) = &info.field_types else {
                continue;
            };
            for (col, declared_ty) in declared.iter().enumerate() {
                let Some(declared_ty) = *declared_ty else {
                    continue;
                };
                let root = self.find(self.col_base[p] + col);
                if let Some(inferred) = self.ty[root]
                    && inferred != declared_ty
                {
                    self.errors.push(Error::semantic(format!(
                        "type error: {} is declared as {} but its values are {}",
                        column_label(info, col),
                        type_label(declared_ty),
                        type_label(inferred),
                    )));
                }
            }
        }
        if !self.errors.is_empty() {
            return Err(dedup(self.errors));
        }
        let columns = self
            .program
            .predicates
            .iter()
            .enumerate()
            .map(|(p, info)| {
                (0..info.arity as usize)
                    .map(|col| {
                        let root = self.find(self.col_base[p] + col);
                        // Fall back to the declared type for a column inference
                        // left unconstrained, so a declared-only column still
                        // reports its asserted type.
                        self.ty[root]
                            .or_else(|| info.field_types.as_ref().and_then(|types| types[col]))
                    })
                    .collect()
            })
            .collect();
        Ok(TypeEnv { columns })
    }
}

/// A description of a predicate column for error messages: `pred.field` when the
/// field is named, else `pred column N`.
fn column_label(info: &ir::PredicateInfo, col: usize) -> String {
    match &info.fields {
        Some(fields) => format!("`{}.{}`", info.name, fields[col]),
        None => format!("`{}` column {col}", info.name),
    }
}

/// Removes duplicate error messages while preserving first-seen order.
fn dedup(errors: Vec<Error>) -> Vec<Error> {
    let mut seen = std::collections::HashSet::new();
    errors
        .into_iter()
        .filter(|e| seen.insert(e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CmpOp, Span};
    use crate::ir::{Atom, BodyLiteral, BodyLiteralKind, Expr, Fact, Program, Rule, Term, Tuple};
    use crate::ir::{RuleId, Value, Var};

    fn lit(kind: BodyLiteralKind) -> BodyLiteral {
        BodyLiteral {
            kind,
            span: Span::DUMMY,
        }
    }

    fn atom(pred: ir::PredId, args: Vec<Term>) -> Atom {
        Atom { pred, args }
    }

    #[test]
    fn well_typed_program_infers_column_types() {
        // age("alice", 30). age("bob", 15).
        // adult(X) :- age(X, A), A >= 18.
        let mut program = Program::default();
        let age = program.intern_pred("age", 2);
        let adult = program.intern_pred("adult", 1);
        for (name, years) in [("alice", 30), ("bob", 15)] {
            program.facts.push(Fact {
                pred: age,
                tuple: Tuple(vec![Value::String(name.to_string()), Value::Int(years)]),
            });
        }
        program.rules.push(Rule {
            head: atom(adult, vec![Term::Var(Var(0))]),
            body: vec![
                lit(BodyLiteralKind::Atom(atom(
                    age,
                    vec![Term::Var(Var(0)), Term::Var(Var(1))],
                ))),
                lit(BodyLiteralKind::Compare {
                    op: CmpOp::Ge,
                    lhs: Expr::Term(Term::Var(Var(1))),
                    rhs: Expr::Term(Term::Const(Value::Int(18))),
                }),
            ],
            var_names: vec![Some("X".to_string()), Some("A".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];

        let env = typecheck(&program).expect("well-typed program");
        assert_eq!(env.column_type(age, 0), Some(TypeName::String));
        assert_eq!(env.column_type(age, 1), Some(TypeName::Int));
        // The head variable flows the type through.
        assert_eq!(env.column_type(adult, 0), Some(TypeName::String));
    }

    #[test]
    fn heterogeneous_fact_column_is_a_type_error() {
        // age("bob", 30). age("carol", "old").  =>  column age.1 is int and string.
        let mut program = Program::default();
        let age = program.intern_pred("age", 2);
        program.facts.push(Fact {
            pred: age,
            tuple: Tuple(vec![Value::String("bob".to_string()), Value::Int(30)]),
        });
        program.facts.push(Fact {
            pred: age,
            tuple: Tuple(vec![
                Value::String("carol".to_string()),
                Value::String("old".to_string()),
            ]),
        });
        let errors = typecheck(&program).expect_err("type conflict");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("int") && e.to_string().contains("string")),
            "unexpected: {errors:?}"
        );
    }

    #[test]
    fn join_across_incompatible_columns_is_a_type_error() {
        // p(1). q("x"). r(X) :- p(X), q(X).  =>  X is int (p) and string (q).
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let q = program.intern_pred("q", 1);
        let r = program.intern_pred("r", 1);
        program.facts.push(Fact {
            pred: p,
            tuple: Tuple(vec![Value::Int(1)]),
        });
        program.facts.push(Fact {
            pred: q,
            tuple: Tuple(vec![Value::String("x".to_string())]),
        });
        program.rules.push(Rule {
            head: atom(r, vec![Term::Var(Var(0))]),
            body: vec![
                lit(BodyLiteralKind::Atom(atom(p, vec![Term::Var(Var(0))]))),
                lit(BodyLiteralKind::Atom(atom(q, vec![Term::Var(Var(0))]))),
            ],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let errors = typecheck(&program).expect_err("join type conflict");
        assert!(!errors.is_empty());
    }

    #[test]
    fn mixed_int_float_arithmetic_is_a_type_error() {
        // seed(1.5). t(N) :- seed(V), N = V + 1.  =>  V is float but `+ 1` forces int.
        let mut program = Program::default();
        let seed = program.intern_pred("seed", 1);
        let t = program.intern_pred("t", 2);
        program.facts.push(Fact {
            pred: seed,
            tuple: Tuple(vec![Value::Float(ir::F64::new(1.5).unwrap())]),
        });
        program.rules.push(Rule {
            head: atom(t, vec![Term::Var(Var(0)), Term::Var(Var(1))]),
            body: vec![
                lit(BodyLiteralKind::Atom(atom(seed, vec![Term::Var(Var(0))]))),
                lit(BodyLiteralKind::Compare {
                    op: CmpOp::Eq,
                    lhs: Expr::Term(Term::Var(Var(1))),
                    rhs: Expr::Binary {
                        op: crate::ast::ArithOp::Add,
                        lhs: Box::new(Expr::Term(Term::Var(Var(0)))),
                        rhs: Box::new(Expr::Term(Term::Const(Value::Int(1)))),
                    },
                }),
            ],
            var_names: vec![Some("V".to_string()), Some("N".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let errors = typecheck(&program).expect_err("mixed arithmetic");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("float") && e.to_string().contains("int")),
            "unexpected: {errors:?}"
        );
    }

    #[test]
    fn ordered_comparison_accepts_any_single_type() {
        // p("x"). t(X) :- p(X), X > "a".  =>  ordered comparison on strings,
        // which §8 orders lexicographically. Rejected until `bugs/006`.
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let t = program.intern_pred("t", 1);
        program.facts.push(Fact {
            pred: p,
            tuple: Tuple(vec![Value::String("x".to_string())]),
        });
        program.rules.push(Rule {
            head: atom(t, vec![Term::Var(Var(0))]),
            body: vec![
                lit(BodyLiteralKind::Atom(atom(p, vec![Term::Var(Var(0))]))),
                lit(BodyLiteralKind::Compare {
                    op: CmpOp::Gt,
                    lhs: Expr::Term(Term::Var(Var(0))),
                    rhs: Expr::Term(Term::Const(Value::String("a".to_string()))),
                }),
            ],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let env = typecheck(&program).expect("`X > \"a\"` over a string column type-checks");
        assert_eq!(env.column_type(p, 0), Some(TypeName::String));
    }

    #[test]
    fn ordered_comparison_still_rejects_a_cross_type_pair() {
        // p("x"). t(X) :- p(X), X > 1.  =>  string vs int, still a type error:
        // widening the ordered comparison left `union(l, r)` in place.
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let t = program.intern_pred("t", 1);
        program.facts.push(Fact {
            pred: p,
            tuple: Tuple(vec![Value::String("x".to_string())]),
        });
        program.rules.push(Rule {
            head: atom(t, vec![Term::Var(Var(0))]),
            body: vec![
                lit(BodyLiteralKind::Atom(atom(p, vec![Term::Var(Var(0))]))),
                lit(BodyLiteralKind::Compare {
                    op: CmpOp::Gt,
                    lhs: Expr::Term(Term::Var(Var(0))),
                    rhs: Expr::Term(Term::Const(Value::Int(1))),
                }),
            ],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let errors = typecheck(&program).expect_err("cross-type ordered comparison");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("string") && e.to_string().contains("int")),
            "unexpected: {errors:?}"
        );
    }

    /// Attaches a `declare`-style schema (names + declared types) to a predicate,
    /// as lowering does via `attach_field_names`.
    fn declare_schema(
        program: &mut Program,
        pred: ir::PredId,
        schema: &[(&str, Option<TypeName>)],
    ) {
        let info = &mut program.predicates[pred.0 as usize];
        info.fields = Some(schema.iter().map(|(n, _)| n.to_string()).collect());
        info.field_types = Some(schema.iter().map(|(_, t)| *t).collect());
    }

    #[test]
    fn declared_signature_matching_inferred_is_accepted() {
        // declare person(name: string, age: int).  person("alice", 30).
        let mut program = Program::default();
        let person = program.intern_pred("person", 2);
        program.facts.push(Fact {
            pred: person,
            tuple: Tuple(vec![Value::String("alice".to_string()), Value::Int(30)]),
        });
        declare_schema(
            &mut program,
            person,
            &[
                ("name", Some(TypeName::String)),
                ("age", Some(TypeName::Int)),
            ],
        );
        let env = typecheck(&program).expect("declared types match inference");
        assert_eq!(env.column_type(person, 0), Some(TypeName::String));
        assert_eq!(env.column_type(person, 1), Some(TypeName::Int));
    }

    #[test]
    fn declared_signature_conflicting_with_fact_is_rejected() {
        // declare person(name: string, age: string).  person("alice", 30).
        // `age` is declared string but the fact makes it int.
        let mut program = Program::default();
        let person = program.intern_pred("person", 2);
        program.facts.push(Fact {
            pred: person,
            tuple: Tuple(vec![Value::String("alice".to_string()), Value::Int(30)]),
        });
        declare_schema(
            &mut program,
            person,
            &[
                ("name", Some(TypeName::String)),
                ("age", Some(TypeName::String)),
            ],
        );
        let errors = typecheck(&program).expect_err("declared/inferred mismatch");
        assert!(
            errors.iter().any(|e| {
                let s = e.to_string();
                s.contains("person.age") && s.contains("declared as string") && s.contains("int")
            }),
            "unexpected: {errors:?}"
        );
    }

    #[test]
    fn declared_field_without_type_is_not_verified() {
        // declare person(name, age).  person("alice", 30).  Names only — no
        // asserted types, so nothing to verify.
        let mut program = Program::default();
        let person = program.intern_pred("person", 2);
        program.facts.push(Fact {
            pred: person,
            tuple: Tuple(vec![Value::String("alice".to_string()), Value::Int(30)]),
        });
        declare_schema(&mut program, person, &[("name", None), ("age", None)]);
        let env = typecheck(&program).expect("untyped declaration adds no constraint");
        assert_eq!(env.column_type(person, 1), Some(TypeName::Int));
    }

    #[test]
    fn declared_only_column_reports_its_declared_type() {
        // declare thing(kind: symbol).  No fact or rule constrains `kind`, so the
        // declaration is unrefuted and seeds the inferred type.
        let mut program = Program::default();
        let thing = program.intern_pred("thing", 1);
        declare_schema(&mut program, thing, &[("kind", Some(TypeName::Symbol))]);
        let env = typecheck(&program).expect("declared-only column is unrefuted");
        assert_eq!(env.column_type(thing, 0), Some(TypeName::Symbol));
    }

    /// Typechecks source text, returning the environment alongside the lowered
    /// program so a test can name a column by predicate name. The cast tests
    /// below read as the programs a user writes rather than as hand-built IR.
    fn typecheck_src(src: &str) -> std::result::Result<(TypeEnv, Program), Vec<Error>> {
        let ast = crate::parse(src).expect("parses");
        let program = crate::lower::lower(&ast).expect("lowers");
        typecheck(&program).map(|env| (env, program))
    }

    fn pred_named(program: &Program, name: &str) -> ir::PredId {
        let index = program
            .predicates
            .iter()
            .position(|p| p.name == name)
            .expect("predicate is in the program");
        ir::PredId(index as u32)
    }

    /// **The acceptance half** of the `as` cast's typing claim (testing rule 4,
    /// and the biconditional corollary — "the checker rejects X" is half a
    /// claim).
    ///
    /// §4: `X as T` has type `T` *unconditionally*, and inference never flows
    /// `T` back into the operand. So the same cast expression must typecheck
    /// over an operand of **every** primitive type, and the column it feeds must
    /// come out as `T` in every one of those programs.
    ///
    /// **Mutation-verified**: replacing the `expr_slot` cast arm's fresh node
    /// with `self.union(operand, node)` reddens this — the operand's type and
    /// `T` collide for four of the five source types.
    #[test]
    fn a_cast_types_as_its_target_over_every_operand_type() {
        // One fact per primitive type, so `src(V)` takes each in turn.
        for fact in [
            r#"src("text")."#,
            "src(sym).",
            "src(30).",
            "src(2.5).",
            "src(true).",
        ] {
            for (ty, expected) in [
                ("int", TypeName::Int),
                ("float", TypeName::Float),
                ("string", TypeName::String),
                ("symbol", TypeName::Symbol),
                ("bool", TypeName::Bool),
            ] {
                let src = format!("{fact}\nout(C) :- src(V), C = V as {ty}.");
                let (env, program) = typecheck_src(&src)
                    .unwrap_or_else(|e| panic!("a cast constrained its operand in {src:?}: {e:?}"));
                assert_eq!(
                    env.column_type(pred_named(&program, "out"), 0),
                    Some(expected),
                    "wrong result type for {src:?}"
                );
            }
        }
    }

    /// **The rejecting half.** The result type is a real `T`, not a free
    /// variable that unifies with anything: joining a cast's result against a
    /// column of a different type is the same conflict any other type clash is.
    /// Without this, a cast arm that returned an *unconstrained* node would
    /// satisfy the acceptance test above and constrain nothing.
    ///
    /// **Mutation-verified**: dropping the `set_type` call from the cast arm
    /// reddens it. Note it reddens the *acceptance* test above too — an
    /// unconstrained result has no type to report — so that mutation does not
    /// discriminate between the two halves. The one that does is the converse,
    /// M6 above: unioning the operand into the result leaves this test green and
    /// reddens only the acceptance half. Both directions are therefore load
    /// bearing, which is the point of stating the claim as a biconditional.
    #[test]
    fn a_casts_result_type_conflicts_like_any_other() {
        // `C` is int by the cast, and string by `label`'s column.
        let errors =
            typecheck_src("src(30).\nlabel(\"a\").\nbad(C) :- src(V), C = V as int, label(C).")
                .expect_err("int result joined against a string column");
        assert!(
            errors.iter().any(|e| e.to_string().contains("type")),
            "unexpected errors: {errors:?}"
        );
    }
}
