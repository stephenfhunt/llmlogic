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

/// `absent` under negation through the real binary (§4/§7, 2026-07-29): the
/// anti-join is a structural membership test, so a stored `absent` refutes an
/// no-match pattern closed to `absent`.
///
/// Both lines of this assertion are the behaviour change, measured on the §16.8
/// sparse-table shape. Before: `unmeasured(absent).` — a food whose id is
/// missing was reported as having no measurement even though
/// `measurement(absent, 3)` is stored — and `contradiction(absent).`, which is
/// P ∧ ¬P. After: the absent-keyed food is refuted like any other, and the
/// contradiction is gone (an empty query prints nothing, §14).
#[test]
fn absent_under_negation_is_a_membership_test() {
    let out = run_file("absent_negation.dl");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "unmeasured(\"kale\").\n");
    assert!(out.stderr.is_empty());
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
fn a_query_in_a_module_does_not_answer_naming_the_file() {
    let dir = std::env::temp_dir().join(format!("datalog-system-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("asks.dl"), "p(1).\n?- p(X).\n").expect("write");
    let root = dir.join("main.dl");
    std::fs::write(&root, "import \"asks.dl\".\n").expect("write");
    let out = run_args(&[root.to_str().unwrap()]);
    assert_eq!(out.code, 2);
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
    // point, symbols print bare, and multiple queries concatenate.
    let out = run_file("features.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "scaled(a, 3.0).\nscaled(b, 6.0).\nmeasure(b, 3.0).\n"
    );
    assert!(out.stderr.is_empty());
}

/// §16.9 — the `as` cast, end to end through the real binary. The first §16
/// example to name its own test (`ROADMAP.md`: no example means no feature), and
/// the output is pinned byte-for-byte rather than described in prose, which is
/// what the retrofit item wants for the other eight.
///
/// Three claims in one run: a cast is the only way two int columns produce a
/// ratio; an unrepresentable conversion is `absent` and so stays *queryable*
/// from both sides; and rendering to `string` is total.
#[test]
fn cast_program_converts_and_guards() {
    let out = run_file_args(
        "16_9_cast.dl",
        &[
            "-q",
            "share(F, R).",
            "-q",
            "amount(V).",
            "-q",
            "unparsed(X).",
            "-q",
            "label(S).",
        ],
    );
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "share(oats, 0.3333333333333333).\n\
         amount(7).\n\
         amount(30).\n\
         unparsed(\"n/a\").\n\
         label(\"9\").\n"
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// §16.10 — every answer shape in one run, pinned byte-for-byte. The queries live
/// in the corpus file rather than in `-q` flags, because here they are the subject.
///
/// Four claims: a non-binding filter keeps the atom's own name; an aggregate
/// binding a variable no atom carries falls to `answer/N` — the boundary the
/// one-directional guard got wrong, since every atom argument *is* projected
/// there; a ground conjunction prints all its atoms in name order; and a body
/// with no answer variables answers `holds(true)` where it used to print nothing
/// whether or not it held.
#[test]
fn answer_shape_program_prints_each_form() {
    let out = run_file("16_10_answer_shape.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "person(\"alice\", 34).\n\
         person(\"carol\", 29).\n\
         answer(\"alice\", 34, 1).\n\
         answer(\"bob\", 17, 1).\n\
         answer(\"carol\", 29, 1).\n\
         banned(\"carol\").\n\
         person(\"bob\", 17).\n\
         holds(true).\n"
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// §16.11 — a query naming itself, pinned byte-for-byte. The queries are again
/// the subject, so they live in the corpus file.
///
/// The first two blocks are the point: the same question asked twice, printing
/// narrowed `person` facts unnamed and `adult` facts named — the projection
/// hazard beside its fix. Then a name replacing the synthesized `answer/N`, an
/// empty projection answering `name(true)`, and a rule reading the relation a
/// query defined, which is what makes the name compose rather than label.
#[test]
fn named_query_program_publishes_its_own_relation() {
    let out = run_file("16_11_named_query.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "person(\"alice\", 34).\n\
         person(\"carol\", 29).\n\
         adult(\"alice\", 34).\n\
         adult(\"carol\", 29).\n\
         with_bans(\"alice\", 34, 1).\n\
         with_bans(\"bob\", 17, 1).\n\
         with_bans(\"carol\", 29, 1).\n\
         clean(true).\n\
         eligible(\"alice\").\n"
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn disjunction_program_runs() {
    let out = run_file("disjunction.dl");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "drinks(alice).\ndrinks(bob).\n");
}

#[test]
fn a_type_error_does_not_answer() {
    let out = run_file("broken_types.dl");
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    // The code, not the prose: this is the surface §12 exists to give a
    // consumer, and the CLI is a consumer.
    assert!(out.stderr.contains("[type-clash]"), "{}", out.stderr);
}

#[test]
fn a_runtime_error_does_not_answer() {
    let out = run_file("broken_arith.dl");
    assert_eq!(out.code, 2);
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
fn a_malformed_file_reports_multiple_errors_and_does_not_answer() {
    let out = run_file("broken_multi.dl");
    assert_eq!(out.code, 2);
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
    assert_eq!(out.code, 2);
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

#[test]
fn an_aggregate_that_skips_absent_warns_on_stderr_but_still_succeeds() {
    // §9's skip-but-report rule at the binary contract: the answer is the
    // aggregate of the values that exist (exit 0, clean stdout), and the drop is
    // reported on stderr so it is never silent.
    let out = run_stdin(
        "m(\"a\", 10). m(\"a\", absent). m(\"b\", 7).\n\
         thing(\"a\"). thing(\"b\").\n\
         mean(T, M) :- thing(T), M = avg { A | m(T, A) }.\n\
         ?- mean(T, M).",
    );
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "mean(\"a\", 10.0).\nmean(\"b\", 7.0).\n");
    assert!(
        out.stderr.contains("`avg` in `mean/2` skipped 1 absent"),
        "{}",
        out.stderr
    );
}

// --- `-q` one-shot queries (spec §14, step 6) ---

/// An aggregate in a `-q` — the form the skill teaches — answers over the
/// variables the query body binds and exits 0. It used to panic (exit 101) on
/// every goal with a named variable, because the projection took every *named*
/// slot and an aggregate's goal-local variables are named but bound only inside
/// the sub-join (§9).
#[test]
fn dash_q_with_an_aggregate() {
    // Goal-local `P`/`C`: one global count.
    let out = run_file_args(
        "16_4_aggregation.dl",
        &["-q", "N = count { C | parent(P, C) }"],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert!(out.stdout.ends_with("answer(3).\n"), "{}", out.stdout);

    // `P` bound outside the aggregate is a group key and is projected.
    let grouped = run_file_args(
        "16_4_aggregation.dl",
        &["-q", "parent(P, _), N = count { C | parent(P, C) }"],
    );
    assert_eq!(grouped.code, 0, "stderr: {}", grouped.stderr);
    assert!(
        grouped
            .stdout
            .ends_with("answer(\"alice\", 2).\nanswer(\"bob\", 1).\n"),
        "{}",
        grouped.stdout
    );

    // The define-and-select form over an aggregate rule.
    let rule = run_file_args(
        "16_4_aggregation.dl",
        &[
            "-q",
            "kids(P, N) :- parent(P, _), N = count { C | parent(P, C) }",
        ],
    );
    assert_eq!(rule.code, 0, "stderr: {}", rule.stderr);
    assert!(
        rule.stdout
            .ends_with("kids(\"alice\", 2).\nkids(\"bob\", 1).\n"),
        "{}",
        rule.stdout
    );
}

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
    // The query ran and found nothing, which is an answer: `1`, not `0` (§14).
    assert_eq!(out.code, 1);
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
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
}

// --- §10 Termination: the lint, and when it is allowed to speak ---

#[test]
fn termination_program_certifies_one_half_and_warns_on_the_other() {
    // §16.12. `doubled` computes exactly as `path_cost` does and says nothing,
    // because no rule feeds it back — that contrast is what the example is for.
    // The uncertified half still runs, answers correctly and exits 0: the
    // warning is the whole of the engine's objection.
    let out = run_file("16_12_termination.dl");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "reach(\"a\", \"b\").\n\
         reach(\"a\", \"c\").\n\
         reach(\"a\", \"d\").\n\
         doubled(\"a\", \"b\", 6).\n\
         path_cost(\"a\", \"d\", 12).\n"
    );
    assert!(
        out.stderr.contains("value-creating recursion"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("`path_cost -> path_cost`"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("`step`"), "{}", out.stderr);
    // Exactly one warning: the certified rules are silent.
    assert_eq!(out.stderr.lines().count(), 1, "{}", out.stderr);
}

#[test]
fn a_nonterminating_program_warns_before_it_hangs() {
    // The eager-emission contract, and nothing else pins it. Answers print after
    // the fixpoint, so a warning carried out on `RunResult` would reach a program
    // that never finishes exactly never — `bugs/004`'s "no output at all". This
    // test therefore reads stderr from a *live* process and then kills it.
    let mut child = Command::new(BIN)
        .arg("tests/programs/nonterminating.dl")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary runs");
    let stderr = child.stderr.take().expect("stderr is piped");

    // Read the first line on another thread: if the warning were *not* eager
    // there would be nothing to read, and a blocking read would hang the suite
    // rather than fail it.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stderr);
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut reader, &mut line);
        let _ = sender.send(line);
    });
    let first = receiver.recv_timeout(std::time::Duration::from_secs(30));
    let _ = child.kill();
    let _ = child.wait();

    let first = first.expect("the warning must arrive while the program is still running");
    assert!(first.contains("value-creating recursion"), "{first:?}");
    assert!(first.contains("`nat -> nat`"), "{first:?}");
    assert!(first.contains("does not terminate"), "{first:?}");
}

#[test]
fn a_rule_no_goal_depends_on_cannot_hang_the_run() {
    // Rule pruning's permissive half (§17, 2026-09-12): `nat` never reaches a
    // fixpoint, but nothing asks about it, so it is never evaluated and the run
    // answers. The termination lint still warns — static warnings cover the
    // whole program. If pruning regresses this hangs, so the child is polled and
    // killed rather than waited on.
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
        .write_all(b"nat(0).\nnat(N) :- nat(M), N = M + 1.\nedge(\"a\", \"b\").\n?- edge(X, Y).\n")
        .unwrap();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("child is waitable") {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the run evaluated a rule no goal depends on, and hung");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut stdout).unwrap();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
    assert_eq!(status.code(), Some(0), "{stderr}");
    assert_eq!(stdout, "edge(\"a\", \"b\").\n");
    assert!(stderr.contains("value-creating recursion"), "{stderr}");
}

/// A fresh directory holding `locked.jsonl` (mode 000: unreadable, but it
/// exists), `u.jsonl` and `ev.jsonl`, for the relation-pruning tests.
#[cfg(feature = "duckdb")]
fn pruning_dir(name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("datalog-prune-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let locked = dir.join("locked.jsonl");
    std::fs::write(&locked, "{\"a\": 1}\n").expect("write");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    std::fs::write(dir.join("u.jsonl"), "{\"a\": 7}\n").expect("write");
    std::fs::write(dir.join("ev.jsonl"), "{\"at\": \"2026-01-01T00:00:00\"}\n").expect("write");
    dir
}

/// Runs `program`, written into `dir` as `main.dl`, so its imports resolve there.
#[cfg(feature = "duckdb")]
fn run_in(dir: &std::path::Path, program: &str) -> Output {
    let main = dir.join("main.dl");
    std::fs::write(&main, program).expect("write");
    run_args(&[main.to_str().unwrap()])
}

#[test]
#[cfg(feature = "duckdb")]
fn a_program_error_arrives_before_any_fact_is_read() {
    // `bugs/012`'s acceptance: the only fact file is unreadable, so a diagnostic
    // that arrives at all arrived without reading it (§17, 2026-09-12).
    let dir = pruning_dir("error");
    let imports = "import \"locked.jsonl\" as t(a: int).\n";

    let lowering = run_in(&dir, &format!("{imports}?- t(nosuch: X).\n"));
    assert_eq!(lowering.code, 2, "{}", lowering.stderr);
    assert!(lowering.stderr.contains("nosuch"), "{}", lowering.stderr);
    assert!(
        !lowering.stderr.contains("locked.jsonl"),
        "{}",
        lowering.stderr
    );

    let syntax = run_in(&dir, &format!("{imports}oops(X) :- t(X)\n"));
    assert_eq!(syntax.code, 2, "{}", syntax.stderr);
    assert!(syntax.stderr.contains("syntax error"), "{}", syntax.stderr);
    assert!(!syntax.stderr.contains("locked.jsonl"), "{}", syntax.stderr);
}

#[test]
#[cfg(feature = "duckdb")]
fn a_relation_no_goal_reaches_is_not_read() {
    let dir = pruning_dir("unreached");
    let imports = "import \"locked.jsonl\" as t(a: int).\nimport \"u.jsonl\" as u(a: int).\n";

    let unreached = run_in(&dir, &format!("{imports}?- u(a: X).\n"));
    assert_eq!(unreached.code, 0, "{}", unreached.stderr);
    assert_eq!(unreached.stdout, "u(7).\n");

    // Ask about it and it is read — and cannot be.
    let reached = run_in(&dir, &format!("{imports}?- t(a: X).\n"));
    assert_eq!(reached.code, 2, "{}", reached.stderr);
    assert!(
        reached.stderr.contains("locked.jsonl"),
        "{}",
        reached.stderr
    );
}

#[test]
#[cfg(feature = "duckdb")]
fn a_missing_import_is_an_error_even_when_no_goal_reaches_it() {
    // Skipping saves the rows, not the check: a typo in a path still fails.
    let dir = pruning_dir("missing");
    let out = run_in(
        &dir,
        "import \"nope.jsonl\" as t(a: int).\nimport \"u.jsonl\" as u(a: int).\n?- u(a: X).\n",
    );
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("file not found"), "{}", out.stderr);
}

#[test]
#[cfg(feature = "duckdb")]
fn a_skipped_relation_cannot_turn_a_program_into_a_type_error() {
    // `ev` is unreached, so its row is not loaded, and without it `A + @1d`
    // over a declared timestamp is `bugs/014`'s false type error. A rejection
    // over a partial load is re-checked over a full one, so this answers.
    let dir = pruning_dir("fallback");
    let out = run_in(
        &dir,
        "import \"ev.jsonl\" as ev(at: timestamp).\nimport \"u.jsonl\" as u(a: int).\n\
         later(T) :- ev(at: A), T = A + @1d.\n?- u(a: X).\n",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "u(7).\n");
}

#[test]
fn an_undefined_predicate_in_a_pruned_rule_still_warns() {
    // Pruning happens at evaluation, after the whole-program lint, so a typo in a
    // rule nothing asks about is still flagged (§17, 2026-09-12).
    let out = run_stdin_args(
        "edge(\"a\", \"b\").\nlonely(X) :- edgee(X, _).\n?- edge(X, Y).\n",
        &[],
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "edge(\"a\", \"b\").\n");
    assert!(
        out.stderr
            .contains("predicate `edgee/2` is referenced but never defined"),
        "{}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// The caller's contract (§14): the exit code answers the question.
//
// Every code in the vocabulary is pinned here, because the vocabulary *is* the
// feature — a caller branches on it and cannot see anything else without
// parsing stdout. The range invariant (`0`/`1` answered, `≥ 2` did not answer)
// is what a later code refines, so these tests are also what stops a refinement
// silently reinterpreting one of them.
// ---------------------------------------------------------------------------

/// §16.13 through the real binary: a consistency check is an ordinary query,
/// and the exit code carries its answer. The two halves differ only in the
/// facts, which is the point — nothing about the *check* changes.
#[test]
fn a_consistency_check_answers_through_the_exit_code() {
    let clean = run_file("16_13_consistency.dl");
    assert_eq!(clean.code, 0, "a clean roster answers yes");
    assert_eq!(clean.stdout, "holds(true).\n");
    assert!(clean.stderr.is_empty(), "{}", clean.stderr);

    let violated = run_file("16_13_consistency_violated.dl");
    assert_eq!(violated.code, 1, "a violated roster answers no");
    assert!(violated.stdout.is_empty(), "{}", violated.stdout);
}

/// §16.14 — the temporal corpus program, end to end through the binary: a date
/// column typed from the file with no schema line, `duration / duration` naming
/// its unit, a range filter over dates, and a period key from `std/time`.
#[cfg(feature = "duckdb")]
#[test]
fn temporal_program_types_dates_and_groups_by_period() {
    let out = run_file_args(
        "16_14_temporal.dl",
        &[
            "-q",
            "days_open(T, N).",
            "-q",
            "slow(T).",
            "-q",
            "recent(T).",
            "-q",
            "june(T).",
            "-q",
            "month_opened(T, M).",
            "-q",
            "per_month(M, N).",
        ],
    );
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "days_open(1, 2.0).\n\
         days_open(2, 3.0).\n\
         days_open(3, 1.0).\n\
         slow(2).\n\
         recent(2).\n\
         recent(3).\n\
         june(1).\n\
         month_opened(1, @2026-06-01).\n\
         month_opened(2, @2026-07-01).\n\
         month_opened(3, @2026-07-01).\n\
         per_month(@2026-06-01, 1).\n\
         per_month(@2026-07-01, 2).\n"
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// The closure property over temporal values (§14): the answers above are
/// valid input, so piping them back in and querying them gives the same rows.
#[cfg(feature = "duckdb")]
#[test]
fn temporal_answers_compose_as_input() {
    let first = run_file_args("16_14_temporal.dl", &["-q", "month_opened(T, M)."]);
    assert_eq!(first.code, 0);
    let again = run_stdin_args(&first.stdout, &["-q", "month_opened(T, M)."]);
    assert_eq!(again.code, 0);
    assert_eq!(
        again.stdout, first.stdout,
        "answers did not re-parse as facts"
    );
}

/// A query that found rows exits `0`; the same query over facts that match
/// nothing exits `1`. Both are answers — the difference from a `2` is that the
/// run completed and the engine knows the answer is "none".
#[test]
fn rows_found_is_zero_and_no_rows_is_one() {
    // `16_9_cast.dl` asks nothing of its own, so the `-q` is the only query in
    // the run and the code is unambiguously about it.
    let found = run_file_args("16_9_cast.dl", &["-q", "share(F, R)"]);
    assert_eq!(found.code, 0);
    assert!(!found.stdout.is_empty());

    let none = run_file_args("16_9_cast.dl", &["-q", "share(nobody, R)"]);
    assert_eq!(none.code, 1);
    assert!(none.stdout.is_empty(), "{}", none.stdout);
}

/// **Any** query decides, so a file that answers on its own masks a `-q` that
/// found nothing (§14). The consequence is the one a caller must know about:
/// the consistency-check idiom is exact over a program whose queries *are* the
/// checks, and a `-q` imposed on a file that asks its own questions is not a
/// check at all. Nothing here is broken — this pins the boundary so a later
/// session changes it deliberately rather than by accident.
#[test]
fn any_query_decides_so_a_files_own_answers_mask_a_dash_q() {
    let out = run_file_args("16_1_ancestry.dl", &["-q", "ancestor(\"nobody\", X)"]);
    assert_eq!(
        out.code, 0,
        "the file's own query answered, so the run found rows"
    );
    assert!(out.stdout.contains("ancestor(\"alice\""), "{}", out.stdout);
}

/// A program with no queries asked nothing, so "no rows" is not an answer to
/// anything: it exits `0`, which keeps `datalog p.dl` usable as a plain check
/// that a program loads, lowers, type-checks and runs.
#[test]
fn a_program_with_no_queries_exits_zero() {
    let out = run_file("16_9_cast.dl");
    assert_eq!(out.code, 0);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
}

/// A warning never moves the code (§12). This program warns on stderr *and*
/// answers on stdout, so it exits `0` — the code reports rows, not happiness.
#[test]
fn a_warning_does_not_change_the_exit_code() {
    let out = run_file("16_12_termination.dl");
    assert_eq!(out.code, 0);
    assert!(!out.stdout.is_empty());
    assert!(
        out.stderr.contains("value-creating recursion"),
        "{}",
        out.stderr
    );
}

/// The two halves of `≥ 2`: a usage problem and a program error report on the
/// same code, because a caller's next move — read stderr, do not trust the
/// output — is the same for both. §12's `category` field carries the finer
/// split, at a granularity an exit code could not reach.
#[test]
fn usage_and_program_errors_share_the_did_not_answer_code() {
    let usage = run_args(&["--nope"]);
    assert_eq!(usage.code, 2);
    let program = run_file("broken_types.dl");
    assert_eq!(program.code, 2);
}

/// §11's asking form, end to end through the binary: a `?why` goal reaches the
/// renderer that until now was library-only, and its block is byte-for-byte
/// §16.6's.
#[test]
fn a_why_goal_prints_the_proof_the_spec_shows() {
    let out = run_file_args(
        "16_1_ancestry.dl",
        &["-q", "?why ancestor(\"alice\",\"carol\")"],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.ends_with(
            "% why ancestor(\"alice\", \"carol\")\n\
             % 0  ancestor(\"alice\", \"carol\")  by ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)\n\
             % 1    parent(\"alice\", \"bob\")  [fact]\n\
             % 1    ancestor(\"bob\", \"carol\")  by ancestor(X, Y) :- parent(X, Y)\n\
             % 2      parent(\"bob\", \"carol\")  [fact]\n"
        ),
        "unexpected proof block:\n{}",
        out.stdout
    );
}

/// **The sigil is not a selector**: either form answers whichever arm the model
/// turns out to hold (§17, 2026-08-16). `?why` over a fact that does not hold is
/// an ordinary question, and `?whynot` over one that does gets a proof — the
/// second at the cost of a second fixpoint, which the answer says out loud
/// because its own sigil provisioned no recorder (§17, 2026-08-21).
#[test]
fn either_sigil_answers_whichever_arm_holds() {
    let why_missing = run_file_args(
        "16_1_ancestry.dl",
        &["-q", "?why ancestor(\"dave\",\"alice\")"],
    );
    assert!(
        why_missing
            .stdout
            .contains("% why ancestor(\"dave\", \"alice\")\n% not derivable\n"),
        "stdout: {}",
        why_missing.stdout
    );

    let whynot_holding = run_file_args(
        "16_1_ancestry.dl",
        &["-q", "?whynot ancestor(\"alice\",\"carol\")"],
    );
    assert!(
        whynot_holding
            .stdout
            .contains("% whynot ancestor(\"alice\", \"carol\")\n"),
        "the header names what was asked: {}",
        whynot_holding.stdout
    );
    assert!(
        whynot_holding
            .stdout
            .contains("% the goal holds; re-ran with provenance recorded\n"),
        "the cross case owes its cost a line: {}",
        whynot_holding.stdout
    );
    assert!(
        whynot_holding.stdout.contains("  by ancestor(X, Y) :-"),
        "and then answers with the proof: {}",
        whynot_holding.stdout
    );
}

/// **Adding an explanation cannot change what a run answers** (§17,
/// 2026-08-21) — the exit-code half of E5's comment-stripping guard, and what
/// makes `?why` safe to append to `datalog check.dl -q '…' && deploy`. Stripping
/// the comments leaves the fact stream byte for byte, which is the other half.
#[test]
fn an_explanation_changes_neither_the_exit_code_nor_the_fact_stream() {
    // A program of its own, because the corpus file carries a query that
    // answers — and this is about a run whose answer is *no*.
    const PROGRAM: &str = "parent(\"alice\", \"bob\").\n";
    let plain = run_stdin_args(PROGRAM, &["-q", "parent(\"zoe\", X)"]);
    let explained = run_stdin_args(
        PROGRAM,
        &[
            "-q",
            "parent(\"zoe\", X)",
            "-q",
            "?whynot parent(\"zoe\",\"alice\")",
        ],
    );
    assert_eq!(plain.code, 1, "the query answers no");
    assert_eq!(
        explained.code, 1,
        "and still answers no with a goal beside it"
    );

    let stripped: String = explained
        .stdout
        .lines()
        .filter(|line| !line.starts_with('%'))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(stripped, plain.stdout, "comments carried a fact");

    // A run whose only goals are explanations asked no question, so it succeeds
    // for the same reason `datalog p.dl` does.
    let only = run_stdin_args(PROGRAM, &["-q", "?why parent(\"alice\",\"bob\")"]);
    assert_eq!(only.code, 0, "stderr: {}", only.stderr);
}

/// A goal names **one fact**, and the diagnostic teaches the division of labour
/// rather than only refusing: `?-` enumerates, `?why`/`?whynot` interrogate one
/// of the rows it returned. This is a likelier mistake than picking the wrong
/// sigil, because both sigils answer.
#[test]
fn a_goal_with_a_variable_is_told_to_ask_a_query_first() {
    let out = run_file_args("16_1_ancestry.dl", &["-q", "?why ancestor(\"alice\", W)"]);
    assert_eq!(out.code, 2);
    assert!(
        out.stderr.contains("is not ground: variable `W`") && out.stderr.contains("?- ancestor(…)"),
        "stderr: {}",
        out.stderr
    );

    let conjunction = run_file_args(
        "16_1_ancestry.dl",
        &["-q", "?why parent(\"a\",\"b\"), parent(\"b\",\"c\")"],
    );
    assert_eq!(conjunction.code, 2);
    assert!(
        conjunction.stderr.contains("explains one fact"),
        "stderr: {}",
        conjunction.stderr
    );
}

/// `?whynot`'s half of the union: a failure trace, one near-miss per rule whose
/// head unifies, re-solved through the scheduler the fixpoint uses so the
/// literal reported as blocked is the one the run really failed.
///
/// The program is the shape `EXPERIMENTS.md` task 6 lost silently — a join
/// across two id-spaces that derived nothing, indistinguishable from a correct
/// empty answer.
#[test]
fn a_whynot_goal_names_the_literal_that_blocked_and_the_step_that_would_pass_it() {
    const PROGRAM: &str = "defined(\"a\", \"id42\").\n\
                           callsite(\"id42\", \"b_impl\").\n\
                           calls(A, B) :- defined(A, Id), callsite(Id, N), defined(B, N).\n";
    let out = run_stdin_args(PROGRAM, &["-q", "?whynot calls(\"a\",\"b\")"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "% whynot calls(\"a\", \"b\")\n\
         % not derivable\n\
         % 0  calls(A, B) :- defined(A, Id), callsite(Id, N), defined(B, N)\n\
         % 1    defined(\"a\", \"id42\")\n\
         % 1    callsite(\"id42\", \"b_impl\")\n\
         % 1    blocked at defined(B, N)\n\
         % 1    repair: add defined(\"b\", \"b_impl\")\n",
        "unexpected trace"
    );
}

/// The repair vocabulary, one case each. **A repair is a step, not a promise**,
/// and three of the five name no fact at all (§17, 2026-08-16).
#[test]
fn a_repair_names_a_fact_only_when_there_is_one_to_name() {
    // Nothing derives the relation: that is the answer, and the commonest one
    // for a mistyped name.
    let asserted = run_stdin_args("p(\"a\").\n", &["-q", "?whynot p(\"z\")"]);
    assert!(
        asserted
            .stdout
            .contains("% no rule derives `p`, so the goal could only be asserted"),
        "stdout: {}",
        asserted.stdout
    );

    // A blocked *derived* premise: the repair is the next question to ask.
    let derived = run_stdin_args(
        "parent(\"alice\", \"bob\").\n\
         ancestor(X, Y) :- parent(X, Y).\n\
         old(X, Y) :- ancestor(X, Y), parent(X, \"bob\").\n",
        &["-q", "?whynot old(\"alice\",\"dan\")"],
    );
    assert!(
        derived
            .stdout
            .contains("repair: ask `?whynot ancestor(\"alice\", \"dan\").`"),
        "stdout: {}",
        derived.stdout
    );

    // A refuted negation names the row that refuted it — there is no retraction
    // in the language, so it is not phrased as a deletion.
    let refuted = run_stdin_args(
        "thing(\"a\").\ncovered(\"a\").\nbare(X) :- thing(X), not covered(X).\n",
        &["-q", "?whynot bare(\"a\")"],
    );
    assert!(
        refuted
            .stdout
            .contains("repair: none — refuted by covered(\"a\")"),
        "stdout: {}",
        refuted.stdout
    );

    // A blocked comparison prints the values it was evaluated on, the same
    // literal-beside-its-values pairing a proof uses for a satisfied one.
    let compared = run_stdin_args(
        "age(\"bob\", 15).\nadult(X) :- age(X, A), A >= 18.\n",
        &["-q", "?whynot adult(\"bob\")"],
    );
    assert!(
        compared.stdout.contains("blocked at A >= 18  (15 >= 18)")
            && compared
                .stdout
                .contains("repair: none — this literal names no fact to add"),
        "stdout: {}",
        compared.stdout
    );
}

/// A reader that goes away is a write failure, not a panic: the run says so on
/// stderr and exits 2, §14's "did not answer". The answer is larger than a pipe's
/// buffer, so the write fails whenever the reader closes.
#[test]
fn a_closed_stdout_exits_2_with_a_message() {
    let mut program: String = (0..20_000).map(|i| format!("n({i}).\n")).collect();
    program.push_str("?- n(X).\n");
    let mut child = Command::new(BIN)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary spawns");
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(program.as_bytes())
        .expect("program written");
    let output = child.wait_with_output().expect("binary exits");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("could not write the answers"),
        "stderr: {stderr}"
    );
}
