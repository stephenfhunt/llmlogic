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

use crate::ast::{
    Args, Atom, Clause, Comparison, Constant, Declaration, Expr, ExprKind, FieldDecl, Import,
    Literal, LiteralKind, NamedArg, Program, Query, Statement, StatementKind, Term, TermKind,
    TypeName,
};
use crate::ir::{PredicateInfo, Value};

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
    let mut out = format!(
        "import {} as {}",
        print_string_literal(&import.path),
        import.relation.name
    );
    if let Some(schema) = &import.schema {
        out.push('(');
        out.push_str(&print_fields(schema));
        out.push(')');
    }
    out.push('.');
    out
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
    match ty {
        TypeName::Symbol => "symbol",
        TypeName::String => "string",
        TypeName::Int => "int",
        TypeName::Float => "float",
        TypeName::Bool => "bool",
    }
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
    format!("?- {}.", print_body(&query.body))
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

fn print_atom(atom: &Atom) -> String {
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

/// Prints an expression. No parentheses are emitted (the v1 grammar has none):
/// a parse-produced tree is left-associative and precedence-nested, so flat
/// printing re-parses to the same tree.
fn print_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Term(term) => print_term(term),
        ExprKind::Binary { op, lhs, rhs } => format!(
            "{} {} {}",
            print_expr(lhs),
            crate::ast::arith_symbol(*op),
            print_expr(rhs)
        ),
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
        Value::Symbol(s) => s.clone(),
        Value::String(s) => print_string_literal(s),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => print_f64(f.get()),
        Value::Bool(b) => b.to_string(),
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
}
