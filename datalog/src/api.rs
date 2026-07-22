//! Programmatic / agent API (`spec.md` §14).
//!
//! [`run`] is the first production entry path: `parse → lower → typecheck →
//! eval`, then each query answered in canonical Datalog. It is the in-process
//! form of what the `datalog` binary does; integration and system tests drive
//! it directly.
//!
//! ## Query output shape (spec §14, decision, Phase D)
//!
//! Answers print as **valid Datalog facts** so output composes as input (the
//! Datalog-in/Datalog-out closure, `testing.md` D1):
//!
//! - A **single positive-atom** query re-emits that atom with the answer
//!   bindings substituted — `?- ancestor("alice", Who).` yields
//!   `ancestor("alice", "bob").` … — provided every variable position is a
//!   named (projected) variable. A fully ground such query prints the atom once
//!   if it holds, nothing otherwise.
//! - Any **other** body (multiple literals, or a wildcard in the sole atom)
//!   emits synthesized `answer/N` facts over the query's named variables.
//! - A body with **no named variables** that is not a substitutable single atom
//!   produces no fact-shaped output in v1 (an existence check with nowhere to
//!   put the answer); this is the one shape the closure does not cover yet.
//!
//! Rows are deduplicated and printed in the canonical value order.
//!
//! The full agent CLI — `-q`, `--format json`, the skill definition — is
//! roadmap step 6; JSON stays reserved for the machine-readable edges (§12
//! errors, §11 provenance).

use crate::engine::{Model, eval};
use crate::error::Error;
use crate::ir;
use crate::lower::lower;
use crate::parser::parse;
use crate::print::print_ground_fact;
use crate::typecheck::typecheck;

/// The result of a successful [`run`]: the least model plus each query's
/// canonical answer lines, in program (statement) order.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub model: Model,
    /// One entry per query, each a list of canonical fact lines (no trailing
    /// newline).
    pub answers: Vec<Vec<String>>,
}

impl RunResult {
    /// All answer lines across every query, joined with newlines (a trailing
    /// newline is added iff there is any output). This is the binary's stdout.
    pub fn output(&self) -> String {
        let mut out = String::new();
        for lines in &self.answers {
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }
}

/// Runs a program end to end: `parse → lower → typecheck → eval`, then answers
/// every query. Returns every error found at the first failing stage (each
/// stage collects all of its own errors).
pub fn run(src: &str) -> Result<RunResult, Vec<Error>> {
    let ast = parse(src)?;
    let program = lower(&ast)?;
    typecheck(&program)?;
    let model = eval(&program).map_err(|e| vec![e])?;

    let mut answers = Vec::with_capacity(program.queries.len());
    for query in &program.queries {
        let rows = model.answer(query).map_err(|e| vec![e])?;
        answers.push(answer_lines(query, &rows, &program));
    }
    Ok(RunResult { model, answers })
}

/// Renders one query's answer rows to canonical fact lines per the §14 output
/// shape. `rows` are the projected named-variable bindings (in named-slot
/// order), as returned by [`Model::answer`].
fn answer_lines(query: &ir::Query, rows: &[Vec<ir::Value>], program: &ir::Program) -> Vec<String> {
    // Slots of the projected (named) variables, in order — this is the order
    // `Model::answer` lays out each row.
    let named_slots: Vec<usize> = query
        .var_names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.is_some())
        .map(|(slot, _)| slot)
        .collect();

    // Substituted-atom form: a single positive atom whose every variable is a
    // projected one.
    if let [literal] = &query.body[..]
        && let ir::BodyLiteralKind::Atom(atom) = &literal.kind
        && atom.args.iter().all(|term| match term {
            ir::Term::Const(_) => true,
            ir::Term::Var(var) => query.var_names[var.0 as usize].is_some(),
        })
    {
        let name = &program.pred_info(atom.pred).name;
        let mut tuples: Vec<Vec<ir::Value>> = rows
            .iter()
            .map(|row| {
                atom.args
                    .iter()
                    .map(|term| match term {
                        ir::Term::Const(value) => value.clone(),
                        ir::Term::Var(var) => {
                            let position = named_slots
                                .iter()
                                .position(|slot| *slot == var.0 as usize)
                                .expect("a projected variable is in named_slots");
                            row[position].clone()
                        }
                    })
                    .collect()
            })
            .collect();
        tuples.sort();
        tuples.dedup();
        return tuples
            .iter()
            .map(|values| print_ground_fact(name, values))
            .collect();
    }

    // Otherwise: synthesized `answer/N` facts over the named variables. With no
    // named variables there is nothing fact-shaped to emit.
    if named_slots.is_empty() {
        return Vec::new();
    }
    rows.iter()
        .map(|row| print_ground_fact("answer", row))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_atom_query_substitutes_bindings() {
        let src = "\
parent(\"alice\", \"bob\").
parent(\"bob\", \"carol\").
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
?- ancestor(\"alice\", Who).
";
        let result = run(src).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "ancestor(\"alice\", \"bob\").".to_string(),
                "ancestor(\"alice\", \"carol\").".to_string(),
            ]]
        );
    }

    #[test]
    fn multi_literal_query_emits_answer_facts() {
        let src = "\
age(\"alice\", 30).
age(\"bob\", 15).
?- age(N, A), A >= 18.
";
        let result = run(src).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["answer(\"alice\", 30).".to_string()]]
        );
    }

    #[test]
    fn ground_single_atom_query_prints_when_it_holds() {
        let holds = run("p(\"a\").\n?- p(\"a\").").expect("runs");
        assert_eq!(holds.answers, vec![vec!["p(\"a\").".to_string()]]);
        let absent = run("p(\"a\").\n?- p(\"b\").").expect("runs");
        assert_eq!(absent.answers, vec![Vec::<String>::new()]);
    }

    #[test]
    fn inline_arithmetic_evaluates_like_a_hand_hoisted_assignment() {
        // The feature invariant: an inline arithmetic argument evaluates the
        // same as writing the assignment out by hand.
        let inline = run("number(1).\nnumber(2).\nsucc(N, N + 1) :- number(N).\n?- succ(X, Y).")
            .expect("runs");
        let hoisted =
            run("number(1).\nnumber(2).\nsucc(N, M) :- number(N), M = N + 1.\n?- succ(X, Y).")
                .expect("runs");
        assert_eq!(inline.answers, hoisted.answers);
        assert_eq!(
            inline.answers,
            vec![vec!["succ(1, 2).".to_string(), "succ(2, 3).".to_string(),]]
        );
    }

    #[test]
    fn disjunction_is_equivalent_to_separate_rules() {
        let disjunctive = run("a(1).\nb(2).\np(X) :- a(X) ; b(X).\n?- p(X).").expect("runs");
        let separate = run("a(1).\nb(2).\np(X) :- a(X).\np(X) :- b(X).\n?- p(X).").expect("runs");
        assert_eq!(disjunctive.answers, separate.answers);
        assert_eq!(
            disjunctive.answers,
            vec![vec!["p(1).".to_string(), "p(2).".to_string(),]]
        );
    }

    #[test]
    fn constant_folding_in_a_fact() {
        let result = run("p(1 + 1).\n?- p(X).").expect("runs");
        assert_eq!(result.answers, vec![vec!["p(2).".to_string()]]);
    }

    #[test]
    fn errors_from_each_stage_propagate() {
        // Parse error.
        assert!(run("p(1)").is_err());
        // Lowering (safety) error.
        assert!(run("p(X) :- q(Y).").is_err());
        // Type error.
        assert!(run("age(\"a\", 30).\nage(\"b\", \"old\").").is_err());
    }

    #[test]
    fn output_is_valid_input_closure() {
        // Run once, feed the printed answers back as a program, query again.
        let first = run("parent(\"a\", \"b\").\nparent(\"b\", \"c\").\nanc(X, Y) :- parent(X, Y).\nanc(X, Y) :- parent(X, Z), anc(Z, Y).\n?- anc(X, Y).")
            .expect("runs");
        let piped = first.output();
        // The printed `anc(...)` facts parse and load again.
        let second = run(&format!("{piped}?- anc(\"a\", Who).")).expect("re-runs");
        assert_eq!(
            second.answers,
            vec![vec![
                "anc(\"a\", \"b\").".to_string(),
                "anc(\"a\", \"c\").".to_string()
            ]]
        );
    }
}
