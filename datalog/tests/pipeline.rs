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
fn named_arguments_16_7() {
    assert_eq!(
        answers(&corpus("16_7_named_args.dl")),
        vec!["adult(\"alice\")."]
    );
}

#[test]
fn imports_are_a_structured_eval_error_16_5() {
    let errors = datalog::run(&corpus("16_5_import.dl")).expect_err("imports do not evaluate yet");
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("imports not yet supported")),
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
    // arithmetic in a head arg, float + symbol value formatting, the `answer/N`
    // fallback for a multi-literal query, and two queries in one program.
    let result = datalog::run(&corpus("features.dl")).expect("runs");
    assert_eq!(
        result.answers,
        vec![
            // Query 1 (single atom): substituted, floats keep their decimal
            // point, symbols print bare.
            vec!["scaled(a, 3.0).".to_string(), "scaled(b, 6.0).".to_string()],
            // Query 2 (multi-literal): synthesized answer/N.
            vec!["answer(b, 3.0).".to_string()],
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
