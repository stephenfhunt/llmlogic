//! Phase F import properties (testing.md; spec §13, §16.5): the reader stack
//! end to end through `sources::load_table`, over real files.
//!
//! Everything here needs the DuckDB reader, so the whole file is gated on the
//! (default-on) `duckdb` feature; `--no-default-features` runs pin the
//! structured feature error instead (`tests/pipeline.rs`).
#![cfg(feature = "duckdb")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;

use datalog::ast::{FieldDecl, Ident, Span, TypeName};
use datalog::ir::{F64, Value};
use datalog::sources::load_table;

/// A fresh scratch directory per call (hand-rolled; no `tempfile` dep —
/// testing.md Phase F). Best-effort cleanup: the OS owns the temp dir.
fn scratch_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "datalog-import-tests-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn field(name: &str, ty: Option<TypeName>) -> FieldDecl {
    FieldDecl {
        name: Ident {
            name: name.to_string(),
            span: Span::DUMMY,
        },
        ty,
        span: Span::DUMMY,
    }
}

/// RFC 4180 writer (test-side, deliberately independent of the reader):
/// every cell is quoted, `"` doubled, rows CRLF-terminated.
fn write_csv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let quoted: Vec<String> = row
            .iter()
            .map(|cell| format!("\"{}\"", cell.replace('"', "\"\"")))
            .collect();
        out.push_str(&quoted.join(","));
        out.push_str("\r\n");
    }
    out
}

/// JSON writer (test-side): one object per line, full escape table.
fn write_jsonl(keys: &[String], rows: &[Vec<Value>]) -> String {
    let mut out = String::new();
    for row in rows {
        let members: Vec<String> = keys
            .iter()
            .zip(row)
            .map(|(k, v)| format!("{}: {}", json_string(k), json_value(v)))
            .collect();
        out.push_str(&format!("{{{}}}\n", members.join(", ")));
    }
    out
}

fn json_value(value: &Value) -> String {
    match value {
        Value::String(s) => json_string(s),
        Value::Int(n) => n.to_string(),
        // `{:?}` is the shortest round-tripping form and is JSON-legal.
        Value::Float(f) => format!("{:?}", f.get()),
        Value::Bool(b) => b.to_string(),
        Value::Symbol(_) => unreachable!("JSONL carries no symbols"),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The import's set semantics, applied test-side for comparison.
fn as_set(mut rows: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    rows.sort();
    rows.dedup();
    rows
}

// --- Generators ---

/// Arbitrary cell text: printable-ish plus the RFC 4180 stress characters
/// (comma, quote, LF, CRLF) and non-ASCII. Lone `\r` is excluded — bare-CR
/// line endings are not a dialect the spec ratifies.
fn arb_cell() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            proptest::char::range('a', 'z').prop_map(|c| c.to_string()),
            Just(",".to_string()),
            Just("\"".to_string()),
            Just("\n".to_string()),
            Just("\r\n".to_string()),
            Just(" ".to_string()),
            Just("é".to_string()),
            Just("42".to_string()),
        ],
        0..6,
    )
    .prop_map(|parts| parts.concat())
    .prop_filter("lone CR is not ratified", |s| {
        !s.replace("\r\n", "").contains('\r')
    })
}

fn arb_table() -> impl Strategy<Value = Vec<Vec<String>>> {
    (1usize..5).prop_flat_map(|width| {
        proptest::collection::vec(proptest::collection::vec(arb_cell(), width), 0..8)
    })
}

/// Cell text drawn from clean literal/non-literal spellings, so the in-test
/// inference oracle below can be trivially independent of the lexer.
fn arb_clean_cell() -> impl Strategy<Value = String> {
    prop_oneof![
        (0i64..1000).prop_map(|n| n.to_string()),
        (-1000i64..0).prop_map(|n| n.to_string()),
        (0i64..1000).prop_map(|n| format!("{n}.5")),
        Just("true".to_string()),
        Just("false".to_string()),
        Just("abc".to_string()),
        Just("".to_string()),
    ]
}

/// One uniformly-typed JSONL column's values.
fn arb_typed_column(rows: usize) -> BoxedStrategy<Vec<Value>> {
    prop_oneof![
        proptest::collection::vec(any::<i64>().prop_map(Value::Int), rows..=rows).boxed(),
        proptest::collection::vec(
            (-1.0e12f64..1.0e12).prop_map(|f| Value::Float(F64::new(f).expect("finite"))),
            rows..=rows
        )
        .boxed(),
        proptest::collection::vec(any::<bool>().prop_map(Value::Bool), rows..=rows).boxed(),
        proptest::collection::vec(arb_cell().prop_map(Value::String), rows..=rows).boxed(),
    ]
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// F1 — CSV round-trip: arbitrary cells survive the writer → DuckDB →
    /// finalize path verbatim (under an all-string schema, so no cell is
    /// reinterpreted), up to import set semantics.
    #[test]
    fn f1_csv_round_trip(table in arb_table()) {
        let width = table.first().map(Vec::len).unwrap_or(1);
        let fields: Vec<FieldDecl> = (0..width)
            .map(|i| field(&format!("f{i}"), Some(TypeName::String)))
            .collect();
        let names: Vec<String> = fields.iter().map(|f| f.name.name.clone()).collect();
        // A first data row equal to the field names would be taken as a
        // header (§13's skip rule) — not this property's subject.
        prop_assume!(table.first() != Some(&names));

        let path = scratch_dir().join("t.csv");
        std::fs::write(&path, write_csv(&table)).expect("write csv");

        let loaded = load_table(path.to_str().unwrap(), None, Some(&fields))
            .expect("csv loads");
        let expected = as_set(
            table
                .iter()
                .map(|row| row.iter().cloned().map(Value::String).collect())
                .collect(),
        );
        prop_assert_eq!(loaded.rows, expected);
    }

    /// F2 — inference oracle: over clean cell spellings, the imported column
    /// types match an independent implementation of the §13 rules.
    #[test]
    fn f2_inference_matches_the_oracle(table in
        (1usize..4).prop_flat_map(|width| {
            proptest::collection::vec(
                proptest::collection::vec(arb_clean_cell(), width),
                1..8,
            )
        })
    ) {
        let width = table[0].len();
        let header: Vec<String> = (0..width).map(|i| format!("f{i}")).collect();
        let mut rows = vec![header];
        rows.extend(table.iter().cloned());

        let path = scratch_dir().join("t.csv");
        std::fs::write(&path, write_csv(&rows)).expect("write csv");
        let loaded = load_table(path.to_str().unwrap(), None, None).expect("csv loads");

        // The oracle: trivially-independent §13 rules over the clean alphabet.
        for col in 0..width {
            let classes: Vec<&str> = table
                .iter()
                .map(|row| {
                    let cell = row[col].as_str();
                    if cell.parse::<i64>().is_ok() {
                        "int"
                    } else if cell.parse::<f64>().is_ok() {
                        "float"
                    } else if cell == "true" || cell == "false" {
                        "bool"
                    } else {
                        "string"
                    }
                })
                .collect();
            let expected = if classes.iter().all(|c| *c == "int") {
                "int"
            } else if classes.iter().all(|c| *c == "int" || *c == "float") {
                "float"
            } else if classes.iter().all(|c| *c == "bool") {
                "bool"
            } else {
                "string"
            };
            for row in &loaded.rows {
                let actual = match row[col] {
                    Value::Int(_) => "int",
                    Value::Float(_) => "float",
                    Value::Bool(_) => "bool",
                    Value::String(_) => "string",
                    Value::Symbol(_) => "symbol",
                };
                prop_assert_eq!(actual, expected, "column {}", col);
            }
        }
    }

    /// F4 — JSONL round-trip: typed values survive the writer → DuckDB →
    /// finalize path exactly (JSON strings never re-inferred), up to import
    /// set semantics.
    #[test]
    fn f4_jsonl_round_trip(
        (keys, rows) in (1usize..4, 0usize..8).prop_flat_map(|(width, height)| {
            let keys: Vec<String> = (0..width).map(|i| format!("k{i}")).collect();
            let columns: Vec<BoxedStrategy<Vec<Value>>> =
                (0..width).map(|_| arb_typed_column(height)).collect();
            (Just(keys), columns).prop_map(move |(keys, columns)| {
                let rows: Vec<Vec<Value>> = (0..height)
                    .map(|r| columns.iter().map(|c| c[r].clone()).collect())
                    .collect();
                (keys, rows)
            })
        })
    ) {
        let path = scratch_dir().join("t.jsonl");
        std::fs::write(&path, write_jsonl(&keys, &rows)).expect("write jsonl");

        let loaded = load_table(path.to_str().unwrap(), None, None).expect("jsonl loads");
        // An all-int column that meets a float column nowhere stays int; a
        // column generated as floats may contain int-valued floats printed
        // `1.0`, which read back as floats — the generator is uniform per
        // column, so plain equality holds after set semantics.
        if !loaded.rows.is_empty() {
            prop_assert_eq!(&loaded.fields, &keys);
        }
        prop_assert_eq!(loaded.rows, as_set(rows));
    }
}

/// F7 — parquet round-trip: a fixture written at test time via DuckDB `COPY`
/// (no binary files in the repo) reads back with the file's own types.
#[test]
fn f7_parquet_round_trip() {
    let dir = scratch_dir();
    let path = dir.join("t.parquet");
    let conn = duckdb_test_connection();
    conn.execute_batch(&format!(
        "CREATE TABLE t(id BIGINT, name VARCHAR, score DOUBLE, active BOOLEAN);
         INSERT INTO t VALUES
           (1, 'alice', 9.5, true),
           (2, 'bob, \"the builder\"', -0.25, false),
           (2, 'bob, \"the builder\"', -0.25, false),
           (3, '42', 3.0, true);
         COPY t TO '{}' (FORMAT PARQUET);",
        path.display()
    ))
    .expect("write parquet fixture");

    let loaded = load_table(path.to_str().unwrap(), None, None).expect("parquet loads");
    assert_eq!(loaded.fields, ["id", "name", "score", "active"]);
    let f = |x: f64| Value::Float(F64::new(x).unwrap());
    assert_eq!(
        loaded.rows,
        vec![
            vec![
                Value::Int(1),
                Value::String("alice".to_string()),
                f(9.5),
                Value::Bool(true),
            ],
            vec![
                Value::Int(2),
                Value::String("bob, \"the builder\"".to_string()),
                f(-0.25),
                Value::Bool(false),
            ],
            vec![
                Value::Int(3),
                // A VARCHAR `42` stays a string: the file is the authority.
                Value::String("42".to_string()),
                f(3.0),
                Value::Bool(true),
            ],
        ],
        "duplicate row deduplicated, VARCHAR never re-inferred"
    );
}

/// JSONL records must supply the same key set: a missing key is a structured
/// error naming the record and key, not a silent null.
#[test]
fn jsonl_missing_key_is_a_structured_error() {
    let path = scratch_dir().join("t.jsonl");
    std::fs::write(&path, "{\"a\": 1, \"b\": 2}\n{\"a\": 3}\n").expect("write");
    let errors = load_table(path.to_str().unwrap(), None, None).expect_err("missing key");
    let message = errors[0].to_string();
    assert!(message.contains("record 2"), "got: {message}");
    assert!(message.contains("`b`"), "got: {message}");
}

/// The wide-table shape of §16.7: header-derived field names come back in
/// file order, ready for named access.
#[test]
fn csv_header_fields_arrive_in_order() {
    let path = scratch_dir().join("employees.csv");
    std::fs::write(
        &path,
        "id,name,age,dept,title,salary,city,start_date\n\
         1,alice,34,eng,manager,120,berlin,2020-01-02\n",
    )
    .expect("write");
    let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(
        loaded.fields,
        [
            "id",
            "name",
            "age",
            "dept",
            "title",
            "salary",
            "city",
            "start_date"
        ]
    );
    // `2020-01-02` is not a literal of the language: it stays a string.
    assert_eq!(loaded.rows[0][7], Value::String("2020-01-02".to_string()));
}

fn duckdb_test_connection() -> datalog::duckdb::Connection {
    datalog::duckdb::Connection::open_in_memory().expect("open duckdb")
}
