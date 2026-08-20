//! Phase F module-import properties (testing.md; spec §13): splice semantics
//! through the full public pipeline (`datalog::run_at`), over real files.
//! Pure std — module resolution needs no reader backend.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;

/// A fresh scratch directory per call (hand-rolled; no `tempfile` dep).
fn scratch_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "datalog-module-tests-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write test file");
    path
}

fn output_at(path: &Path) -> String {
    let source = std::fs::read_to_string(path).expect("read root");
    datalog::run_at(&source, Some(path))
        .unwrap_or_else(|e| panic!("run failed: {e:?}"))
        .output()
}

fn output(src: &str) -> String {
    datalog::run(src)
        .unwrap_or_else(|e| panic!("run failed: {e:?}"))
        .output()
}

fn facts(pred: &str, rows: &[(i64, i64)]) -> String {
    rows.iter()
        .map(|(a, b)| format!("{pred}({a}, {b}).\n"))
        .collect()
}

/// The three shared queries every generated program ends with, so model
/// equality is observed through the §14 output stream.
const QUERIES: &str = "?- base(A, B).\n?- left(A, B).\n?- right(A, B).\n";

fn arb_rows() -> impl Strategy<Value = Vec<(i64, i64)>> {
    proptest::collection::vec((0i64..6, 0i64..6), 0..8)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// F5 — module diamond: root→{a,b}, a→c, b→c evaluates identically to
    /// the flat concatenation with the shared module included once.
    #[test]
    fn f5_diamond_equals_flat_concatenation(
        base in arb_rows(),
        left in arb_rows(),
        right in arb_rows(),
    ) {
        let dir = scratch_dir();
        let c = facts("base", &base);
        let a = format!("import \"c.dl\".\nleft(X, Y) :- base(X, Y).\n{}", facts("left", &left));
        let b = format!("import \"c.dl\".\nright(Y, X) :- base(X, Y).\n{}", facts("right", &right));
        write(&dir, "c.dl", &c);
        write(&dir, "a.dl", &a);
        write(&dir, "b.dl", &b);
        let root = write(&dir, "main.dl", &format!("import \"a.dl\".\nimport \"b.dl\".\n{QUERIES}"));

        // The flat program: DFS pre-order with c spliced once, at its first
        // (a's) position.
        let a_flat = a.replace("import \"c.dl\".\n", &c);
        let b_flat = b.replace("import \"c.dl\".\n", "");
        let flat = format!("{a_flat}{b_flat}{QUERIES}");

        prop_assert_eq!(output_at(&root), output(&flat));
    }

    /// F6 — mutual import cycle: terminates, and equals the union of the two
    /// files' statements.
    #[test]
    fn f6_cycle_terminates_and_equals_the_union(
        base in arb_rows(),
        left in arb_rows(),
    ) {
        let dir = scratch_dir();
        let x = format!("import \"y.dl\".\n{}left(X, Y) :- base(X, Y).\n", facts("base", &base));
        let y = format!("import \"x.dl\".\n{}right(Y, X) :- base(X, Y).\n", facts("left", &left));
        write(&dir, "x.dl", &x);
        write(&dir, "y.dl", &y);
        let root = write(&dir, "main.dl", &format!("import \"x.dl\".\n{QUERIES}"));

        // x splices first; its own import of y splices y (whose back-import
        // of x is already visited and vanishes).
        let union = format!(
            "{}{}{QUERIES}",
            y.replace("import \"x.dl\".\n", ""),
            x.replace("import \"y.dl\".\n", ""),
        );

        prop_assert_eq!(output_at(&root), output(&union));
    }
}

// --- `std` modules (§13, `notes/temporal-values.md`) ---

/// The gate in both directions: without the import `year` is an ordinary
/// relation the program may define and use; with it, `year` is the builtin.
///
/// This is the property the whole mechanism exists for — gating is what makes
/// short names affordable, and it is only true if an unimporting program is
/// completely unaffected.
#[test]
fn a_gated_name_is_an_ordinary_relation_until_the_module_is_imported() {
    let own = datalog::run("year(1066, hastings). year(1215, runnymede).\n?- year(Y, E).")
        .expect("a program may define its own `year`");
    assert_eq!(
        own.output(),
        "year(1066, hastings).\nyear(1215, runnymede).\n"
    );
    assert!(own.warnings.is_empty(), "{:?}", own.warnings);

    let gated =
        datalog::run("import \"std/time\".\nd(@2026-08-19).\nr(Y) :- d(D), year(D, Y).\n?- r(Y).")
            .expect("the import brings the builtin into scope");
    assert_eq!(gated.output(), "r(2026).\n");
}

#[test]
fn defining_a_gated_name_while_importing_it_names_both_origins() {
    let errors = datalog::run("import \"std/time\".\nyear(1066, hastings).\n?- year(Y, E).")
        .expect_err("a collision is an error, never a silent preference");
    let text = errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("defined by this program"), "{text}");
    assert!(text.contains("std/time"), "{text}");
    assert!(text.contains("rename"), "{text}");
}

#[test]
fn using_a_gated_name_without_the_import_says_which_module_provides_it() {
    // The honest cost of the gate, kept to one step by §12's suggestion.
    let result = datalog::run("d(@2026-08-19).\nr(Y) :- d(D), year(D, Y).\n?- r(Y).")
        .expect("an undefined relation is a warning, not an error");
    let warnings: Vec<String> = result.warnings.iter().map(ToString::to_string).collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("provided by `std/time`"),
        "{warnings:?}"
    );
    assert!(warnings[0].contains("import \"std/time\"."), "{warnings:?}");
}

#[test]
fn an_unknown_std_module_lists_the_ones_that_exist() {
    let errors = datalog::run("import \"std/nope\".\np(1).\n?- p(X).").expect_err("no such module");
    let text = errors[0].to_string();
    assert!(text.contains("there is no `std/nope` module"), "{text}");
    assert!(text.contains("std/time"), "{text}");
}

#[test]
fn a_real_std_directory_cannot_shadow_the_prefix() {
    // `std/` always wins, and the collision is loud rather than resolved
    // silently in either direction (§13).
    let dir = scratch_dir();
    std::fs::create_dir_all(dir.join("std")).expect("create std dir");
    write(&dir, "std/time", "p(1).\n");
    let root = write(&dir, "root.dl", "import \"std/time\".\nq(1).\n?- q(X).");
    let source = std::fs::read_to_string(&root).expect("read root");
    let errors = datalog::run_at(&source, Some(&root)).expect_err("the shadow is an error");
    let text = errors[0].to_string();
    assert!(text.contains("reserved `std/` module prefix"), "{text}");
    assert!(text.contains("also exists on disk"), "{text}");
}

#[test]
fn a_std_import_survives_printing_as_what_it_was_written_as() {
    // §14's closure: resolution reclassifies the import, and the printed form
    // must still be the source form.
    let program = datalog::parse("import \"std/time\".\np(1).").expect("parses");
    let printed = datalog::print::print_program(&program);
    assert!(printed.contains("import \"std/time\"."), "{printed}");
}
