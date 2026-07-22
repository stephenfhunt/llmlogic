//! System tests: run the compiled `datalog` binary over program files and
//! assert on stdout, stderr, and exit codes — the outermost layer of the test
//! pyramid (`testing.md`). Uses `CARGO_BIN_EXE_datalog` (Cargo sets it for
//! integration tests), so there are no new dependencies.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_datalog");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Runs the binary on a corpus file.
fn run_file(name: &str) -> Output {
    let output = Command::new(BIN)
        .arg(format!("tests/programs/{name}"))
        .output()
        .expect("binary runs");
    Output {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code().expect("process exited normally"),
    }
}

/// Runs the binary reading its program from stdin (`-`).
fn run_stdin(program: &str) -> Output {
    let mut child = Command::new(BIN)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary spawns");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(program.as_bytes())
        .unwrap();
    let output = child.wait_with_output().expect("binary finishes");
    Output {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code().expect("process exited normally"),
    }
}

#[test]
fn ancestry_prints_facts_and_exits_zero() {
    let out = run_file("16_1_ancestry.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "ancestor(\"alice\", \"bob\").\n\
         ancestor(\"alice\", \"carol\").\n\
         ancestor(\"alice\", \"dave\").\n"
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn named_and_negation_programs_run() {
    assert_eq!(run_file("16_2_negation.dl").stdout, "root(\"alice\").\n");
    assert_eq!(run_file("16_7_named_args.dl").stdout, "adult(\"alice\").\n");
    assert_eq!(
        run_file("16_3_arithmetic.dl").stdout,
        "adult(\"alice\").\nadult(\"carol\").\n"
    );
}

#[test]
fn import_program_fails_with_a_structured_error() {
    let out = run_file("16_5_import.dl");
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.contains("imports not yet supported"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_malformed_file_reports_multiple_errors_and_exits_one() {
    let out = run_file("broken_multi.dl");
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.contains("relation names must be lowercase"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("comparisons do not chain"),
        "{}",
        out.stderr
    );
    // Multiple errors surface in one run (statement-level recovery).
    assert!(out.stderr.lines().count() >= 3, "{}", out.stderr);
}

#[test]
fn an_unsafe_program_reports_a_lowering_error() {
    let out = run_file("broken_unsafe.dl");
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("unsafe rule"), "{}", out.stderr);
}

#[test]
fn no_arguments_is_a_usage_error() {
    let output = Command::new(BIN).output().expect("binary runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr).unwrap().contains("usage:"));
}

#[test]
fn stdin_program_is_read_with_dash() {
    let out = run_stdin("p(\"a\"). p(\"b\").\n?- p(X).");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "p(\"a\").\np(\"b\").\n");
}

#[test]
fn a_query_free_program_prints_nothing_and_exits_zero() {
    let out = run_stdin("p(\"a\").\nq(X) :- p(X).");
    assert_eq!(out.code, 0);
    assert!(out.stdout.is_empty());
}

#[test]
fn binary_output_composes_over_a_pipe() {
    // The §14 jq-composition story, exercised for real: run 16.1, feed its
    // stdout back in with an appended query.
    let first = run_file("16_1_ancestry.dl");
    assert_eq!(first.code, 0);
    let composed = format!("{}?- ancestor(Who, \"carol\").", first.stdout);
    let second = run_stdin(&composed);
    assert_eq!(second.code, 0);
    // 16.1's query materialized only alice's ancestors, so alice is the match.
    assert_eq!(second.stdout, "ancestor(\"alice\", \"carol\").\n");
}
