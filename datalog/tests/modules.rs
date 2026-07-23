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
