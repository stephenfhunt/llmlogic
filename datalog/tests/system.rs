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

/// Runs the binary with an arbitrary argument vector (no stdin).
fn run_args(args: &[&str]) -> Output {
    let output = Command::new(BIN).args(args).output().expect("binary runs");
    Output {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code().expect("process exited normally"),
    }
}

/// Runs the binary on a corpus file with extra trailing arguments (e.g. `-q`).
fn run_file_args(name: &str, extra: &[&str]) -> Output {
    let mut args = vec![format!("tests/programs/{name}")];
    args.extend(extra.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_args(&refs)
}

/// Runs the binary reading its program from stdin (`-`), optionally with extra
/// trailing arguments.
fn run_stdin_args(program: &str, extra: &[&str]) -> Output {
    let mut child = Command::new(BIN)
        .arg("-")
        .args(extra)
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
    assert_eq!(
        run_file("16_4_aggregation.dl").stdout,
        "child_count(\"alice\", 2).\nchild_count(\"bob\", 1).\n"
    );
}

/// §13 module imports through the real binary: `modules_import.dl` splices
/// `modules/family.dl` relative to the program file's directory, while the
/// binary runs with the crate root as working directory — the
/// relative-resolution proof.
#[test]
fn module_import_program_runs() {
    let out = run_file("modules_import.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "ancestor(\"alice\", \"bob\").\nancestor(\"alice\", \"carol\").\n"
    );
    assert!(out.stderr.is_empty());
}

/// A query inside an imported module is refused, naming the module file.
#[test]
fn a_query_in_a_module_exits_one_naming_the_file() {
    let dir = std::env::temp_dir().join(format!("datalog-system-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("asks.dl"), "p(1).\n?- p(X).\n").expect("write");
    let root = dir.join("main.dl");
    std::fs::write(&root, "import \"asks.dl\".\n").expect("write");
    let out = run_args(&[root.to_str().unwrap()]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("asks.dl"), "{}", out.stderr);
    assert!(
        out.stderr.contains("not allowed in imported modules"),
        "{}",
        out.stderr
    );
}

#[test]
fn feature_program_prints_floats_and_symbols() {
    // Proves value formatting survives the real binary: floats keep a decimal
    // point, symbols print bare, answer/N and multiple queries concatenate.
    let out = run_file("features.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "scaled(a, 3.0).\nscaled(b, 6.0).\nanswer(b, 3.0).\n"
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn disjunction_program_runs() {
    let out = run_file("disjunction.dl");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "drinks(alice).\ndrinks(bob).\n");
}

#[test]
fn a_type_error_exits_one() {
    let out = run_file("broken_types.dl");
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("type error"), "{}", out.stderr);
}

#[test]
fn a_runtime_error_exits_one() {
    let out = run_file("broken_arith.dl");
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("division by zero"), "{}", out.stderr);
}

/// §16.5 through the real binary: the CSV import evaluates to ancestry facts,
/// resolved relative to the program file (the binary runs from the crate root).
#[cfg(feature = "duckdb")]
#[test]
fn import_program_evaluates() {
    let out = run_file("16_5_import.dl");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "ancestor(\"alice\", \"bob\").\n\
         ancestor(\"alice\", \"carol\").\n\
         ancestor(\"alice\", \"dave\").\n"
    );
    assert!(out.stderr.is_empty());
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
    // A lexical near-miss hint reaches stderr alongside the parse errors.
    assert!(out.stderr.contains("did you mean `<=`?"), "{}", out.stderr);
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

#[test]
fn an_undefined_predicate_warns_on_stderr_but_still_succeeds() {
    // `ancester` is a typo for `ancestor`: valid closed-world semantics (it is
    // just empty), so the run succeeds with exit 0 and prints the real answers
    // to stdout — but the likely mistake is flagged on stderr.
    let out = run_stdin(
        "parent(\"alice\", \"bob\").\n\
         ancestor(X, Y) :- parent(X, Y).\n\
         ancestor(X, Y) :- parent(X, Z), ancester(Z, Y).\n\
         ?- ancestor(\"alice\", Who).",
    );
    assert_eq!(out.code, 0);
    // stdout stays a clean fact stream, valid as pipe input.
    assert_eq!(out.stdout, "ancestor(\"alice\", \"bob\").\n");
    assert!(
        out.stderr
            .contains("predicate `ancester/2` is referenced but never defined"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("did you mean `ancestor`?"),
        "{}",
        out.stderr
    );
}

// --- `-q` one-shot queries (spec §14, step 6) ---

#[test]
fn dash_q_bare_atom_over_a_file() {
    // The file carries its own `?- ancestor("alice", Who).`; the `-q` block
    // follows it in program order.
    let out = run_file_args("16_1_ancestry.dl", &["-q", "ancestor(Who, \"dave\")"]);
    assert_eq!(out.code, 0);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert!(
        out.stdout.ends_with(
            "ancestor(\"alice\", \"dave\").\n\
             ancestor(\"bob\", \"dave\").\n\
             ancestor(\"carol\", \"dave\").\n"
        ),
        "{}",
        out.stdout
    );
}

#[test]
fn dash_q_define_and_select_rule() {
    // A rule query over stdin facts with no query of their own: the only output
    // is the synthesized head query's answers.
    let out = run_stdin_args(
        "parent(\"a\", \"b\").\nparent(\"b\", \"c\").\n",
        &["-q", "gp(X,Z) :- parent(X,Y), parent(Y,Z)"],
    );
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "gp(\"a\", \"c\").\n");
}

#[test]
fn dash_q_multiple_in_order() {
    let out = run_stdin_args(
        "p(1).\np(2).\n",
        &["-q", "p(X)", "-q", "big(X) :- p(X), X >= 2"],
    );
    assert_eq!(out.code, 0);
    // First block: p(X). Second block: the synthesized big(X) head query.
    assert_eq!(out.stdout, "p(1).\np(2).\nbig(2).\n");
}

#[test]
fn dash_q_only_with_empty_base() {
    let out = run_args(&["-q", "p(X)"]);
    assert_eq!(out.code, 0);
    // No facts, so the query has no answers.
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    // `p` is referenced by the query but nowhere defined — the warning explains
    // *why* the result is empty (an undefined relation, not a false query).
    assert!(
        out.stderr
            .contains("predicate `p/1` is referenced but never defined"),
        "{}",
        out.stderr
    );
}

#[test]
fn dash_q_over_stdin_base() {
    let out = run_stdin_args("p(\"a\"). p(\"b\").", &["-q", "p(X)"]);
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "p(\"a\").\np(\"b\").\n");
}

#[test]
fn lone_dash_q_is_a_usage_error() {
    let out = run_file_args("16_1_ancestry.dl", &["-q"]);
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("requires a query"), "{}", out.stderr);
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    let out = run_args(&["--json"]);
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("unknown flag"), "{}", out.stderr);
}

#[test]
fn a_malformed_dash_q_is_a_program_error() {
    let out = run_args(&["-q", "p("]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
}
