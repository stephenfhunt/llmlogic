//! Integration tests: source text → query answers through the full public
//! pipeline (`datalog::run`), over the spec §16 corpus. These exercise the
//! library surface from outside the crate; the system tests
//! (`tests/system.rs`) run the compiled binary over the same programs.

use std::fs;

fn corpus(name: &str) -> String {
    fs::read_to_string(format!("tests/programs/{name}")).expect("corpus file exists")
}

fn answers(src: &str) -> Vec<String> {
    datalog::run(src)
        .unwrap_or_else(|e| panic!("run failed: {e:?}"))
        .answers
        .into_iter()
        .flatten()
        .collect()
}

#[test]
fn ancestry_16_1() {
    assert_eq!(
        answers(&corpus("16_1_ancestry.dl")),
        vec![
            "ancestor(\"alice\", \"bob\").",
            "ancestor(\"alice\", \"carol\").",
            "ancestor(\"alice\", \"dave\").",
        ]
    );
}

#[test]
fn negation_16_2() {
    assert_eq!(
        answers(&corpus("16_2_negation.dl")),
        vec!["root(\"alice\")."]
    );
}

#[test]
fn arithmetic_16_3() {
    assert_eq!(
        answers(&corpus("16_3_arithmetic.dl")),
        vec!["adult(\"alice\").", "adult(\"carol\")."]
    );
}

#[test]
fn aggregation_16_4() {
    assert_eq!(
        answers(&corpus("16_4_aggregation.dl")),
        vec!["child_count(\"alice\", 2).", "child_count(\"bob\", 1)."]
    );
}

#[test]
fn aggregation_skips_absent_and_typechecks() {
    // sum/avg skip absent inputs; min extends to strings; the whole program
    // (including typecheck) accepts the aggregates (§9).
    let src = "\
        measure(\"iron\", 5). measure(\"iron\", 3). measure(\"iron\", absent).\n\
        total(N, S) :- measure(N, _), S = sum { A | measure(N, A) }.\n\
        mean(N, M)  :- measure(N, _), M = avg { A | measure(N, A) }.\n\
        name(\"bob\"). name(\"alice\").\n\
        first(F) :- F = min { X | name(X) }.\n\
        ?- total(N, S).\n\
        ?- mean(N, M).\n\
        ?- first(F).";
    assert_eq!(
        answers(src),
        vec![
            "total(\"iron\", 8).",
            "mean(\"iron\", 4.0).",
            "first(\"alice\").",
        ]
    );
}

#[test]
fn count_counts_bindings_present_written_explicitly() {
    // count counts bindings (absent included); the present count is written
    // out with an explicit `is not absent` filter (§9).
    let src = "\
        m(\"a\", 1). m(\"a\", absent). m(\"a\", 2).\n\
        thing(\"a\").\n\
        bindings(T, C) :- thing(T), C = count { A | m(T, A) }.\n\
        present(T, C)  :- thing(T), C = count { A | m(T, A), A is not absent }.\n\
        ?- bindings(T, C).\n\
        ?- present(T, C).";
    assert_eq!(
        answers(src),
        vec!["bindings(\"a\", 3).", "present(\"a\", 2)."]
    );
}

/// An aggregate in a **query** answers over the variables the query body binds
/// — not over every named slot. An aggregate's goal-local variables are named
/// but exist only inside its sub-join (§9), so `?- N = count { C | m(T, C) }.`
/// answers over `N` alone. (Projecting them used to leave an unbound slot in the
/// answer row.)
#[test]
fn an_aggregate_in_a_query_projects_only_body_bound_variables() {
    let facts = "m(\"a\", 1). m(\"a\", 2). m(\"b\", 9).\nthing(\"a\"). thing(\"b\").\n";

    // Goal-local `T` and `C`: one global count, projected over `N` alone.
    assert_eq!(
        answers(&format!("{facts}?- N = count {{ C | m(T, C) }}.")),
        vec!["answer(3)."]
    );
    // `T` bound outside the aggregate is a group key and *is* projected.
    assert_eq!(
        answers(&format!("{facts}?- thing(T), N = count {{ C | m(T, C) }}.")),
        vec!["answer(\"a\", 2).", "answer(\"b\", 1)."]
    );
    // Composing under §8: the aggregate inside arithmetic, and as a filter.
    assert_eq!(
        answers(&format!("{facts}?- N = count {{ C | m(T, C) }} + 1.")),
        vec!["answer(4)."]
    );
    // As a *filter* the aggregate binds only a fresh unnamed slot, so the atom
    // still accounts for every answer variable and the substituted form holds
    // (§17, 2026-08-17) — unlike the group-key case above, where `N` is named
    // and no atom carries it.
    assert_eq!(
        answers(&format!("{facts}?- thing(T), count {{ C | m(T, C) }} > 1.")),
        vec!["thing(\"a\")."]
    );
}

/// An aggregate's goal is a body and binds like one: a variable bound inside it
/// by an `=`-assignment or by a *nested* aggregate is available to the collected
/// expression. Lowering hoists a nested aggregate into the goal for exactly this
/// reason, so the safety check must use the same notion of "bound".
#[test]
fn an_aggregate_goal_binds_assignments_and_nested_aggregates() {
    let facts = "s(\"a\", 1). s(\"a\", 2). s(\"b\", 10).\n";

    // `T` bound by an `=`-assignment inside the goal.
    assert_eq!(
        answers(&format!(
            "{facts}doubled(K, M) :- s(K, _), M = max {{ T | s(K, V), T = V * 2 }}.\n\
             ?- doubled(K, M)."
        )),
        vec!["doubled(\"a\", 4).", "doubled(\"b\", 20)."]
    );
    // `T` bound by a nested aggregate: the largest per-key sum (a=3, b=10).
    assert_eq!(
        answers(&format!(
            "{facts}?- M = max {{ T | s(K, _), T = sum {{ V | s(K, V) }} }}."
        )),
        vec!["answer(10)."]
    );
}

/// §9's skip-but-report rule: `sum`/`avg`/`min`/`max` drop `absent` inputs, and
/// the drop is *reported* — read back out of the recorded provenance, one
/// warning per aggregate site. Until `?why` (§11) lands this is the only signal
/// that an answer covers less data than it appears to.
#[test]
fn skipped_absents_are_reported_as_warnings() {
    let result = datalog::run(
        "m(\"a\", 5). m(\"a\", absent). m(\"b\", 7).\n\
         thing(\"a\"). thing(\"b\").\n\
         total(T, S) :- thing(T), S = sum { A | m(T, A) }.\n\
         seen(T, N)  :- thing(T), N = count { A | m(T, A) }.\n\
         ?- total(T, S).",
    )
    .expect("runs");
    let warnings: Vec<String> = result.warnings.iter().map(|w| w.to_string()).collect();
    assert_eq!(warnings.len(), 1, "one site skipped: {warnings:?}");
    assert!(
        warnings[0].contains("`sum` in `total/2`")
            && warnings[0].contains("skipped 1 absent input(s)"),
        "unexpected warning: {}",
        warnings[0]
    );

    // No absent, no warning — and `count` never skips, so it never warns.
    let clean = datalog::run(
        "m(\"a\", 5).\nthing(\"a\").\n\
         total(T, S) :- thing(T), S = sum { A | m(T, A) }.\n?- total(T, S).",
    )
    .expect("runs");
    assert!(clean.warnings.is_empty(), "{:?}", clean.warnings);
}

#[test]
fn named_arguments_16_7() {
    assert_eq!(
        answers(&corpus("16_7_named_args.dl")),
        vec!["adult(\"alice\")."]
    );
}

/// §13 module imports: the corpus program splices `modules/family.dl` —
/// resolved relative to the program file, while this test runs with the crate
/// root as working directory.
#[test]
fn module_import_splices_a_library() {
    let path = std::path::Path::new("tests/programs/modules_import.dl");
    let source = fs::read_to_string(path).expect("corpus file exists");
    let result = datalog::run_at(&source, Some(path)).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(
        answers,
        vec![
            "ancestor(\"alice\", \"bob\").",
            "ancestor(\"alice\", \"carol\").",
        ]
    );
}

/// A pathless program (stdin/`-q` shape) resolves module imports against the
/// working directory — for tests, the crate root.
#[test]
fn pathless_module_import_resolves_against_the_working_directory() {
    let src = "import \"tests/programs/modules/family.dl\".\n?- parent(X, Y).\n";
    let result = datalog::run(src).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(answers.len(), 2);
}

/// §16.5 — a CSV import evaluates: its tuples are base facts feeding the
/// recursive ancestry rules. Resolved relative to the program file, so the
/// `data/parents.csv` path works from any working directory.
#[cfg(feature = "duckdb")]
#[test]
fn csv_import_evaluates_16_5() {
    let path = std::path::Path::new("tests/programs/16_5_import.dl");
    let source = fs::read_to_string(path).expect("corpus file exists");
    let result = datalog::run_at(&source, Some(path)).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(
        answers,
        vec![
            "ancestor(\"alice\", \"bob\").",
            "ancestor(\"alice\", \"carol\").",
            "ancestor(\"alice\", \"dave\").",
        ]
    );
}

/// Under `--no-default-features` there is no reader, so a data import is a
/// structured error naming the feature (not a silent empty result).
#[cfg(not(feature = "duckdb"))]
#[test]
fn csv_import_without_the_reader_feature_is_a_structured_error() {
    let path = std::path::Path::new("tests/programs/16_5_import.dl");
    let source = fs::read_to_string(path).expect("corpus file exists");
    let errors = datalog::run_at(&source, Some(path)).expect_err("no reader");
    assert!(
        errors.iter().any(|e| e.to_string().contains("duckdb")),
        "got {errors:?}"
    );
}

#[test]
fn a_malformed_program_reports_multiple_errors_in_one_run() {
    let errors = datalog::run(&corpus("broken_multi.dl")).expect_err("malformed");
    assert!(errors.len() >= 3, "expected several errors, got {errors:?}");
}

#[test]
fn features_program_output_shapes() {
    // One program exercising several end-to-end gaps at once: inline
    // arithmetic in a head arg, float + symbol value formatting, a filtered
    // query, and two queries in one program.
    let result = datalog::run(&corpus("features.dl")).expect("runs");
    assert_eq!(
        result.answers,
        vec![
            // Query 1 (single atom): substituted, floats keep their decimal
            // point, symbols print bare.
            vec!["scaled(a, 3.0).".to_string(), "scaled(b, 6.0).".to_string()],
            // Query 2 (atom + a non-binding filter): also substituted, under
            // the source relation's name (§17, 2026-08-17).
            vec!["measure(b, 3.0).".to_string()],
        ]
    );
}

#[test]
fn disjunction_end_to_end() {
    assert_eq!(
        answers(&corpus("disjunction.dl")),
        vec!["drinks(alice).", "drinks(bob)."]
    );
}

#[test]
fn a_type_error_is_reported() {
    let errors = datalog::run(&corpus("broken_types.dl")).expect_err("heterogeneous column");
    assert!(
        errors.iter().any(|e| e.to_string().contains("type error")),
        "got {errors:?}"
    );
}

#[test]
fn ordered_comparison_uses_every_types_natural_order() {
    // §8: ordered comparisons use the operand type's natural order — strings and
    // symbols lexicographically, `false < true` — the same order `min`/`max`
    // fold with. `bugs/006`: the typechecker required int or float, so `<` was
    // usable on two of the five primitives while `min` ordered all five.
    assert_eq!(
        answers("s(\"a\"). s(\"b\").\n?- s(X), s(Y), X < Y."),
        vec!["answer(\"a\", \"b\")."]
    );
    assert_eq!(
        answers("y(alpha). y(beta).\n?- y(X), y(Y), X < Y."),
        vec!["answer(alpha, beta)."]
    );
    assert_eq!(
        answers("b(true). b(false).\n?- b(X), b(Y), X < Y."),
        vec!["answer(false, true)."]
    );
    // The canonicalisation idiom the defect made unwritable over string keys:
    // an unordered pair counted once (`bugs/006`, "What it cost").
    assert_eq!(
        answers(
            "e(\"x\", \"y\"). e(\"y\", \"x\"). e(\"x\", \"z\").\n\
             pair(A, B) :- e(A, B), A < B.\n?- pair(A, B)."
        ),
        vec!["pair(\"x\", \"y\").", "pair(\"x\", \"z\")."]
    );
}

#[test]
fn a_cross_type_ordered_comparison_is_still_a_type_error() {
    // Widening `<` past int/float left the same-type rule alone: `union(l, r)`
    // is what rejects this, and it is independent of the numeric constraint
    // that `bugs/006` removed.
    let errors =
        datalog::run("s(\"a\").\n?- s(X), X < 1.").expect_err("string compared against int");
    assert!(
        errors.iter().any(|e| e.to_string().contains("type error")),
        "got {errors:?}"
    );
}

#[test]
fn a_runtime_arithmetic_error_is_reported() {
    let errors = datalog::run(&corpus("broken_arith.dl")).expect_err("division by zero");
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("division by zero")),
        "got {errors:?}"
    );
}

#[test]
fn output_is_valid_input_end_to_end() {
    // Materialize 16.1's query result (alice's ancestors), then query those
    // materialized facts again — the printed facts parse and load as input.
    let first = datalog::run(&corpus("16_1_ancestry.dl")).expect("runs");
    let materialized = first.output();
    let composed = format!("{materialized}?- ancestor(Who, \"dave\").");
    let second = datalog::run(&composed).expect("re-runs over its own output");
    // Only alice's ancestors were materialized, so alice is the only match.
    assert_eq!(
        second.answers.into_iter().flatten().collect::<Vec<_>>(),
        vec!["ancestor(\"alice\", \"dave\")."]
    );
}
