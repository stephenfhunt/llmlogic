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

use crate::ast::{AggOp, ArithOp, Span, TypeName};
use crate::error::{Error, ErrorCode};
use crate::ir;
use crate::print::print_value;

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

/// A type as a clash message names it: the type word, and the term that fixed
/// it where there was one — `symbol `alice`` rather than bare `symbol`.
///
/// `bugs/009`: the type words alone are what a subject looped fifteen rewrites
/// against, because "used as both symbol and string" never says that `name` was
/// read as a symbol. Naming the term makes a lowercase-identifier-where-a-
/// variable-was-meant self-evident without the engine guessing intent.
fn describe(ty: TypeName, witness: Option<&str>) -> String {
    match witness {
        Some(term) => format!("{} `{}`", type_label(ty), term),
        None => type_label(ty).to_string(),
    }
}

fn is_numeric(ty: TypeName) -> bool {
    matches!(ty, TypeName::Int | TypeName::Float)
}

/// The source spelling of an arithmetic operator, for messages.
fn arith_symbol(op: ArithOp) -> &'static str {
    match op {
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        ArithOp::Div => "/",
    }
}

/// One arithmetic node, held until unification has settled enough of its
/// operands to type it.
struct BinaryConstraint {
    op: ArithOp,
    lhs: usize,
    rhs: usize,
    result: usize,
    /// Where the arithmetic is written. Carried because §8's temporal rule
    /// defers this constraint to [`TypeChecker::finish`], long after the walk
    /// that knew the place has moved on.
    at: Option<Span>,
}

/// The source spelling of an aggregate operator, for messages.
fn agg_symbol(op: AggOp) -> &'static str {
    match op {
        AggOp::Count => "count",
        AggOp::Sum => "sum",
        AggOp::Min => "min",
        AggOp::Max => "max",
        AggOp::Avg => "avg",
    }
}

/// The message and fix for an arithmetic pair §8's algebra has no rule for.
///
/// Each case names *why* rather than restating the types, because the whole
/// point of the points-and-vectors rule is that it can be re-derived: a reader
/// told "two points do not add" can work out what does.
fn temporal_arith_error(op: ArithOp, lhs: TypeName, rhs: TypeName) -> (String, String) {
    let operation = format!(
        "`{} {} {}`",
        type_label(lhs),
        arith_symbol(op),
        type_label(rhs)
    );
    if lhs.is_temporal_point() && rhs.is_temporal_point() {
        if lhs != rhs {
            return (
                format!(
                    "{operation} mixes a date and a timestamp, which are \
                     different types"
                ),
                "widen the date with `as timestamp`".to_string(),
            );
        }
        return (
            format!("{operation} has no meaning — two points in time do not add"),
            "subtract them for the duration between, or add a duration to move one".to_string(),
        );
    }
    if (lhs == TypeName::Duration && is_numeric(rhs))
        || (is_numeric(lhs) && rhs == TypeName::Duration)
    {
        return (
            format!(
                "{operation} — a duration and a number do not add or subtract, \
                 because the number has no unit"
            ),
            "scale with `*`, or divide by a duration to get a number: `(C - O) / @1d`".to_string(),
        );
    }
    (
        format!("{operation} is not one of §8's temporal operations"),
        "point - point is a duration; point ± duration is a point; duration / duration \
         is a number"
            .to_string(),
    )
}

/// One `sum`/`avg` whose result type follows the type it folds over.
struct ReducerConstraint {
    op: AggOp,
    result: usize,
    value: usize,
    /// Where the aggregate is written; deferred for the same reason as
    /// [`BinaryConstraint::at`].
    at: Option<Span>,
}

/// What §8's algebra says an operator does to a pair of **known** operand
/// types, when at least one of them is temporal.
///
/// The rule is points and vectors: a `date`/`timestamp` is a position, a
/// `duration` a displacement. Point − point is a displacement, point ±
/// displacement is a point, displacements add to each other and scale by
/// numbers, and dividing one by another **cancels the unit** — which is the
/// only route from a duration to a number, and so the reason there is no
/// `duration as int` (§8).
fn temporal_result(op: ArithOp, lhs: TypeName, rhs: TypeName) -> Option<TypeName> {
    use ArithOp::{Add, Div, Mul, Sub};
    use TypeName::Duration;
    match (op, lhs, rhs) {
        // Point − point: the displacement between two positions. Same point
        // type on both sides — a date and a timestamp do not mix (§8).
        (Sub, l, r) if l.is_temporal_point() && l == r => Some(Duration),
        // Point ± displacement, in either order for `+`.
        (Add | Sub, l, Duration) if l.is_temporal_point() => Some(l),
        (Add, Duration, r) if r.is_temporal_point() => Some(r),
        // Displacements add to each other.
        (Add | Sub, Duration, Duration) => Some(Duration),
        // Scaling by a number preserves the unit; a scalar is a scalar, so
        // `int` and `float` both scale (§8).
        (Mul | Div, Duration, n) if is_numeric(n) => Some(Duration),
        (Mul, n, Duration) if is_numeric(n) => Some(Duration),
        // The units cancel: this is where a program names its unit.
        (Div, Duration, Duration) => Some(TypeName::Float),
        _ => None,
    }
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
/// The concrete type a class settled on, and the term that fixed it.
///
/// `bugs/009`: storing the type alone is why a column-level clash could name
/// one side. The type is *what* the class resolved to; `witness` is **how the
/// term that pinned it was written** — `alice` for a symbol against `"alice"`
/// for a string — which is the difference a reader needs and the difference the
/// type words alone cannot show.
#[derive(Debug, Clone)]
struct Fixed {
    ty: TypeName,
    /// `None` where the type came from inference or a declaration rather than a
    /// literal — a `count` result is an int with no term to point at.
    witness: Option<String>,
}

struct TypeChecker<'a> {
    program: &'a ir::Program,
    parent: Vec<usize>,
    rank: Vec<u8>,
    ty: Vec<Option<Fixed>>,
    /// A human-readable description of each slot, for error messages.
    label: Vec<String>,
    /// Slots that must resolve to a numeric type (arithmetic / ordered
    /// comparison operands), checked after all unification — each with where it
    /// was written, since the check outlives the walk.
    numeric: Vec<(usize, Option<Span>)>,
    /// Arithmetic whose typing cannot be decided where it is met, because §8's
    /// temporal rule makes an operator's result depend on its **operand types**
    /// — `date - date` is a duration, `date - duration` a date. Resolved to a
    /// fixpoint in [`Self::finish`], once unification has settled what is known.
    binaries: Vec<BinaryConstraint>,
    /// `sum`/`avg` over a value whose type is not yet known: both fold with
    /// `+`, so both admit `duration` as well as the numeric types, and `avg`'s
    /// result type differs between them (§9).
    reducers: Vec<ReducerConstraint>,
    /// First slot index of each predicate's columns.
    col_base: Vec<usize>,
    /// Where the constraint currently being gathered comes from: the clause
    /// being walked, narrowed to the body literal while inside one. Every
    /// diagnostic raised during gathering is stamped with it (§12), which is
    /// what turns "variable `Q` in rule 0" into a place a reader can go to.
    /// `None` while nothing is being walked, and for facts, whose own span the
    /// IR does not retain.
    at: Option<Span>,
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
            binaries: Vec::new(),
            reducers: Vec::new(),
            col_base,
            at: None,
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

    /// Raises a semantic error against whatever is being walked
    /// ([`Self::at`]).
    fn raise(&mut self, code: ErrorCode, message: String) -> &mut Error {
        let mut error = Error::new(code, message);
        if let Some(span) = self.at {
            error = error.at_span(span);
        }
        self.errors.push(error);
        self.errors.last_mut().expect("just pushed")
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
        self.set_type_from(node, t, None);
    }

    /// [`Self::set_type`], carrying the literal that pinned the type so a clash
    /// can name both sides as they were written (`bugs/009`).
    fn set_type_from(&mut self, node: usize, t: TypeName, witness: Option<String>) {
        let root = self.find(node);
        match &self.ty[root] {
            None => self.ty[root] = Some(Fixed { ty: t, witness }),
            Some(fixed) if fixed.ty == t => {}
            Some(fixed) => {
                let message = format!(
                    "{} is used as both {} and {}",
                    self.label[root],
                    describe(fixed.ty, fixed.witness.as_deref()),
                    describe(t, witness.as_deref()),
                );
                self.raise(ErrorCode::TypeClash, message);
            }
        }
    }

    /// Unifies two slots' classes, reporting a conflict if they carry different
    /// concrete types.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let clash = match (&self.ty[ra], &self.ty[rb]) {
            (Some(x), Some(y)) if x.ty != y.ty => Some((x.clone(), y.clone())),
            _ => None,
        };
        let merged = match clash {
            Some((x, y)) => {
                // Deliberately *not* `describe`: here the two sides are two
                // slots, not two terms. A variable's class took its type from
                // whatever pinned it, so appending that literal would read as
                // the variable's own value — "variable `Q` has type int `4`"
                // says something the message does not mean. Naming the term is
                // apt in `set_type_from`, where the incoming side **is** the
                // term being typed; provenance for an inherited type is the
                // secondary-span question (`bugs/009`), not this one.
                // The two slots being unified, not their classes' roots: a root
                // is whichever slot the class happened to grow from — a
                // wildcard in another rule, as often as not — and names neither
                // participant (`bugs/resolved/011`).
                let message = format!(
                    "{} has type {} but {} has type {}",
                    self.label[a],
                    type_label(x.ty),
                    self.label[b],
                    type_label(y.ty),
                );
                self.raise(ErrorCode::TypeClash, message);
                Some(x)
            }
            None => self.ty[ra].clone().or_else(|| self.ty[rb].clone()),
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
                    self.set_type_from(base + col, ty, Some(print_value(value)));
                }
            }
        }
        for (rule_id, rule) in self.program.rules.iter().enumerate() {
            let ctx = format!("rule {rule_id}");
            self.at = Some(rule.span);
            let vars = self.var_slots(&rule.var_names, &ctx);
            self.atom_constraints(&rule.head, &vars);
            self.body_constraints(&rule.body, &vars);
        }
        for (query_id, query) in self.program.queries.iter().enumerate() {
            let ctx = format!("query {query_id}");
            self.at = Some(query.span);
            let vars = self.var_slots(&query.var_names, &ctx);
            self.body_constraints(&query.body, &vars);
        }
        self.at = None;
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
                        self.set_type_from(base + col, ty, Some(print_value(value)));
                    }
                }
                ir::Term::Var(var) => self.union(vars[var.0 as usize], base + col),
            }
        }
    }

    fn body_constraints(&mut self, body: &[ir::BodyLiteral], vars: &[usize]) {
        let clause = self.at;
        for literal in body {
            // The literal is the narrower place, and the one a reader wants:
            // a clash is between two operands of one premise, not a property of
            // the whole clause.
            self.at = Some(literal.span);
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
                        // sum : the same type as the collected value, which
                        // must be one the fold makes sense over (§9).
                        AggOp::Sum => {
                            self.union(result_slot, value);
                            self.reducers.push(ReducerConstraint {
                                at: self.at,
                                op: AggOp::Sum,
                                result: result_slot,
                                value,
                            });
                        }
                        // avg : `float` over numbers, `duration` over durations
                        // — the divisor rule, not an exception to it (§9). The
                        // choice needs the value's type, so it is deferred.
                        AggOp::Avg => self.reducers.push(ReducerConstraint {
                            at: self.at,
                            op: AggOp::Avg,
                            result: result_slot,
                            value,
                        }),
                        // min / max : the same type as the collected value, which
                        // may be any single ordered type (every primitive is
                        // ordered, §8) — so no numeric constraint.
                        AggOp::Min | AggOp::Max => self.union(result_slot, value),
                    }
                }
            }
        }
        self.at = clause;
    }

    /// The slot representing an expression's type. Arithmetic unifies both
    /// operands and the result into one numeric class.
    fn expr_slot(&mut self, expr: &ir::Expr, vars: &[usize]) -> usize {
        match expr {
            ir::Expr::Term(ir::Term::Const(value)) => match type_of(value) {
                Some(ty) => {
                    let node = self.fresh(format!("literal {}", type_label(ty)));
                    self.set_type_from(node, ty, Some(print_value(value)));
                    node
                }
                // A bare `absent` literal is type-neutral (§4): a fresh
                // unconstrained node. Comparisons against it are skipped in
                // `body_constraints`, so this node never forces a type.
                None => self.fresh("literal absent".to_string()),
            },
            ir::Expr::Term(ir::Term::Var(var)) => vars[var.0 as usize],
            ir::Expr::Binary { op, lhs, rhs } => {
                let l = self.expr_slot(lhs, vars);
                let r = self.expr_slot(rhs, vars);
                let result = self.fresh(format!("the result of `{}`", arith_symbol(*op)));
                self.binaries.push(BinaryConstraint {
                    at: self.at,
                    op: *op,
                    lhs: l,
                    rhs: r,
                    result,
                });
                result
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
            // A `std` relation types by its operation (§13). Extraction is an
            // `int` whatever it read, which is the cast's shape — inference
            // terminates there. **`truncate` is the exception on purpose**: it
            // returns the same point type it was given, so its result *is* its
            // input's slot, and a group key can never change type with the
            // unit. Which input types are accepted is a value-level question,
            // settled at evaluation exactly as an undefined conversion is.
            ir::Expr::Builtin { op, args } => {
                let arg_slots: Vec<usize> =
                    args.iter().map(|arg| self.expr_slot(arg, vars)).collect();
                match (op, arg_slots.first()) {
                    (crate::ast::BuiltinOp::Truncate, Some(input)) => *input,
                    _ => {
                        let node = self.fresh("the result of a `std/time` extraction".to_string());
                        self.set_type(node, TypeName::Int);
                        node
                    }
                }
            }
        }
    }

    /// The concrete type a slot's class has settled on, if any.
    fn slot_type(&mut self, slot: usize) -> Option<TypeName> {
        let root = self.find(slot);
        self.ty[root].as_ref().map(|fixed| fixed.ty)
    }

    /// Resolves the deferred arithmetic and reducer constraints (§8/§9).
    ///
    /// Two phases, and the split is what keeps a temporal operand from being
    /// mistyped by an eager guess: **first** settle every constraint whose
    /// operands are both known, to a fixpoint — a binary's result types the
    /// expression it feeds, so one pass is not enough — and only **then**, one
    /// at a time, fall back on a constraint that is still undecided. A fallback
    /// can unlock more known-operand work, so the outer loop repeats.
    fn resolve_deferred(&mut self) {
        let mut binaries: Vec<Option<BinaryConstraint>> = std::mem::take(&mut self.binaries)
            .into_iter()
            .map(Some)
            .collect();
        let mut reducers: Vec<Option<ReducerConstraint>> = std::mem::take(&mut self.reducers)
            .into_iter()
            .map(Some)
            .collect();
        loop {
            loop {
                let mut progress = false;
                for pending in binaries.iter_mut() {
                    let Some(constraint) = pending.take() else {
                        continue;
                    };
                    let lhs = self.slot_type(constraint.lhs);
                    let rhs = self.slot_type(constraint.rhs);
                    match (lhs, rhs) {
                        (Some(lhs), Some(rhs)) => {
                            self.settle_binary(&constraint, lhs, rhs);
                            progress = true;
                        }
                        _ => *pending = Some(constraint),
                    }
                }
                for pending in reducers.iter_mut() {
                    let Some(constraint) = pending.take() else {
                        continue;
                    };
                    match self.slot_type(constraint.value) {
                        Some(value) => {
                            self.settle_reducer(&constraint, value);
                            progress = true;
                        }
                        None => *pending = Some(constraint),
                    }
                }
                if !progress {
                    break;
                }
            }
            // Nothing more is decidable from what is known; take one undecided
            // constraint on its fallback and go round again.
            if let Some(slot) = binaries.iter().position(Option::is_some) {
                let constraint = binaries[slot].take().expect("just found");
                self.fall_back_binary(&constraint);
            } else if let Some(slot) = reducers.iter().position(Option::is_some) {
                let constraint = reducers[slot].take().expect("just found");
                self.fall_back_reducer(&constraint);
            } else {
                return;
            }
        }
    }

    /// Types one arithmetic node from both operand types (§8).
    fn settle_binary(&mut self, constraint: &BinaryConstraint, lhs: TypeName, rhs: TypeName) {
        // Deferred resolution runs long after the walk, so each constraint
        // restores the place it was gathered from before it can raise.
        self.at = constraint.at;
        if !lhs.is_temporal() && !rhs.is_temporal() {
            // The homogeneous rule, unchanged: one numeric class for both
            // operands and the result.
            self.union(constraint.lhs, constraint.rhs);
            self.union(constraint.lhs, constraint.result);
            self.numeric.push((constraint.lhs, constraint.at));
            return;
        }
        match temporal_result(constraint.op, lhs, rhs) {
            Some(result) => self.set_type(constraint.result, result),
            None => {
                let (message, suggestion) = temporal_arith_error(constraint.op, lhs, rhs);
                self.at = constraint.at;
                self.raise(ErrorCode::TypeMismatch, message).suggestion =
                    Some(suggestion.to_string());
            }
        }
    }

    /// Types one `sum`/`avg` from the type it folds over (§9).
    fn settle_reducer(&mut self, constraint: &ReducerConstraint, value: TypeName) {
        self.at = constraint.at;
        if !is_numeric(value) && value != TypeName::Duration {
            // A point does not fold: `sum` and `avg` both add, and §8 has no
            // addition of two dates — so the message says that rather than
            // "not numeric", which would misdescribe why.
            let root = self.find(constraint.value);
            let label = self.label[root].clone();
            let reason = if value.is_temporal_point() {
                format!(
                    "{label} has type {} but is folded by `{}`, and two points \
                     in time do not add",
                    type_label(value),
                    agg_symbol(constraint.op),
                )
            } else {
                format!(
                    "{label} has type {} but is folded by `{}`, which requires \
                     int, float, or duration",
                    type_label(value),
                    agg_symbol(constraint.op),
                )
            };
            let suggestion = if value.is_temporal_point() {
                "for a span, subtract two points first; for the earliest or latest, use \
                 `min`/`max`"
            } else {
                "project a numeric column, or convert with `as`"
            };
            self.at = constraint.at;
            self.raise(ErrorCode::TypeMismatch, reason).suggestion = Some(suggestion.to_string());
            return;
        }
        if constraint.op == AggOp::Avg {
            // `avg` is `int → float` because a mean of numbers is not an
            // integer; a mean of durations *is* a duration, since dividing one
            // by a count is scaling (§8). Not an exception to the rule — the
            // rule's other half.
            let result = if value == TypeName::Duration {
                TypeName::Duration
            } else {
                TypeName::Float
            };
            self.set_type(constraint.result, result);
        }
    }

    /// An arithmetic node nothing else typed. With no temporal type in
    /// evidence this is the homogeneous rule as before; with one, the operator
    /// is genuinely ambiguous and says so.
    fn fall_back_binary(&mut self, constraint: &BinaryConstraint) {
        self.at = constraint.at;
        let known: Vec<TypeName> = [constraint.lhs, constraint.rhs, constraint.result]
            .into_iter()
            .filter_map(|slot| self.slot_type(slot))
            .collect();
        if known.iter().any(|ty| ty.is_temporal()) {
            self.errors.push(
                Error::new(
                    ErrorCode::TypeMismatch,
                    format!(
                        "cannot tell what `{}` means here — {} is temporal, and \
                     the other operand's type is never fixed, so the result could be a \
                     point or a duration (§8)",
                        arith_symbol(constraint.op),
                        known
                            .iter()
                            .find(|ty| ty.is_temporal())
                            .map(|ty| type_label(*ty))
                            .expect("just found one"),
                    ),
                )
                .suggest(
                    "give the other operand a type — a temporal literal, a column, or an `as` cast",
                ),
            );
            return;
        }
        self.union(constraint.lhs, constraint.rhs);
        self.union(constraint.lhs, constraint.result);
        self.numeric.push((constraint.lhs, constraint.at));
    }

    /// A `sum`/`avg` whose collected value nothing typed: the pre-temporal
    /// rule, which is still right when no duration is in evidence.
    fn fall_back_reducer(&mut self, constraint: &ReducerConstraint) {
        self.at = constraint.at;
        if self.slot_type(constraint.result) == Some(TypeName::Duration) {
            // `avg` over an untyped value feeding a duration column: only a
            // duration folds to one.
            self.set_type(constraint.value, TypeName::Duration);
            return;
        }
        if constraint.op == AggOp::Avg {
            self.set_type(constraint.result, TypeName::Float);
        }
        self.numeric.push((constraint.value, constraint.at));
    }

    /// Runs the deferred numeric checks and builds the [`TypeEnv`], or returns
    /// the deduplicated type errors.
    fn finish(mut self) -> std::result::Result<TypeEnv, Vec<Error>> {
        self.resolve_deferred();
        for (node, at) in std::mem::take(&mut self.numeric) {
            let root = self.find(node);
            if let Some(ty) = self.ty[root].as_ref().map(|fixed| fixed.ty)
                && !is_numeric(ty)
            {
                let message = format!(
                    "{} has type {} but is used in arithmetic or a numeric \
                     aggregate (`sum`/`avg`), which requires int or float",
                    self.label[root],
                    type_label(ty),
                );
                self.at = at;
                self.raise(ErrorCode::TypeMismatch, message);
            }
        }
        // Verify asserted declared types (§4) against the inferred ones. A
        // declared type that contradicts what inference derived is an error
        // naming the column; a declared column inference never constrained is
        // simply unrefuted (no error).
        //
        // Only once nothing else has failed. A reported conflict still merges:
        // `union` keeps the first operand's type and `set_type` keeps the
        // incumbent, so the class carries one of the two and *every* column
        // conclusion drawn from it afterwards is unreliable — not only the ones
        // that happen to look wrong (`bugs/resolved/008`, §17 2026-08-24).
        if self.errors.is_empty() {
            for (p, info) in self.program.predicates.iter().enumerate() {
                let Some(declared) = &info.field_types else {
                    continue;
                };
                for (col, declared_ty) in declared.iter().enumerate() {
                    let Some(declared_ty) = *declared_ty else {
                        continue;
                    };
                    let root = self.find(self.col_base[p] + col);
                    if let Some(inferred) = self.ty[root].as_ref().map(|fixed| fixed.ty)
                        && inferred != declared_ty
                    {
                        let message = format!(
                            "{} is declared as {} but {}",
                            column_label(info, col),
                            type_label(declared_ty),
                            contradiction(self.program, p, col, inferred),
                        );
                        // The declaration is the place: it is the half of the
                        // contradiction this message asserts is wrong.
                        self.at = info.decl_span;
                        self.raise(ErrorCode::DeclaredTypeMismatch, message);
                    }
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
                            .as_ref()
                            .map(|fixed| fixed.ty)
                            .or_else(|| info.field_types.as_ref().and_then(|types| types[col]))
                    })
                    .collect()
            })
            .collect();
        Ok(TypeEnv { columns })
    }
}

/// How a column contradicts its declaration, worded from what is actually
/// known. Inference reaches a column from two directions — the values in the
/// fact table, and the way rules use it — and only the first is a claim about
/// data.
///
/// So the message says "its values are" **only** when the facts, read on their
/// own, say so. Otherwise the contradiction came from a rule and the wording is
/// about use. Saying "its values are" of a relation with no facts at all was
/// the second half of `bugs/resolved/008`.
fn contradiction(program: &ir::Program, pred: usize, col: usize, inferred: TypeName) -> String {
    if column_type_from_facts(program, pred, col) == Some(inferred) {
        format!("its values are {}", type_label(inferred))
    } else {
        format!("is used as {}", type_label(inferred))
    }
}

/// The type a column's **facts** give it, read from the fact table and nothing
/// else — no unification, no rule constraints. `None` when the column holds no
/// typed value, or when its values disagree.
///
/// Imported rows are ordinary facts by this point (§13 materializes them during
/// lowering), so an imported column's values are read here too.
fn column_type_from_facts(program: &ir::Program, pred: usize, col: usize) -> Option<TypeName> {
    let mut found: Option<TypeName> = None;
    for fact in &program.facts {
        if fact.pred.0 as usize != pred {
            continue;
        }
        let Some(ty) = fact.tuple.0.get(col).and_then(type_of) else {
            continue;
        };
        match found {
            None => found = Some(ty),
            Some(seen) if seen == ty => {}
            // Values that disagree are not a claim anything can rest on.
            Some(_) => return None,
        }
    }
    found
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
