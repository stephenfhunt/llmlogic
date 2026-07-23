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

use std::path::Path;

use crate::ast::StatementKind;
use crate::engine::{Model, eval};
use crate::error::{Error, Warning};
use crate::ir;
use crate::lower::{check_program, lower_with_sources};
use crate::parser::parse;
use crate::print::{print_atom, print_ground_fact};
use crate::resolve::resolve_modules;
use crate::sources::load_imports;
use crate::typecheck::typecheck;

/// The result of a successful [`run`]: the least model plus each query's
/// canonical answer lines, in program (statement) order.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub model: Model,
    /// One entry per query, each a list of canonical fact lines (no trailing
    /// newline).
    pub answers: Vec<Vec<String>>,
    /// Non-fatal diagnostics (e.g. referenced-but-undefined predicates). The
    /// program still ran; these belong on stderr, never in the fact stream.
    pub warnings: Vec<Warning>,
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

/// Runs a program end to end with no file context: module and data imports
/// resolve against the working directory (§13). Equivalent to
/// [`run_at`]`(src, None)`.
pub fn run(src: &str) -> Result<RunResult, Vec<Error>> {
    run_at(src, None)
}

/// Runs a program end to end: `parse → resolve modules → lower → typecheck →
/// eval`, then answers every query. Returns every error found at the first
/// failing stage (each stage collects all of its own errors).
///
/// `source_path` is the program file itself when there is one; each file's
/// imports resolve relative to that file's directory (`None` — stdin or
/// `-q`-only programs — resolves against the working directory).
pub fn run_at(src: &str, source_path: Option<&Path>) -> Result<RunResult, Vec<Error>> {
    let ast = parse(src)?;
    let resolved = resolve_modules(ast, source_path)?;
    let tables = load_imports(&resolved.program)?;
    let program = lower_with_sources(&resolved.program, &tables)?;
    typecheck(&program)?;
    let warnings = check_program(&program);
    let model = eval(&program).map_err(|e| vec![e])?;

    let mut answers = Vec::with_capacity(program.queries.len());
    for query in &program.queries {
        let rows = model.answer(query).map_err(|e| vec![e])?;
        answers.push(answer_lines(query, &rows, &program));
    }
    Ok(RunResult {
        model,
        answers,
        warnings,
    })
}

/// Builds the combined program source for the agent CLI: the `base` program
/// followed by one appended query (or rule + synthesized query) per `-q`
/// argument, in order (`spec.md` §14, `-q` semantics).
///
/// Each `-q` argument is classified by **parsing** it (never by splitting on
/// `:-`, which a string literal may contain):
///
/// - A **define-and-select rule** — a single clause with a non-empty body, e.g.
///   `gp(X,Z) :- parent(X,Y), parent(Y,Z)` — appends the rule plus a synthesized
///   query over its head, `?- gp(X, Z).`.
/// - Anything else (a **bare atom** `ancestor("alice", X)`, a **comma-body**
///   `adult(N), N != "bob"`) is a query body and appends `?- <arg>.`.
///
/// A trailing `.` on the argument is optional. A malformed `-q` returns its own
/// parse errors, attributed to that argument rather than to the synthesized
/// combined source.
pub fn program_with_queries(base: &str, queries: &[String]) -> Result<String, Vec<Error>> {
    let mut source = base.to_string();
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
    }
    for query in queries {
        source.push_str(&query_source(query)?);
    }
    Ok(source)
}

/// Renders one `-q` argument to the program text it contributes (see
/// [`program_with_queries`]). The returned string ends with a newline.
fn query_source(arg: &str) -> Result<String, Vec<Error>> {
    // Normalize: trim surrounding whitespace and any single trailing `.`.
    let core = arg.trim();
    let core = core.strip_suffix('.').unwrap_or(core).trim_end();

    // A define-and-select rule parses as exactly one clause with a non-empty
    // body. A bare atom parses as a clause with an *empty* body (a fact), which
    // we treat as a query body, not a rule.
    if let Ok(program) = parse(&format!("{core}."))
        && let [statement] = &program.statements[..]
        && let StatementKind::Clause(clause) = &statement.kind
        && !clause.body.is_empty()
    {
        let head = print_atom(&clause.head);
        return Ok(format!("{core}.\n?- {head}.\n"));
    }

    // Otherwise it is a query body. Validate it as one so a genuinely malformed
    // `-q` surfaces its own parse error here.
    let query = format!("?- {core}.");
    parse(&query)?;
    Ok(format!("{query}\n"))
}

/// Runs a `base` program plus one-shot `-q` queries with no file context.
/// Equivalent to [`run_with_queries_at`]`(base, None, queries)`.
pub fn run_with_queries(base: &str, queries: &[String]) -> Result<RunResult, Vec<Error>> {
    run_with_queries_at(base, None, queries)
}

/// Runs a `base` program plus one-shot `-q` queries end to end: builds the
/// combined source with [`program_with_queries`], then [`run_at`]s it. This is
/// the agent CLI's entry point (`datalog [<file>|-] [-q …]`); `source_path` is
/// the base program's file, threading §13 relative-path resolution (the
/// appended `-q` text has no paths of its own).
pub fn run_with_queries_at(
    base: &str,
    source_path: Option<&Path>,
    queries: &[String],
) -> Result<RunResult, Vec<Error>> {
    let source = program_with_queries(base, queries)?;
    run_at(&source, source_path)
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

    // --- `-q` one-shot queries (spec §14, step 6) ---

    fn build(base: &str, queries: &[&str]) -> String {
        let queries: Vec<String> = queries.iter().map(|q| q.to_string()).collect();
        program_with_queries(base, &queries).expect("builds")
    }

    #[test]
    fn bare_atom_query_becomes_a_query_line() {
        assert_eq!(
            build("", &["ancestor(\"alice\", X)"]),
            "?- ancestor(\"alice\", X).\n"
        );
    }

    #[test]
    fn comma_body_query_becomes_one_query_line() {
        // A conjunction is a query body, not a valid statement, so it must fall
        // through to the `?- …` branch rather than being misclassified.
        assert_eq!(
            build("", &["adult(N), N != \"bob\""]),
            "?- adult(N), N != \"bob\".\n"
        );
    }

    #[test]
    fn define_and_select_rule_appends_rule_then_head_query() {
        // The rule text is kept verbatim; only the synthesized head query is
        // canonically printed (note the space after the comma).
        assert_eq!(
            build("", &["gp(X,Z) :- parent(X,Y), parent(Y,Z)"]),
            "gp(X,Z) :- parent(X,Y), parent(Y,Z).\n?- gp(X, Z).\n"
        );
    }

    #[test]
    fn trailing_period_is_normalized() {
        assert_eq!(build("", &["p(X)."]), build("", &["p(X)"]));
    }

    #[test]
    fn base_program_is_preserved_and_newline_separated() {
        assert_eq!(build("p(1).", &["p(X)"]), "p(1).\n?- p(X).\n");
    }

    #[test]
    fn multiple_queries_apply_in_order() {
        let source = build(
            "",
            &["gp(X,Z) :- parent(X,Y), parent(Y,Z)", "gp(X, \"dave\")"],
        );
        assert_eq!(
            source,
            "gp(X,Z) :- parent(X,Y), parent(Y,Z).\n?- gp(X, Z).\n?- gp(X, \"dave\").\n"
        );
    }

    #[test]
    fn a_colon_dash_inside_a_string_is_not_a_rule() {
        // Classification parses the arg; it must not string-split on `:-`.
        assert_eq!(
            build("", &["note(\"build :- run\")"]),
            "?- note(\"build :- run\").\n"
        );
    }

    #[test]
    fn a_malformed_query_is_an_error() {
        let queries = vec!["p(".to_string()];
        assert!(program_with_queries("", &queries).is_err());
    }

    #[test]
    fn run_with_queries_answers_over_an_in_memory_base() {
        let base = "\
parent(\"alice\", \"bob\").
parent(\"bob\", \"carol\").
anc(X, Y) :- parent(X, Y).
anc(X, Y) :- parent(X, Z), anc(Z, Y).
";
        let result = run_with_queries(base, &["anc(\"alice\", Who)".to_string()]).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "anc(\"alice\", \"bob\").".to_string(),
                "anc(\"alice\", \"carol\").".to_string(),
            ]]
        );
    }

    #[test]
    fn run_with_queries_evaluates_a_define_and_select_rule() {
        let base = "parent(\"a\", \"b\").\nparent(\"b\", \"c\").\n";
        let result = run_with_queries(base, &["gp(X,Z) :- parent(X,Y), parent(Y,Z)".to_string()])
            .expect("runs");
        assert_eq!(result.answers, vec![vec!["gp(\"a\", \"c\").".to_string()]]);
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
