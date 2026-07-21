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
//! 3. builtin operands (§8): arithmetic and ordered comparisons require int or
//!    float, and every comparison requires both operands the same type.
//!
//! Any conflict — an int column joined against a string column, `age(X, "old")`
//! beside `age("bob", 30)`, or `1 + "a"` — is a structured **type error before
//! evaluation** (§12), never a silent empty result. Imported column types (§13)
//! and declared-signature verification (§4) are additional sources that land
//! once imports and IR-level declared types exist.

use crate::ast::{CmpOp, TypeName};
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

/// The concrete type of a ground value.
fn type_of(value: &ir::Value) -> TypeName {
    match value {
        ir::Value::Symbol(_) => TypeName::Symbol,
        ir::Value::String(_) => TypeName::String,
        ir::Value::Int(_) => TypeName::Int,
        ir::Value::Float(_) => TypeName::Float,
        ir::Value::Bool(_) => TypeName::Bool,
    }
}

fn is_numeric(ty: TypeName) -> bool {
    matches!(ty, TypeName::Int | TypeName::Float)
}

fn type_label(ty: TypeName) -> &'static str {
    match ty {
        TypeName::Symbol => "symbol",
        TypeName::String => "string",
        TypeName::Int => "int",
        TypeName::Float => "float",
        TypeName::Bool => "bool",
    }
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
            Some(existing) => self.errors.push(Error::Semantic(format!(
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
                self.errors.push(Error::Semantic(format!(
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
                self.set_type(base + col, type_of(value));
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
                ir::Term::Const(value) => self.set_type(base + col, type_of(value)),
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
                ir::BodyLiteralKind::Compare { op, lhs, rhs } => {
                    let l = self.expr_slot(lhs, vars);
                    let r = self.expr_slot(rhs, vars);
                    // Every comparison unifies its operands' types (§8): `=`
                    // assignment gives the target the other side's type, and any
                    // filter requires both operands the same type.
                    self.union(l, r);
                    if matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge) {
                        self.numeric.push(l);
                    }
                }
            }
        }
    }

    /// The slot representing an expression's type. Arithmetic unifies both
    /// operands and the result into one numeric class.
    fn expr_slot(&mut self, expr: &ir::Expr, vars: &[usize]) -> usize {
        match expr {
            ir::Expr::Term(ir::Term::Const(value)) => {
                let node = self.fresh(format!("literal {}", type_label(type_of(value))));
                self.set_type(node, type_of(value));
                node
            }
            ir::Expr::Term(ir::Term::Var(var)) => vars[var.0 as usize],
            ir::Expr::Binary { lhs, rhs, .. } => {
                let l = self.expr_slot(lhs, vars);
                let r = self.expr_slot(rhs, vars);
                self.union(l, r);
                self.numeric.push(l);
                l
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
                self.errors.push(Error::Semantic(format!(
                    "type error: {} has type {} but is used in arithmetic or an ordered \
                     comparison, which requires int or float",
                    self.label[root],
                    type_label(ty),
                )));
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
                        self.ty[root]
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
    use crate::ast::Span;
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
    fn ordered_comparison_on_non_numeric_is_a_type_error() {
        // p("x"). t(X) :- p(X), X > "a".  =>  ordered comparison on strings.
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
        let errors = typecheck(&program).expect_err("non-numeric ordered comparison");
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("int or float")),
            "unexpected: {errors:?}"
        );
    }
}
