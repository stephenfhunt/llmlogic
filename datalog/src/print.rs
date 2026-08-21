//! Canonical Datalog printer (`spec.md` §14).
//!
//! Two audiences:
//!
//! - [`print_program`] renders a surface [`ast::Program`] back to Datalog text.
//!   It is the round-trip partner of the parser (`testing.md` D2/D3): a
//!   parse-reachable AST prints to text that re-parses to the same AST.
//! - [`print_ground_fact`] / [`print_fact_lines`] render **ground facts** in the
//!   §14 canonical output form — one fact per line, values in the canonical
//!   cross-type order the evaluator already stores them in. This is what makes
//!   query output valid input (the Datalog-in/Datalog-out closure, `testing.md`
//!   D1): the binary prints answers this way and they parse straight back.
//!
//! Floats print via `{:?}` so an integer-valued float keeps its decimal point
//! (`1.0`, not `1`) and therefore re-lexes as a float, not an int — required
//! for the closure property.

use crate::ast::AggOp;
use crate::ast::{
    Args, Atom, Clause, Comparison, Constant, Declaration, Expr, ExprKind, FieldDecl, Import,
    ImportKind, Literal, LiteralKind, NamedArg, Program, Query, Statement, StatementKind, Term,
    TermKind, TypeName,
};
use crate::ir::{self, PredicateInfo, Value};
use crate::provenance::{NoMatchPattern, ProofTree};

/// Renders a surface program to canonical Datalog text, one statement per line.
pub fn print_program(program: &Program) -> String {
    let mut out = String::new();
    for statement in &program.statements {
        out.push_str(&print_statement(statement));
        out.push('\n');
    }
    out
}

fn print_statement(statement: &Statement) -> String {
    match &statement.kind {
        StatementKind::Import(import) => print_import(import),
        StatementKind::Declare(declaration) => print_declare(declaration),
        StatementKind::Clause(clause) => print_clause(clause),
        StatementKind::Query(query) => print_query(query),
    }
}

fn print_import(import: &Import) -> String {
    match &import.kind {
        ImportKind::Module => format!("import {}.", print_string_literal(&import.path)),
        // A `std` import prints as the module import it was written as, so the
        // closure property holds through resolution (§14).
        ImportKind::Std { module } => format!("import \"std/{module}\"."),
        ImportKind::Data {
            table,
            relation,
            schema,
        } => {
            let mut out = format!("import {}", print_string_literal(&import.path));
            if let Some((table, _)) = table {
                out.push_str(&format!(" table {}", print_string_literal(table)));
            }
            out.push_str(&format!(" as {}", relation.name));
            if let Some(schema) = schema {
                out.push('(');
                out.push_str(&print_fields(schema));
                out.push(')');
            }
            out.push('.');
            out
        }
    }
}

fn print_declare(declaration: &Declaration) -> String {
    format!(
        "declare {}({}).",
        declaration.relation.name,
        print_fields(&declaration.fields)
    )
}

fn print_fields(fields: &[FieldDecl]) -> String {
    fields
        .iter()
        .map(|field| match field.ty {
            Some(ty) => format!("{}: {}", field.name.name, print_type(ty)),
            None => field.name.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_type(ty: TypeName) -> &'static str {
    // The spelling is [`TypeName::keyword`]'s: printing a type and parsing one
    // must agree for the closure property, so there is one list, not two.
    ty.keyword()
}

fn print_clause(clause: &Clause) -> String {
    if clause.body.is_empty() {
        format!("{}.", print_atom(&clause.head))
    } else {
        format!(
            "{} :- {}.",
            print_atom(&clause.head),
            print_body(&clause.body)
        )
    }
}

fn print_query(query: &Query) -> String {
    match &query.name {
        Some(name) => format!("?- {}: {}.", name.name, print_body(&query.body)),
        None => format!("?- {}.", print_body(&query.body)),
    }
}

fn print_body(body: &[Literal]) -> String {
    body.iter()
        .map(print_literal)
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_literal(literal: &Literal) -> String {
    match &literal.kind {
        LiteralKind::Atom { negated, atom } => {
            if *negated {
                format!("not {}", print_atom(atom))
            } else {
                print_atom(atom)
            }
        }
        LiteralKind::Comparison(comparison) => print_comparison(comparison),
        LiteralKind::Presence { expr, negated } => {
            let op = if *negated {
                "is not absent"
            } else {
                "is absent"
            };
            format!("{} {op}", print_expr(expr))
        }
    }
}

fn print_comparison(comparison: &Comparison) -> String {
    format!(
        "{} {} {}",
        print_expr(&comparison.lhs),
        crate::ast::cmp_symbol(comparison.op),
        print_expr(&comparison.rhs)
    )
}

/// Renders a single atom to canonical Datalog text (no trailing `.`). Public so
/// the CLI can print the head of a define-and-select `-q` rule as a synthesized
/// query (`src/api.rs`).
pub fn print_atom(atom: &Atom) -> String {
    let args = match &atom.args {
        Args::Positional(exprs) => exprs.iter().map(print_expr).collect::<Vec<_>>().join(", "),
        Args::Named(named) => named
            .iter()
            .map(print_named_arg)
            .collect::<Vec<_>>()
            .join(", "),
    };
    format!("{}({})", atom.predicate.name, args)
}

fn print_named_arg(arg: &NamedArg) -> String {
    format!("{}: {}", arg.field.name, print_expr(&arg.value))
}

/// Prints an expression, parenthesizing exactly where the grouping would
/// otherwise be lost on re-parse (`testing.md` D2/D3).
///
/// A binary child is wrapped when it binds **looser** than its parent, and —
/// since all four operators are left-associative — when it binds *equally* and
/// sits on the **right**: `A - (B - C)` must keep its parentheses, while
/// `(A - B) - C` is what flat printing already re-parses to. Terms and
/// aggregates are atomic and never wrapped.
///
/// A cast binds tightest of all (§5), so it never needs wrapping as an operand;
/// what it does need is a wrapped **operand** of its own, since the grammar
/// admits only a `primary` there — see [`print_cast_operand`].
fn print_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Term(term) => print_term(term),
        // `expr as type` (§8). Left-to-right chaining means a nested cast on the
        // left needs no parentheses: `X as int as float` re-parses as itself.
        ExprKind::Cast { expr, ty, .. } => {
            format!("{} as {}", print_cast_operand(expr), ty.keyword())
        }
        ExprKind::Binary { op, lhs, rhs } => format!(
            "{} {} {}",
            print_operand(lhs, *op, Side::Left),
            crate::ast::arith_symbol(*op),
            print_operand(rhs, *op, Side::Right)
        ),
        // A set-builder aggregate `op { expr | goal }` (§9). `params` is empty for
        // the v1 five; when parameterised reducers land they render before `{`.
        ExprKind::Aggregate(agg) => format!(
            "{} {{ {} | {} }}",
            agg.op.keyword(),
            print_expr(&agg.expr),
            print_body(&agg.goal)
        ),
    }
}

/// Which operand of its parent a sub-expression is. Left-associativity makes
/// the two sides differ at equal precedence.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
}

/// Prints `expr` as an operand of `parent`, adding parentheses only when
/// omitting them would re-parse to a different tree.
fn print_operand(expr: &Expr, parent: crate::ast::ArithOp, side: Side) -> String {
    let text = print_expr(expr);
    let needs_parens = match &expr.kind {
        ExprKind::Binary { op, .. } => {
            op.precedence() < parent.precedence()
                || (op.precedence() == parent.precedence() && side == Side::Right)
        }
        // A cast binds tighter than every arithmetic operator, so it is never
        // wrapped as one's operand.
        ExprKind::Term(_) | ExprKind::Aggregate(_) | ExprKind::Cast { .. } => false,
    };
    if needs_parens {
        format!("({text})")
    } else {
        text
    }
}

/// Prints the operand of a cast. The grammar is `cast = primary { "as" type }`,
/// so anything looser than a `primary` must be parenthesized or the `as` would
/// re-parse as binding to the operand's *last* factor: `A + B as int` is
/// `A + (B as int)`, which is a different tree from `(A + B) as int`.
///
/// A nested cast is exempt — chaining is left-to-right, so it is already a legal
/// left operand — as are terms and aggregates, which are atomic.
fn print_cast_operand(expr: &Expr) -> String {
    let text = print_expr(expr);
    match &expr.kind {
        ExprKind::Binary { .. } => format!("({text})"),
        ExprKind::Term(_) | ExprKind::Aggregate(_) | ExprKind::Cast { .. } => text,
    }
}

fn print_term(term: &Term) -> String {
    match &term.kind {
        TermKind::Constant(constant) => print_constant(constant),
        TermKind::Variable(name) => name.clone(),
        TermKind::Wildcard => "_".to_string(),
    }
}

fn print_constant(constant: &Constant) -> String {
    match constant {
        Constant::Symbol(s) => s.clone(),
        Constant::String(s) => print_string_literal(s),
        Constant::Int(n) => n.to_string(),
        Constant::Float(f) => print_f64(*f),
        Constant::Bool(b) => b.to_string(),
        Constant::Absent => "absent".to_string(),
        // The sigil is part of the literal: what the printer emits must re-lex
        // to the same constant (§14's closure, `testing.md` D2/D3).
        Constant::Temporal(value) => format!("@{value}"),
    }
}

// --- Ground-fact (§14 output) printing ---

/// Prints one ground fact as canonical Datalog: `pred(v1, v2, …).`.
pub fn print_ground_fact(predicate: &str, values: &[Value]) -> String {
    let args = values
        .iter()
        .map(print_value)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{predicate}({args}).")
}

/// Renders a sequence of facts (as `(PredId index, tuple values)`) to canonical
/// output text, one per line. `predicates` supplies names.
pub fn print_fact_lines<'a>(
    facts: impl IntoIterator<Item = (usize, &'a [Value])>,
    predicates: &[PredicateInfo],
) -> String {
    let mut out = String::new();
    for (pred_index, values) in facts {
        out.push_str(&print_ground_fact(&predicates[pred_index].name, values));
        out.push('\n');
    }
    out
}

/// Prints a ground [`Value`] in canonical form.
pub fn print_value(value: &Value) -> String {
    match value {
        // The missing-data value round-trips as the reserved literal `absent`
        // (§4): Datalog-out is Datalog-in.
        Value::Absent => "absent".to_string(),
        Value::Symbol(s) => s.clone(),
        Value::String(s) => print_string_literal(s),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => print_f64(f.get()),
        Value::Bool(b) => b.to_string(),
        // A temporal value prints as its §3 literal, sigil included, so a
        // computed date composes back as input (§14's closure). The `@` lives
        // here and not in `Display` because `as string` renders the same value
        // *without* it — §8's read/render pair is the unsigilled text.
        Value::Date(d) => format!("@{d}"),
        Value::Timestamp(t) => format!("@{t}"),
        Value::Duration(d) => format!("@{d}"),
    }
}

/// Formats a float so it always carries a `.` or `e` (round-trips as a float,
/// not an int). `{:?}` on `f64` guarantees this for finite values.
fn print_f64(f: f64) -> String {
    format!("{f:?}")
}

/// Renders a string constant with the §3 escapes, double-quoted (the canonical
/// delimiter).
fn print_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// --- Proof (§11) printing ---

/// Renders one proof tree as `%`-comment lines — the §11 explanation surface.
///
/// Each line is already `%`-prefixed and newline-free, so a caller concatenates
/// them into the same stream as §14's answers. That a proof rides in comments is
/// the 2026-08-16 decision — a proof tree is not a fact and joins a fact stream
/// on no terms — so stripping every comment must leave byte-for-byte what the
/// same program prints without its goals, which is what keeps
/// Datalog-out-is-Datalog-in true in the presence of provenance (`testing.md`
/// E5, whose lexical precondition is asserted here).
///
/// **A line carries its depth twice, for two readers** (§17). The leading
/// integer is the structural channel and the indentation is the human's.
/// Parent/child is the entire content of a proof, so it cannot rest on
/// indentation alone: that makes depth a whitespace-run *length*, and an agent
/// reading a linear token stream then compares run lengths several lines apart.
/// An integer is a distinct token per level, makes "deeper" an integer
/// comparison, and lets `grep '^% 1 '` cut one level out. Box-drawing was the
/// other candidate and fails for the same reason: `│` and `└─` mean something
/// to an eye tracking a column down a page and nothing to a reader with no
/// column. The two channels cost each other nothing, so the renderer emits both.
///
/// Nothing elides and nothing is shared: a fact proved in two branches prints
/// twice, and a deep proof is deep. That is §15's no-budget-anywhere decision
/// applied here, not a second policy.
pub fn print_proof(tree: &ProofTree, program: &ir::Program) -> Vec<String> {
    let mut out = Vec::new();
    // The header wears no depth number, which is what keeps it distinct from a
    // node to a reader and to a `grep`.
    if let Some(fact) = tree.fact() {
        out.push(format!("% why {}", print_ir_fact(fact, program)));
    }
    proof_lines(tree, program, 0, None, &mut out);
    out
}

/// Where a premise sat in its parent: the rule, and which body literal it
/// answers. `None` at the root, which answers no literal.
///
/// This is what lets a self-justifying premise — an §8 comparison, a presence
/// test, a §9 aggregate — print the literal it satisfied and not only the values
/// it satisfied it with. The record holds the values alone, so *which* of a
/// body's three comparisons this is has to come from the IR.
type Site<'a> = Option<(&'a ir::Rule, usize)>;

fn proof_lines(
    tree: &ProofTree,
    program: &ir::Program,
    depth: usize,
    site: Site<'_>,
    out: &mut Vec<String>,
) {
    let content = match tree {
        ProofTree::Leaf(fact) => {
            let anchor = match import_path(program, fact.pred) {
                // Relation-level, never row-level: §13 materializes an import
                // into ordinary base facts before lowering, so `ImportSpec` is
                // the finest anchor that exists. The proof says *because
                // `employees.csv`*, never *because row 4,182* (§11).
                Some(path) => format!("[fact from {}]", print_string_literal(path)),
                None => "[fact]".to_string(),
            };
            format!("{}  {anchor}", print_ir_fact(fact, program))
        }
        ProofTree::Derived { fact, rule, .. } => format!(
            "{}  by {}",
            print_ir_fact(fact, program),
            print_ir_rule(&program.rules[rule.0 as usize], program)
        ),
        ProofTree::NoMatch(pattern) => format!("no {}", print_no_match(pattern, program)),
        ProofTree::Builtin { op, lhs, rhs, lost } => {
            let held = format!(
                "{} {} {}",
                print_value(lhs),
                crate::ast::cmp_symbol(*op),
                print_value(rhs)
            );
            let mut text = with_site(site, program, held);
            // §12's *missing* / *malformed* distinction reaches a proof only
            // here: by the time the value model sees them both are `absent`.
            if let Some(lost) = lost {
                let plural = if lost.count == 1 { "value" } else { "values" };
                text.push_str(&format!(
                    "  [{} {plural} lost converting to {}]",
                    lost.count,
                    lost.to.keyword()
                ));
            }
            text
        }
        ProofTree::Presence { value, negated } => {
            let not = if *negated { "not " } else { "" };
            with_site(
                site,
                program,
                format!("{} is {not}absent", print_value(value)),
            )
        }
        ProofTree::Aggregate {
            op,
            value,
            present,
            skipped,
        } => {
            // `count` reads the binding total, absent bindings included (§9: a
            // binding is a binding), so `present`/`skipped` describe a fold it
            // did not do — hence the value alone. `fold_aggregate` forces its
            // `skipped` to 0 for that same reason.
            let held = if *op == AggOp::Count {
                print_value(value)
            } else if *skipped == 0 {
                format!("{} over {present} values", print_value(value))
            } else {
                format!(
                    "{} over {present} values, {skipped} absent skipped",
                    print_value(value)
                )
            };
            // The cited literal already names the operator; without one there is
            // nothing to say which fold this was.
            match site_literal(site, program) {
                Some(literal) => format!("{literal}  ({held})"),
                None => format!("{} {held}", op.keyword()),
            }
        }
    };
    out.push(format!(
        "% {depth}  {:indent$}{content}",
        "",
        indent = depth * 2
    ));

    if let ProofTree::Derived { rule, children, .. } = tree {
        let rule_id = *rule;
        let rule = &program.rules[rule_id.0 as usize];
        for (i, child) in children.iter().enumerate() {
            // `children[i]` proves `premises[i]`, which answers body literal `i`
            // — the `BodyIdx` alignment the whole provenance record rests on.
            proof_lines(child, program, depth + 1, Some((rule, i)), out);
        }
    }
}

/// Pairs the literal a self-justifying premise satisfied with the values it
/// satisfied it with: `A >= 18  (30 >= 18)`.
fn with_site(site: Site<'_>, program: &ir::Program, held: String) -> String {
    match site_literal(site, program) {
        Some(literal) => format!("{literal}  ({held})"),
        None => held,
    }
}

fn site_literal(site: Site<'_>, program: &ir::Program) -> Option<String> {
    let (rule, idx) = site?;
    let literal = rule.body.get(idx)?;
    Some(print_ir_body_literal(literal, &RuleCx::new(program, rule)))
}

/// The path an import bound `pred` to, if one did.
fn import_path(program: &ir::Program, pred: ir::PredId) -> Option<&str> {
    program
        .imports
        .iter()
        .find(|spec| spec.pred == pred)
        .map(|spec| spec.path.as_str())
}

// --- IR printing, for the rule a proof cites ---
//
// §11's proof cites the rule that fired, and it has to be the **lowered** one.
// Premises align index-for-index with `ir::Rule::body`, while lowering hoists a
// compound argument into an `=`-assignment, expands each `;` disjunct into its
// own clause, and turns named arguments positional — so a slice of the source
// span would show a body whose literal count does not match the premise list.
// Reconstruction is also the honest answer: it is what the engine ran.

/// Rendering context for one rule: the names its variable slots carry, and how
/// often each slot occurs.
struct RuleCx<'a> {
    program: &'a ir::Program,
    var_names: &'a [Option<String>],
    /// Occurrences of each slot across head and body. A lowering-generated slot
    /// (`var_names[i]` is `None`) occurring **once** is a wildcard or an omitted
    /// named argument: it constrains nothing, so it prints as `_` and drops out
    /// of the named form entirely. One occurring more than once is a hoisted
    /// temporary and must keep an identity, or `succ(N, _), _ = N + 1` would
    /// spell two different variables the same.
    counts: Vec<u32>,
}

impl<'a> RuleCx<'a> {
    fn new(program: &'a ir::Program, rule: &'a ir::Rule) -> RuleCx<'a> {
        let mut counts = vec![0u32; rule.var_names.len()];
        count_atom(&rule.head, &mut counts);
        for literal in &rule.body {
            count_literal(literal, &mut counts);
        }
        RuleCx {
            program,
            var_names: &rule.var_names,
            counts,
        }
    }

    /// The source name of a slot, or a stand-in for a lowering-generated one.
    ///
    /// A multiply-occurring generated slot takes `_g{slot}`, unique within the
    /// rule. §3 admits `_`-initial variable names, so a program that spells one
    /// `_g3` could collide with it — in a comment, and only there.
    fn var_name(&self, var: ir::Var) -> String {
        let slot = var.0 as usize;
        match self.var_names.get(slot).and_then(Option::as_ref) {
            Some(name) => name.clone(),
            None if self.counts.get(slot).copied().unwrap_or(0) <= 1 => "_".to_string(),
            None => format!("_g{slot}"),
        }
    }

    /// Was this argument omitted at the surface — a slot lowering invented and
    /// nothing else mentions? Such an argument is exactly what §5's partial
    /// selection dropped, so the named form drops it again.
    fn is_omitted(&self, term: &ir::Term) -> bool {
        match term {
            ir::Term::Var(var) => {
                let slot = var.0 as usize;
                self.var_names.get(slot).and_then(Option::as_ref).is_none()
                    && self.counts.get(slot).copied().unwrap_or(0) <= 1
            }
            ir::Term::Const(_) => false,
        }
    }
}

fn count_atom(atom: &ir::Atom, counts: &mut [u32]) {
    for term in &atom.args {
        count_term(term, counts);
    }
}

fn count_term(term: &ir::Term, counts: &mut [u32]) {
    if let ir::Term::Var(var) = term
        && let Some(slot) = counts.get_mut(var.0 as usize)
    {
        *slot += 1;
    }
}

fn count_expr(expr: &ir::Expr, counts: &mut [u32]) {
    match expr {
        ir::Expr::Term(term) => count_term(term, counts),
        ir::Expr::Binary { lhs, rhs, .. } => {
            count_expr(lhs, counts);
            count_expr(rhs, counts);
        }
        ir::Expr::Cast { expr, .. } => count_expr(expr, counts),
        ir::Expr::Builtin { args, .. } => {
            for arg in args {
                count_expr(arg, counts);
            }
        }
    }
}

fn count_literal(literal: &ir::BodyLiteral, counts: &mut [u32]) {
    match &literal.kind {
        ir::BodyLiteralKind::Atom(atom) | ir::BodyLiteralKind::NegAtom(atom) => {
            count_atom(atom, counts)
        }
        ir::BodyLiteralKind::Compare { lhs, rhs, .. } => {
            count_expr(lhs, counts);
            count_expr(rhs, counts);
        }
        ir::BodyLiteralKind::Presence { expr, .. } => count_expr(expr, counts),
        ir::BodyLiteralKind::Aggregate {
            result,
            params,
            expr,
            goal,
            ..
        } => {
            count_term(&ir::Term::Var(*result), counts);
            for param in params {
                count_expr(param, counts);
            }
            count_expr(expr, counts);
            for literal in goal {
                count_literal(literal, counts);
            }
        }
    }
}

/// Prints a lowered rule in its source spelling: `head :- lit, lit`.
fn print_ir_rule(rule: &ir::Rule, program: &ir::Program) -> String {
    let cx = RuleCx::new(program, rule);
    let head = print_ir_atom(&rule.head, &cx);
    if rule.body.is_empty() {
        return head;
    }
    let body = rule
        .body
        .iter()
        .map(|literal| print_ir_body_literal(literal, &cx))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{head} :- {body}")
}

fn print_ir_body_literal(literal: &ir::BodyLiteral, cx: &RuleCx) -> String {
    match &literal.kind {
        ir::BodyLiteralKind::Atom(atom) => print_ir_atom(atom, cx),
        ir::BodyLiteralKind::NegAtom(atom) => format!("not {}", print_ir_atom(atom, cx)),
        ir::BodyLiteralKind::Compare { op, lhs, rhs } => format!(
            "{} {} {}",
            print_ir_expr(lhs, cx),
            crate::ast::cmp_symbol(*op),
            print_ir_expr(rhs, cx)
        ),
        ir::BodyLiteralKind::Presence { expr, negated } => {
            let not = if *negated { "not " } else { "" };
            format!("{} is {not}absent", print_ir_expr(expr, cx))
        }
        // The lowered form of `result = op { expr | goal }` (§9), printed back
        // as the assignment it is.
        ir::BodyLiteralKind::Aggregate {
            result,
            op,
            expr,
            goal,
            ..
        } => {
            let goal = goal
                .iter()
                .map(|literal| print_ir_body_literal(literal, cx))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} = {} {{ {} | {goal} }}",
                cx.var_name(*result),
                op.keyword(),
                print_ir_expr(expr, cx)
            )
        }
    }
}

/// Prints an atom in named form where the predicate has field names, positional
/// otherwise (§11) — and named form drops the arguments §5's partial selection
/// omitted, which is what keeps a proof over a wide imported table readable.
///
/// It falls back to positional when every argument would drop, since §5 bans a
/// 0-arity atom and `p()` is not a form this printer may emit.
fn print_ir_atom(atom: &ir::Atom, cx: &RuleCx) -> String {
    let info = &cx.program.predicates[atom.pred.0 as usize];
    if let Some(fields) = &info.fields {
        let args = atom
            .args
            .iter()
            .zip(fields)
            .filter(|(term, _)| !cx.is_omitted(term))
            .map(|(term, field)| format!("{field}: {}", print_ir_term(term, cx)))
            .collect::<Vec<_>>();
        if !args.is_empty() {
            return format!("{}({})", info.name, args.join(", "));
        }
    }
    let args = atom
        .args
        .iter()
        .map(|term| print_ir_term(term, cx))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({args})", info.name)
}

fn print_ir_term(term: &ir::Term, cx: &RuleCx) -> String {
    match term {
        ir::Term::Const(value) => print_value(value),
        ir::Term::Var(var) => cx.var_name(*var),
    }
}

/// Prints a lowered expression, parenthesizing on the same rule as
/// [`print_operand`] — a child binding looser, or binding equally on the right.
///
/// The justification differs, though: this text rides in a `%` comment and never
/// re-parses, so what parentheses buy here is a reader who can tell `A + B * C`
/// from `(A + B) * C`, not the D2/D3 round-trip property.
fn print_ir_expr(expr: &ir::Expr, cx: &RuleCx) -> String {
    match expr {
        ir::Expr::Term(term) => print_ir_term(term, cx),
        ir::Expr::Binary { op, lhs, rhs } => format!(
            "{} {} {}",
            print_ir_operand(lhs, *op, Side::Left, cx),
            crate::ast::arith_symbol(*op),
            print_ir_operand(rhs, *op, Side::Right, cx)
        ),
        ir::Expr::Cast { expr, ty } => {
            let operand = print_ir_expr(expr, cx);
            // `cast = primary { "as" type }`, so anything looser than a primary
            // must be wrapped; a nested cast chains left-to-right and need not be.
            match **expr {
                ir::Expr::Binary { .. } => format!("({operand}) as {}", ty.keyword()),
                _ => format!("{operand} as {}", ty.keyword()),
            }
        }
        // A `std` relation, lowered to a function of its inputs (§13). The name
        // comes from `stdlib`'s own tables so the two cannot drift.
        ir::Expr::Builtin { op, args } => {
            let args = args
                .iter()
                .map(|arg| print_ir_expr(arg, cx))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({args})", crate::stdlib::op_name(*op))
        }
    }
}

fn print_ir_operand(
    expr: &ir::Expr,
    parent: crate::ast::ArithOp,
    side: Side,
    cx: &RuleCx,
) -> String {
    let text = print_ir_expr(expr, cx);
    let needs_parens = match expr {
        ir::Expr::Binary { op, .. } => {
            op.precedence() < parent.precedence()
                || (op.precedence() == parent.precedence() && side == Side::Right)
        }
        // A cast binds tighter than every arithmetic operator; terms and
        // builtin calls are atomic.
        ir::Expr::Term(_) | ir::Expr::Cast { .. } | ir::Expr::Builtin { .. } => false,
    };
    if needs_parens {
        format!("({text})")
    } else {
        text
    }
}

/// Prints a ground fact in named form where the predicate has field names,
/// positional otherwise (§11).
///
/// Unlike §14's answer stream this never has to re-parse — a proof rides in
/// comments — which is what makes the named form available at all. It is the
/// form that matters on a wide imported table, where eight positional columns
/// say nothing about which is which.
fn print_ir_fact(fact: &ir::Fact, program: &ir::Program) -> String {
    let info = &program.predicates[fact.pred.0 as usize];
    let args = match &info.fields {
        Some(fields) => fields
            .iter()
            .zip(&fact.tuple.0)
            .map(|(field, value)| format!("{field}: {}", print_value(value)))
            .collect::<Vec<_>>(),
        None => fact.tuple.0.iter().map(print_value).collect::<Vec<_>>(),
    };
    // Not [`print_ground_fact`], which terminates with the `.` that makes an
    // answer line a statement. A proof cites a fact mid-sentence — `p(1)  by …`
    // — so the terminator would be punctuation in the wrong place, and the two
    // arms have to agree about it either way.
    format!("{}({})", info.name, args.join(", "))
}

/// Prints the pattern no fact matched (§7) — `_` for a slot the negation left
/// open, the value for one the rule's bindings closed.
fn print_no_match(pattern: &NoMatchPattern, program: &ir::Program) -> String {
    let info = &program.predicates[pattern.pred.0 as usize];
    let slot = |value: &Option<Value>| match value {
        Some(value) => print_value(value),
        None => "_".to_string(),
    };
    match &info.fields {
        Some(fields) => {
            let args = fields
                .iter()
                .zip(&pattern.args)
                .map(|(field, value)| format!("{field}: {}", slot(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({args})", info.name)
        }
        None => {
            let args = pattern.args.iter().map(slot).collect::<Vec<_>>().join(", ");
            format!("{}({args})", info.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::fixtures;
    use crate::parser::parse;

    /// D3-style round trip over the §16 corpus: printing a parsed program and
    /// re-parsing yields the same AST (modulo spans). The fixtures already
    /// carry `Span::DUMMY`, so comparing parsed-then-reparsed against the
    /// fixture pins both directions.
    fn round_trips(program: &Program) {
        let text = print_program(program);
        let reparsed =
            parse(&text).unwrap_or_else(|e| panic!("re-parse failed for:\n{text}\n{e:?}"));
        // Compare via printing again — a stable canonical form is idempotent.
        assert_eq!(print_program(&reparsed), text);
    }

    #[test]
    fn corpus_round_trips() {
        round_trips(&fixtures::example_16_1());
        round_trips(&fixtures::example_16_2());
        round_trips(&fixtures::example_16_7());
    }

    /// The three import shapes of §13 print canonically and re-parse to the
    /// same text (the D3 fixpoint, statement-level).
    #[test]
    fn import_forms_round_trip() {
        for src in [
            "import \"lib/family.dl\".\n",
            "import \"data/parents.csv\" as parent.\n",
            "import \"analytics.duckdb\" table \"orders\" as order.\n",
            "import \"db.sqlite\" table \"t\" as t(a: int, b: string).\n",
        ] {
            let program = parse(src).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e:?}"));
            assert_eq!(print_program(&program), src);
        }
    }

    #[test]
    fn floats_keep_their_decimal_point() {
        assert_eq!(
            print_value(&Value::Float(crate::ir::F64::new(1.0).unwrap())),
            "1.0"
        );
        assert_eq!(
            print_value(&Value::Float(crate::ir::F64::new(-0.5).unwrap())),
            "-0.5"
        );
    }

    #[test]
    fn ground_fact_is_canonical_datalog() {
        let fact = print_ground_fact(
            "ancestor",
            &[Value::String("alice".into()), Value::String("bob".into())],
        );
        assert_eq!(fact, "ancestor(\"alice\", \"bob\").");
        // …and it parses straight back.
        assert!(parse(&fact).is_ok());
    }

    #[test]
    fn absent_prints_and_reparses_as_the_reserved_literal() {
        assert_eq!(print_value(&Value::Absent), "absent");
        // Datalog-out is Datalog-in: a fact carrying absent parses straight back.
        let fact = print_ground_fact("m", &[Value::String("bread".into()), Value::Absent]);
        assert_eq!(fact, "m(\"bread\", absent).");
        assert!(parse(&fact).is_ok());
    }

    #[test]
    fn aggregate_rule_round_trips() {
        // Datalog-out is Datalog-in for a set-builder aggregate (§9): the
        // canonical print re-parses to the same text (D3 fixpoint).
        let src = "cc(P, N) :- parent(P, _), N = count { C | parent(P, C) }.\n";
        let program = parse(src).unwrap_or_else(|e| panic!("parse: {e:?}"));
        assert_eq!(print_program(&program), src);
    }

    #[test]
    fn symbols_and_strings_print_distinctly() {
        assert_eq!(print_value(&Value::Symbol("red".into())), "red");
        assert_eq!(print_value(&Value::String("red".into())), "\"red\"");
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(
            print_value(&Value::String("a\"b\\c\nd".into())),
            "\"a\\\"b\\\\c\\nd\""
        );
    }

    // --- Proof (§11) rendering ---

    /// Renders the proof of one fact, end to end from source: the pipeline a
    /// `?why` goal will drive once §5 has a form for one.
    ///
    /// Facts are addressed by predicate name so a test reads as the program
    /// does; `PredId` is an interning artifact and pinning one here would make
    /// these tests depend on lowering's numbering.
    fn proof_of(src: &str, predicate: &str, args: &[Value]) -> String {
        let ast = parse(src).expect("parses");
        let program = crate::lower::lower(&ast).expect("lowers");
        let model = crate::engine::eval(&program).expect("evaluates");
        let pred = program
            .predicates
            .iter()
            .position(|info| info.name == predicate)
            .unwrap_or_else(|| panic!("no predicate `{predicate}`"));
        let fact = ir::Fact {
            pred: ir::PredId(pred as u32),
            tuple: ir::Tuple(args.to_vec()),
        };
        let tree = ProofTree::explain(&model, &fact).expect("fact holds");
        print_proof(&tree, &program).join("\n")
    }

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }

    /// §16.6 — the worked example, and the block that example claims. Recursion
    /// unwinds to base facts, each node cites the lowered rule that fired, and
    /// depth rides in both channels (§17).
    #[test]
    fn example_16_6_proof_renders_as_the_spec_shows() {
        // §16.1's program verbatim, since §16.6 asks its question of that one:
        // an example is a claim about the engine, and this is the test that
        // runs it.
        let proof = proof_of(
            "parent(\"alice\", \"bob\").\n\
             parent(\"bob\", \"carol\").\n\
             parent(\"carol\", \"dave\").\n\
             ancestor(X, Y) :- parent(X, Y).\n\
             ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).\n\
             ?- ancestor(\"alice\", Who).\n",
            "ancestor",
            &[text("alice"), text("carol")],
        );
        assert_eq!(
            proof,
            "% why ancestor(\"alice\", \"carol\")\n\
             % 0  ancestor(\"alice\", \"carol\")  by ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)\n\
             % 1    parent(\"alice\", \"bob\")  [fact]\n\
             % 1    ancestor(\"bob\", \"carol\")  by ancestor(X, Y) :- parent(X, Y)\n\
             % 2      parent(\"bob\", \"carol\")  [fact]"
        );
    }

    /// A declared schema puts a fact in named form, and a comparison premise
    /// prints the literal it satisfied beside the values it satisfied it with —
    /// the record holds only the values, so the literal comes from the IR.
    #[test]
    fn a_schema_names_fields_and_a_builtin_cites_its_literal() {
        let proof = proof_of(
            "declare person(name: string, age: int).\n\
             person(\"alice\", 30).\n\
             adult(N) :- person(name: N, age: A), A >= 18.\n",
            "adult",
            &[text("alice")],
        );
        assert_eq!(
            proof,
            "% why adult(\"alice\")\n\
             % 0  adult(\"alice\")  by adult(N) :- person(name: N, age: A), A >= 18\n\
             % 1    person(name: \"alice\", age: 30)  [fact]\n\
             % 1    A >= 18  (30 >= 18)"
        );
    }

    /// The explainability pillar's sharpest case: a negated literal proves by
    /// the pattern nothing matched, and the branch stops there (§7/§11). The
    /// wildcard occurs once, so it prints as the `_` the program wrote.
    #[test]
    fn a_negated_premise_proves_by_its_no_match_pattern() {
        let proof = proof_of(
            "parent(\"alice\", \"bob\").\n\
             person(\"alice\").\n\
             person(\"bob\").\n\
             root(X) :- person(X), not parent(_, X).\n",
            "root",
            &[text("alice")],
        );
        assert_eq!(
            proof,
            "% why root(\"alice\")\n\
             % 0  root(\"alice\")  by root(X) :- person(X), not parent(_, X)\n\
             % 1    person(\"alice\")  [fact]\n\
             % 1    no parent(_, \"alice\")"
        );
    }

    /// §9's skip report reaching a proof: the fold's value, how many values it
    /// folded, and how many `absent` inputs it stepped over.
    #[test]
    fn an_aggregate_premise_reports_its_fold_and_its_skips() {
        let proof = proof_of(
            "m(\"a\", 1).\n\
             m(\"a\", absent).\n\
             m(\"b\", 5).\n\
             total(T) :- T = sum { C | m(_, C) }.\n",
            "total",
            &[Value::Int(6)],
        );
        assert!(
            proof.contains("(6 over 2 values, 1 absent skipped)"),
            "{proof}"
        );
    }

    /// `count` reads the binding total, absent bindings included (§9), so it
    /// reports the value alone — `present`/`skipped` describe a fold it did not
    /// do.
    #[test]
    fn count_reports_its_value_alone() {
        let proof = proof_of(
            "m(\"a\", 1).\n\
             m(\"a\", absent).\n\
             n(N) :- N = count { C | m(_, C) }.\n",
            "n",
            &[Value::Int(2)],
        );
        assert!(proof.contains("| m(_, C) }  (2)"), "{proof}");
    }

    /// §12's *missing* / *malformed* distinction reaching a proof. Both are
    /// `absent` in the value model, so the premise record and the diagnostic are
    /// the only two places it survives.
    #[test]
    fn a_conversion_that_failed_on_data_says_so() {
        let proof = proof_of(
            "declare m(food: string, amount: string).\n\
             m(\"rice\", \"abc\").\n\
             recorded(F, V) :- m(food: F, amount: A), V = A as int.\n",
            "recorded",
            &[text("rice"), Value::Absent],
        );
        assert!(
            proof.contains("V = A as int  (absent = absent)  [1 value lost converting to int]"),
            "{proof}"
        );
    }

    /// A presence test prints in the same literal-then-values shape a
    /// comparison does — both are self-justifying premises (§8).
    #[test]
    fn a_presence_premise_cites_its_literal() {
        let proof = proof_of(
            "m(\"kale\", 7).\n\
             known(F) :- m(F, A), A is not absent.\n",
            "known",
            &[text("kale")],
        );
        assert!(
            proof.contains("A is not absent  (7 is not absent)"),
            "{proof}"
        );
    }

    /// A base fact that is also derivable explains as a leaf, and a leaf says so
    /// (`ProofTree::explain`'s base-first rule).
    #[test]
    fn a_base_fact_is_a_leaf() {
        let proof = proof_of(
            "parent(\"alice\", \"bob\").\n\
             ancestor(X, Y) :- parent(X, Y).\n",
            "parent",
            &[text("alice"), text("bob")],
        );
        assert_eq!(
            proof,
            "% why parent(\"alice\", \"bob\")\n% 0  parent(\"alice\", \"bob\")  [fact]"
        );
    }

    /// A slot lowering invented and nothing else mentions is the argument §5's
    /// partial selection omitted, so the named form omits it again — otherwise a
    /// proof over a wide table is mostly `_`. A hoisted temporary occurs twice
    /// and keeps an identity instead.
    #[test]
    fn an_omitted_argument_stays_omitted_and_a_temporary_keeps_its_name() {
        let proof = proof_of(
            "declare e(a: int, b: int, c: int).\n\
             e(1, 2, 3).\n\
             q(N) :- e(b: 2, c: N).\n",
            "q",
            &[Value::Int(3)],
        );
        assert!(proof.contains("by q(N) :- e(b: 2, c: N)"), "{proof}");
        // The *fact* keeps every column — a row is not a projection.
        assert!(proof.contains("e(a: 1, b: 2, c: 3)  [fact]"), "{proof}");

        let hoisted = proof_of(
            "n(1).\n\
             succ(X, Y) :- n(X), Y = X + 1.\n",
            "succ",
            &[Value::Int(1), Value::Int(2)],
        );
        assert!(
            hoisted.contains("succ(X, Y) :- n(X), Y = X + 1"),
            "{hoisted}"
        );
    }

    // --- Phase E (E7–E8) properties (testing.md) ---

    mod properties {
        use proptest::prelude::*;

        use super::super::*;
        use crate::engine::eval;
        use crate::provenance::ProofTree;
        use crate::testgen::arb_program_with_edb;

        /// Every proof of every derived fact in `program`, as rendered lines.
        fn all_proof_lines(program: &ir::Program) -> Vec<Vec<String>> {
            let model = eval(program).expect("evaluates");
            model
                .facts()
                .filter(|fact| !model.is_base(fact))
                .map(|fact| {
                    let tree = ProofTree::explain(&model, &fact).expect("a held fact has a proof");
                    print_proof(&tree, program)
                })
                .collect()
        }

        /// The depth a line declares, read back off its `% {n}  ` prefix.
        fn declared_depth(line: &str) -> Option<usize> {
            line.strip_prefix("% ")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        }

        /// The depth of every node of `tree`, in the order [`print_proof`] emits
        /// them — computed here by an independent walk, so agreeing with the
        /// renderer is a claim and not a restatement.
        fn node_depths(tree: &ProofTree, depth: usize, out: &mut Vec<usize>) {
            out.push(depth);
            if let ProofTree::Derived { children, .. } = tree {
                for child in children {
                    node_depths(child, depth + 1, out);
                }
            }
        }

        proptest! {
            /// **E7 — every proof line is a comment.** The lexical precondition
            /// E5 rests on: a proof rides in `%` comments (§17, 2026-08-16), so
            /// stripping comments can only leave the fact stream untouched if
            /// every line the renderer emits *is* one. A value carrying a
            /// newline would end the comment and put arbitrary text into the
            /// stream as program syntax; §3's string escapes are what stop it,
            /// and this is where that is asserted rather than assumed.
            ///
            /// E5 itself — the byte-for-byte stripping guard — still waits on a
            /// §5 form for a goal, since there is no program-level output to
            /// strip until one exists.
            ///
            /// *Mutation-verified* (testing.md rule 3): dropping the `%` from
            /// the node line's format string reddens this and leaves E8 green,
            /// which is the split the two properties are for.
            #[test]
            fn e7_every_proof_line_is_a_comment(program in arb_program_with_edb()) {
                for proof in all_proof_lines(&program) {
                    for line in proof {
                        prop_assert!(line.starts_with('%'), "not a comment: {line:?}");
                        prop_assert!(!line.contains('\n'), "line contains a newline: {line:?}");
                    }
                }
            }

            /// **E8 — the depth number agrees with the tree.** What makes the
            /// leading integer load-bearing rather than a remark about the
            /// indentation beside it (§17): read back off each line it must
            /// equal that node's actual depth, so a reader may navigate by the
            /// number alone.
            ///
            /// *Mutation-verified* (testing.md rule 3): freezing the emitted
            /// number at `0` — every line still a well-formed comment, still
            /// correctly indented — reddens this and leaves E7 green. That is
            /// the mutant the indentation alone could not catch, and the reason
            /// the number is asserted against the tree rather than against the
            /// spaces beside it.
            #[test]
            fn e8_declared_depth_is_the_nodes_depth(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts().filter(|f| !model.is_base(f)) {
                    let tree = ProofTree::explain(&model, &fact).unwrap();
                    let mut want = Vec::new();
                    node_depths(&tree, 0, &mut want);
                    // The header carries no depth number, which is what keeps it
                    // distinct from a node; every line after it is one node.
                    let lines = print_proof(&tree, &program);
                    let got: Vec<usize> = lines[1..]
                        .iter()
                        .map(|line| declared_depth(line).expect("a node line declares a depth"))
                        .collect();
                    prop_assert_eq!(got, want);
                    prop_assert!(declared_depth(&lines[0]).is_none(), "header wears a depth");
                }
            }
        }

        /// **Non-vacuity for E7/E8** (testing.md rule 2). Both properties are
        /// satisfied trivially by a program with nothing to explain, and
        /// `arb_program_with_edb` is free to generate one. This pins that the
        /// generator reaches a *nested* proof — depth ≥ 1 — since a run of
        /// single-line proofs would exercise neither the recursion nor the
        /// distinction E8 states.
        #[test]
        fn the_generator_reaches_a_nested_proof() {
            use proptest::strategy::{Strategy, ValueTree};
            use proptest::test_runner::TestRunner;

            let mut runner = TestRunner::deterministic();
            let deepest = (0..256)
                .filter_map(|_| {
                    let program = arb_program_with_edb().new_tree(&mut runner).ok()?.current();
                    all_proof_lines(&program)
                        .iter()
                        .filter_map(|proof| proof.last().and_then(|line| declared_depth(line)))
                        .max()
                })
                .max()
                .unwrap_or(0);
            assert!(
                deepest >= 1,
                "generator produced no proof deeper than one node"
            );
        }
    }
}
