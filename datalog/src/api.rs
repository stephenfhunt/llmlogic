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

use std::collections::{BTreeMap, HashMap};
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
    let mut warnings = check_program(&program);
    let model = eval(&program).map_err(|e| vec![e])?;
    warnings.extend(absent_skip_warnings(&model, &program));

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

    // A define-and-select rule parses as one *rule* with a non-empty body —
    // however many clauses that rule desugars to, since the parser expands a
    // top-level `;` into one clause per disjunct sharing the head (§17,
    // 2026-07-22). A bare atom parses as a clause with an *empty* body (a fact),
    // which we treat as a query body, not a rule.
    if let Ok(program) = parse(&format!("{core}."))
        && let Some((first, rest)) = program.statements.split_first()
        && let StatementKind::Clause(first_clause) = &first.kind
        && !first_clause.body.is_empty()
    {
        let head = print_atom(&first_clause.head);
        // Every clause must share that head, or this is two unrelated rules
        // rather than one disjunctive one — which belongs on the query-body path
        // below, where it becomes a parse error attributed to the argument,
        // instead of here where it would silently select only the first head.
        // Compared as printed text: the canonical printer is span-free, and the
        // shared head means the disjuncts print identically.
        let one_rule = rest.iter().all(|statement| match &statement.kind {
            StatementKind::Clause(clause) => {
                !clause.body.is_empty() && print_atom(&clause.head) == head
            }
            _ => false,
        });
        if one_rule {
            return Ok(format!("{core}.\n?- {head}.\n"));
        }
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

/// Reports every aggregate that skipped an `absent` input (§9's skip-but-report
/// rule), read back out of the recorded provenance rather than tracked
/// separately — [`crate::provenance::Premise::Aggregate`] already carries the
/// count, and this is what makes it visible before the `?why` surface (§11)
/// exists.
///
/// One warning per aggregate *site* — a `(rule, body index)` pair — summed over
/// every group that site produced. Sites are keyed in `BTreeMap` order so the
/// warning stream is deterministic like everything else in §14.
///
/// Limitation, deliberate for the interim: `Model::answer` records no
/// derivations, so an aggregate that appears only in a *query* is not covered.
fn absent_skip_warnings(model: &Model, program: &ir::Program) -> Vec<Warning> {
    // Only programs with an aggregate can skip; skipping the derivation scan
    // keeps this free for everything else.
    if !program.rules.iter().any(|rule| {
        rule.body
            .iter()
            .any(|literal| matches!(literal.kind, ir::BodyLiteralKind::Aggregate { .. }))
    }) {
        return Vec::new();
    }

    // (rule, body index) → (op, total skipped, groups that skipped).
    let mut sites: BTreeMap<(u32, usize), (&'static str, usize, usize)> = BTreeMap::new();
    for fact in model.facts() {
        for derivation in model.derivations_of(&fact) {
            for (idx, premise) in derivation.premises.iter().enumerate() {
                if let crate::provenance::Premise::Aggregate { op, skipped, .. } = premise
                    && *skipped > 0
                {
                    let entry =
                        sites
                            .entry((derivation.rule.0, idx))
                            .or_insert((op.keyword(), 0, 0));
                    entry.1 += skipped;
                    entry.2 += 1;
                }
            }
        }
    }

    sites
        .into_iter()
        .map(|((rule_id, _), (op, skipped, groups))| {
            let rule = &program.rules[rule_id as usize];
            let info = program.pred_info(rule.head.pred);
            Warning::AbsentSkippedInAggregate {
                op,
                rule: format!("{}/{}", info.name, info.arity),
                skipped,
                groups,
            }
        })
        .collect()
}

/// Renders one query's answer rows to canonical fact lines per the §14 output
/// shape. `rows` are the projected answer-variable bindings (in
/// [`ir::Query::projection`] order), as returned by [`Model::answer`].
fn answer_lines(query: &ir::Query, rows: &[Vec<ir::Value>], program: &ir::Program) -> Vec<String> {
    // Row position of each projected slot — this is the order `Model::answer`
    // lays out each row.
    let position_of: HashMap<u32, usize> = query
        .projection
        .iter()
        .enumerate()
        .map(|(position, &slot)| (slot, position))
        .collect();

    // Substituted-atom form: a single positive atom whose every variable is a
    // projected one.
    if let [literal] = &query.body[..]
        && let ir::BodyLiteralKind::Atom(atom) = &literal.kind
        && atom.args.iter().all(|term| match term {
            ir::Term::Const(_) => true,
            ir::Term::Var(var) => position_of.contains_key(&var.0),
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
                        ir::Term::Var(var) => row[position_of[&var.0]].clone(),
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

    // Otherwise: synthesized `answer/N` facts over the answer variables. With
    // none there is nothing fact-shaped to emit.
    if query.projection.is_empty() {
        return Vec::new();
    }
    rows.iter()
        .map(|row| print_ground_fact("answer", row))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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

    /// `bugs/001`: a computed argument under `not` used to be read as a
    /// wildcard, so the rule silently degraded to `not q(_)` — "no `q` fact at
    /// all" — and one unrelated `q(3)` suppressed every row.
    ///
    /// For `X = 1`, `not q(2)` holds; for `X = 2`, `not q(3)` fails; for
    /// `X = 3`, `not q(4)` holds.
    #[test]
    fn a_computed_argument_under_negation_is_matched_not_wildcarded() {
        let facts = "p(1).\np(2).\np(3).\nq(3).\n";
        let expected = vec![vec!["r(1).".to_string(), "r(3).".to_string()]];
        let inline = run(&format!("{facts}r(X) :- p(X), not q(X + 1).\n?- r(X).")).expect("runs");
        assert_eq!(inline.answers, expected);
    }

    /// The §5 claim that inline arithmetic and a hand-written assignment lower
    /// to the same IR, extended to negated atoms — where it used to fail three
    /// different ways at once (`bugs/001`): the inline form returned the wrong
    /// rows, and both hand-hoisted spellings were rejected outright.
    #[test]
    fn negated_inline_arithmetic_agrees_with_both_hoisted_spellings() {
        let facts = "p(1).\np(2).\np(3).\nq(3).\n";
        let inline =
            run(&format!("{facts}r(X) :- p(X), not q(X + 1).\n?- r(X).")).expect("inline runs");
        let binder_first = run(&format!(
            "{facts}r(X) :- p(X), Y = X + 1, not q(Y).\n?- r(X)."
        ))
        .expect("binder-first runs");
        let binder_last = run(&format!(
            "{facts}r(X) :- p(X), not q(Y), Y = X + 1.\n?- r(X)."
        ))
        .expect("binder-last runs");
        assert_eq!(inline.answers, binder_first.answers);
        assert_eq!(inline.answers, binder_last.answers);
    }

    /// The same defect reached through an aggregate result rather than
    /// arithmetic — also a lowering-generated slot, also hoisted (`bugs/001`).
    /// `count { Y | p(Y) }` is 2, so `not q(2)` holds for every `X`.
    #[test]
    fn an_aggregate_argument_under_negation_is_matched_not_wildcarded() {
        let program = "p(1).\np(2).\nq(3).\nr(X) :- p(X), not q(count { Y | p(Y) }).\n?- r(X).";
        let result = run(program).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["r(1).".to_string(), "r(2).".to_string()]]
        );
    }

    /// The widening does not reach genuine wildcards: `_` under a negation is
    /// bound nowhere, so it stays open and existential (§7).
    #[test]
    fn a_wildcard_under_negation_stays_existential() {
        let result =
            run("p(1).\np(2).\nq(2, 9).\nr(X) :- p(X), not q(X, _).\n?- r(X).").expect("runs");
        assert_eq!(result.answers, vec![vec!["r(1).".to_string()]]);
    }

    /// A *named* variable no literal binds is still unsafe — the one thing the
    /// scheduler cannot decide, since to it an unbound slot is a wildcard.
    #[test]
    fn a_named_variable_bound_nowhere_is_still_unsafe_under_negation() {
        let errors = run("p(1).\nr(X) :- p(X), not q(Y).\n?- r(X).").expect_err("Y is unsafe");
        assert!(
            errors.iter().any(|e| {
                let msg = e.to_string();
                msg.contains("negated atom") && msg.contains("`Y`")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    // --- Absent value, end-to-end through the pipeline (§4/§8) ---

    /// A `measurement` fixture with one present and one missing amount. The
    /// missing cell is the `absent` literal in a fact — the same value an empty
    /// import cell will produce (§13).
    const SPARSE: &str = "m(\"a\", 5).\nm(\"b\", absent).\n";

    #[test]
    fn presence_test_partitions_rows() {
        // `is not absent` and `is absent` each select exactly one row; together
        // they cover both (unlike `=`/`!=`, which are both false on absent).
        let present = run(&format!(
            "{SPARSE}has(F) :- m(F, A), A is not absent.\n?- has(F)."
        ))
        .expect("runs");
        assert_eq!(present.answers, vec![vec!["has(\"a\").".to_string()]]);

        let missing = run(&format!(
            "{SPARSE}mis(F) :- m(F, A), A is absent.\n?- mis(F)."
        ))
        .expect("runs");
        assert_eq!(missing.answers, vec![vec!["mis(\"b\").".to_string()]]);
    }

    #[test]
    fn threshold_silently_excludes_absent() {
        // A comparison with an absent operand is false, so the missing row drops
        // out — no type error despite the column being int.
        let result =
            run(&format!("{SPARSE}high(F) :- m(F, A), A >= 5.\n?- high(F).")).expect("runs");
        assert_eq!(result.answers, vec![vec!["high(\"a\").".to_string()]]);
    }

    #[test]
    fn both_eq_and_ne_are_false_on_absent() {
        // A comparison against a *value* is silently false when the operand is
        // absent — the two-valued rule (§8), and the reason presence has its own
        // operator. `m("b", absent)` is selected by neither `A = 9` nor `A != 9`,
        // even though one of them holds for every ordinary value.
        for (op, expected) in [("=", Vec::new()), ("!=", vec!["q(\"a\").".to_string()])] {
            let result = run(&format!("{SPARSE}q(F) :- m(F, A), A {op} 9.\n?- q(F)."))
                .unwrap_or_else(|e| panic!("runs: {e:?}"));
            assert_eq!(
                result.answers,
                vec![expected],
                "`A {op} 9` never selects the absent row"
            );
        }
    }

    #[test]
    fn comparing_against_the_literal_absent_is_a_steered_error() {
        // Writing the literal is the mistake the presence operator exists to
        // prevent: `A = absent` and `A != absent` are *both* always false, so
        // rather than silently answering nothing, lowering steers to `is absent`.
        for op in ["=", "!=", "<", ">="] {
            let errors = run(&format!(
                "{SPARSE}q(F) :- m(F, A), A {op} absent.\n?- q(F)."
            ))
            .expect_err("comparing against the literal `absent` is rejected");
            let message = errors[0].to_string();
            assert!(
                message.contains("is always false") && message.contains("X is not absent"),
                "expected a presence-test steer for `{op}`, got: {message}"
            );
        }
        // The producer form is untouched: `X = absent` binding an unbound `X`.
        let produced = run(&format!(
            "{SPARSE}z(F, X) :- m(F, _), X = absent.\n?- z(F, X)."
        ))
        .expect("the producer form still lowers");
        assert_eq!(
            produced.answers,
            vec![vec![
                "z(\"a\", absent).".to_string(),
                "z(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn absent_flows_to_the_head_and_round_trips() {
        // Naming a column returns every row; the missing one prints as the
        // reserved literal `absent` (Datalog-out is Datalog-in).
        let result = run(&format!("{SPARSE}rec(F, A) :- m(F, A).\n?- rec(F, A).")).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "rec(\"a\", 5).".to_string(),
                "rec(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn arithmetic_annihilates_through_a_computed_column() {
        let result = run(&format!(
            "{SPARSE}plus(F, A) :- m(F, X), A = X + 10.\n?- plus(F, A)."
        ))
        .expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "plus(\"a\", 15).".to_string(),
                "plus(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn absent_keys_do_not_join() {
        // Missing foreign keys must not cartesian-blow-up: absent unifies with
        // nothing, including another absent.
        let src = "a(\"x\", absent).\nb(\"y\", absent).\nj(P, Q) :- a(P, K), b(Q, K).\n?- j(P, Q).";
        let result = run(src).expect("runs");
        assert_eq!(result.answers, vec![Vec::<String>::new()]);
    }

    #[test]
    fn a_literal_absent_in_a_body_atom_is_an_error_steering_to_is_absent() {
        let errors =
            run(&format!("{SPARSE}q(F) :- m(F, absent).\n?- q(F).")).expect_err("rejected");
        let joined = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("is absent"), "got: {joined}");
        assert!(joined.contains("cannot be matched"), "got: {joined}");
    }

    #[test]
    fn a_literal_absent_in_a_head_or_fact_is_allowed() {
        // Production sites accept the literal: the fact above, plus a rule head.
        let result = run(&format!(
            "{SPARSE}tag(F, absent) :- m(F, _).\n?- tag(F, A)."
        ))
        .expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "tag(\"a\", absent).".to_string(),
                "tag(\"b\", absent).".to_string(),
            ]]
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

    // --- Surface-spelling equivalence (testing.md C8) ---
    //
    // Two ways of writing one program must answer identically. Both claims
    // below were carried by the unit tests above them until this session; the
    // record for such claims is that the ones with properties held and the ones
    // without became `bugs/001` and `bugs/002`.

    proptest! {
        /// **C8** — `;` disjunction means the same as writing the disjuncts as
        /// separate rules (§5; the parser expands one clause per disjunct,
        /// §17 2026-07-22). Generalizes
        /// `disjunction_is_equivalent_to_separate_rules` above.
        #[test]
        fn disjunction_equals_separate_rules(
            (disjunctive, separate) in crate::testgen::arb_disjunction_spellings()
        ) {
            let a = run(&disjunctive).map(|r| r.answers);
            let b = run(&separate).map(|r| r.answers);
            match (a, b) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "the two spellings disagreed on acceptance: {:?} vs {:?}\n\
                     --- disjunctive ---\n{}\n--- separate ---\n{}",
                    a.is_ok(), b.is_ok(), &disjunctive, &separate
                ),
            }
        }
    }

    proptest! {
        /// **C8** — a `-q` rule answers exactly as the same rule written into
        /// the file would (§14: `-q` "is sugar for appending `?- …` to the
        /// loaded program").
        ///
        /// This was `bugs/002`'s executable acceptance criterion, `#[ignore]`d
        /// and failing until the classifier stopped matching a *single*
        /// statement (resolved 2026-07-26).
        #[test]
        fn dash_q_rule_equals_the_same_rule_in_a_file(
            (base, rule, head) in crate::testgen::arb_dash_q_rule()
        ) {
            let from_file = run(&format!("{base}{rule}.\n?- {head}.\n")).map(|r| r.answers);
            let from_q = run_with_queries(&base, std::slice::from_ref(&rule)).map(|r| r.answers);
            match (from_file, from_q) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "`-q` and the file form disagreed on acceptance ({:?} vs {:?}) \
                     for rule `{}`", a.is_ok(), b.is_ok(), &rule
                ),
            }
        }
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
    fn a_disjunctive_rule_is_one_rule() {
        // `bugs/002`: the parser expands a top-level `;` into one clause per
        // disjunct sharing the head, so this arrives as *two* statements and
        // used to fall through to the query path — reporting a syntax error
        // about a grammar the user did not write.
        assert_eq!(
            build("", &["r(X) :- p(X), X < 5 ; p(X), s(X)"]),
            "r(X) :- p(X), X < 5 ; p(X), s(X).\n?- r(X).\n"
        );
        let result = run_with_queries(
            "p(1). p(9). s(9).",
            &["r(X) :- p(X), X < 5 ; p(X), s(X)".to_string()],
        )
        .expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["r(1).".to_string(), "r(9).".to_string()]]
        );
    }

    #[test]
    fn two_unrelated_rules_are_not_a_define_and_select() {
        // Outside the documented `-q` contract: heads differ, so this must not
        // silently select only `a`. It lands on the query-body path, where it is
        // a parse error attributed to the argument.
        let queries = ["a(X) :- p(X). b(X) :- p(X)".to_string()];
        assert!(program_with_queries("p(1).", &queries).is_err());
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
