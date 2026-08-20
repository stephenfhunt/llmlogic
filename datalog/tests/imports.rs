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
use proptest::strategy::ValueTree;

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
        // A missing value serializes back to a JSON null (§13).
        Value::Absent => "null".to_string(),
        Value::Symbol(_) => unreachable!("JSONL carries no symbols"),
        Value::Date(_) | Value::Timestamp(_) | Value::Duration(_) => {
            unreachable!("the JSONL generator emits no temporal values")
        }
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

fn type_name(ty: TypeName) -> &'static str {
    match ty {
        TypeName::Symbol => "symbol",
        TypeName::String => "string",
        TypeName::Int => "int",
        TypeName::Float => "float",
        TypeName::Bool => "bool",
        TypeName::Date => "date",
        TypeName::Timestamp => "timestamp",
        TypeName::Duration => "duration",
    }
}

/// A value as an untyped CSV cell: the raw text for strings, the canonical
/// literal for everything else (so it lexes back to the same value).
///
/// A temporal cell drops the `@`, which is the anchor property's own rule
/// (§13): the sigil is a delimiter the *reader* supplies, exactly as a string's
/// quotes are — a CSV cell holds `2026-08-19`, not `@2026-08-19`.
fn cell_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Date(d) => d.to_string(),
        Value::Timestamp(t) => t.to_string(),
        Value::Duration(d) => d.to_string(),
        other => datalog::print::print_value(other),
    }
}

/// A value as an in-program Datalog literal (strings quoted).
fn datalog_literal(value: &Value) -> String {
    datalog::print::print_value(value)
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
                    Value::Absent => "absent",
                    Value::Date(_) => "date",
                    Value::Timestamp(_) => "timestamp",
                    Value::Duration(_) => "duration",
                };
                prop_assert_eq!(actual, expected, "column {}", col);
            }
        }
    }

    /// F3 — the anchor property: a typed table imported with an explicit
    /// schema evaluates to the same answers as the same facts written as
    /// in-program literals. Runs the full pipeline twice and compares output.
    #[test]
    fn f3_import_equals_inline_facts(
        (fields, rows) in (1usize..4, 0usize..8).prop_flat_map(|(width, height)| {
            let fields: Vec<(String, TypeName)> = (0..width)
                .map(|i| (format!("f{i}"), TypeName::String))
                .collect();
            let columns: Vec<BoxedStrategy<Vec<Value>>> =
                (0..width).map(|_| arb_typed_column(height)).collect();
            (Just(fields), columns).prop_map(move |(mut fields, columns)| {
                for (i, col) in columns.iter().enumerate() {
                    fields[i].1 = match col.first() {
                        Some(Value::Int(_)) => TypeName::Int,
                        Some(Value::Float(_)) => TypeName::Float,
                        Some(Value::Bool(_)) => TypeName::Bool,
                        _ => TypeName::String,
                    };
                }
                let rows: Vec<Vec<Value>> = (0..height)
                    .map(|r| columns.iter().map(|c| c[r].clone()).collect())
                    .collect();
                (fields, rows)
            })
        })
    ) {
        let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
        // A headerless CSV under an explicit schema: skip the (astronomically
        // unlikely) first-row-equals-field-names case, which is the header rule.
        let csv_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.iter().map(cell_text).collect())
            .collect();
        prop_assume!(csv_rows.first() != Some(&names));

        let query = format!(
            "?- p({}).\n",
            (0..fields.len()).map(|i| format!("V{i}")).collect::<Vec<_>>().join(", ")
        );

        let dir = scratch_dir();
        let path = dir.join("t.csv");
        std::fs::write(&path, write_csv(&csv_rows)).expect("write csv");
        let schema_text = fields
            .iter()
            .map(|(n, t)| format!("{n}: {}", type_name(*t)))
            .collect::<Vec<_>>()
            .join(", ");
        let import_program = format!(
            "import {:?} as p({schema_text}).\n{query}",
            path.to_str().unwrap()
        );

        let inline_facts: String = rows
            .iter()
            .map(|r| {
                let cells = r.iter().map(datalog_literal).collect::<Vec<_>>().join(", ");
                format!("p({cells}).\n")
            })
            .collect();
        // An empty import defines `p`, but an empty inline program leaves it
        // undefined; a lone `declare` gives the inline side the same relation.
        let inline_program = format!(
            "declare p({schema_text}).\n{inline_facts}{query}"
        );

        let from_import = datalog::run(&import_program)
            .unwrap_or_else(|e| panic!("import program: {e:?}"))
            .output();
        let from_inline = datalog::run(&inline_program)
            .unwrap_or_else(|e| panic!("inline program: {e:?}"))
            .output();
        prop_assert_eq!(from_import, from_inline);
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
/// A missing JSONL key is the absent value (§4/§13), not an error: the key set
/// comes from the first record, and a later record lacking `b` yields absent —
/// without forcing `b`'s type (it stays int from the present records).
#[test]
fn jsonl_missing_key_becomes_absent() {
    let path = scratch_dir().join("t.jsonl");
    std::fs::write(&path, "{\"a\": 1, \"b\": 2}\n{\"a\": 3}\n").expect("write");
    let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(loaded.fields, ["a", "b"]);
    assert_eq!(
        loaded.rows,
        vec![
            vec![Value::Int(1), Value::Int(2)],
            vec![Value::Int(3), Value::Absent],
        ]
    );
}

/// An explicit JSON `null` is absent too (§4/§13) — same as a missing key.
#[test]
fn jsonl_explicit_null_becomes_absent() {
    let path = scratch_dir().join("t.jsonl");
    std::fs::write(&path, "{\"a\": 1, \"b\": 2}\n{\"a\": 3, \"b\": null}\n").expect("write");
    let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(
        loaded.rows,
        vec![
            vec![Value::Int(1), Value::Int(2)],
            vec![Value::Int(3), Value::Absent],
        ]
    );
}

/// The USDA dogfood failure (§17, 2026-07-24): an empty numeric cell no longer
/// forces the column to string — it stays int, the gap becoming absent.
#[test]
fn csv_empty_cell_is_absent_and_keeps_the_column_numeric() {
    let path = scratch_dir().join("t.csv");
    std::fs::write(&path, "food,amount\napple,5\nbanana,\ncherry,7\n").expect("write");
    let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(loaded.fields, ["food", "amount"]);
    assert_eq!(
        loaded.rows,
        vec![
            vec![Value::String("apple".into()), Value::Int(5)],
            vec![Value::String("banana".into()), Value::Absent],
            vec![Value::String("cherry".into()), Value::Int(7)],
        ]
    );
}

/// §16.8 end-to-end: a sparse CSV imports with a gap in a numeric column, and
/// the absent value flows through named access, a threshold (silently
/// excluding the gap), and a presence test — the full pipeline, imports → eval.
#[test]
fn worked_example_16_8_sparse_nutrient_table() {
    let dir = scratch_dir();
    let csv = dir.join("food_nutrient.csv");
    std::fs::write(
        &csv,
        "food,nutrient,amount\nbread,iron,3\nspinach,iron,\nbeef,iron,8\n",
    )
    .expect("write csv");
    let src = format!(
        "import \"{}\" as measurement.\n\
         high_iron(F) :- measurement(food: F, nutrient: \"iron\", amount: A), A >= 5.\n\
         missing(F) :- measurement(food: F, amount: A), A is absent.\n\
         recorded(F, A) :- measurement(food: F, nutrient: \"iron\", amount: A).\n\
         ?- high_iron(F).\n?- missing(F).\n?- recorded(F, A).\n",
        csv.to_str().unwrap()
    );
    let result = datalog::run(&src).unwrap_or_else(|e| panic!("run: {e:?}"));
    assert_eq!(result.answers[0], vec!["high_iron(\"beef\").".to_string()]);
    assert_eq!(result.answers[1], vec!["missing(\"spinach\").".to_string()]);
    // Every row returns; the gap round-trips as `absent`.
    assert_eq!(
        result.answers[2],
        vec![
            "recorded(\"beef\", 8).".to_string(),
            "recorded(\"bread\", 3).".to_string(),
            "recorded(\"spinach\", absent).".to_string(),
        ]
    );
}

/// `allow_quoted_nulls=false`: an unquoted empty cell is absent, but a quoted
/// `""` is the empty string — the two stay distinct (§13, 2026-07-24).
#[test]
fn csv_quoted_empty_is_a_string_unquoted_empty_is_absent() {
    let path = scratch_dir().join("t.csv");
    std::fs::write(&path, "k,note\napple,\"\"\nbanana,\n").expect("write");
    let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(
        loaded.rows,
        vec![
            vec![Value::String("apple".into()), Value::String(String::new())],
            vec![Value::String("banana".into()), Value::Absent],
        ]
    );
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
    // An ISO-8601 extended cell types the column `date` with no schema line
    // (§13, 2026-08-19): the `@` is a delimiter the reader supplies, exactly as
    // a string's quotes are, and this default path is the case S4 is about.
    assert_eq!(
        loaded.rows[0][7],
        Value::Date(datalog::temporal::Date::from_ymd(2020, 1, 2).expect("a real day"))
    );
}

fn duckdb_test_connection() -> datalog::duckdb::Connection {
    datalog::duckdb::Connection::open_in_memory().expect("open duckdb")
}

/// URL import happy path (§13): reads a small stable public CSV over httpfs.
/// Ignored by default — it needs network and a runtime httpfs install; run
/// with `cargo test -- --ignored` when online. `flights.csv` has a
/// lowercase-identifier header (`year,month,passengers`), so schema inference
/// applies with no explicit schema.
#[test]
#[ignore = "network + runtime httpfs extension install"]
fn url_csv_import_reads_over_httpfs() {
    let url = "https://raw.githubusercontent.com/mwaskom/seaborn-data/master/flights.csv";
    let loaded = load_table(url, None, None).expect("url import over httpfs");
    assert_eq!(loaded.fields, ["year", "month", "passengers"]);
    // 12 years × 12 months in this well-known dataset.
    assert_eq!(loaded.rows.len(), 144);
}

/// A schema-less import's header field names reach lowering, so named access
/// to a wide imported relation works with no `declare` (§13; §16.7). This is
/// the case that was a structured error before §13 landed.
#[test]
fn named_access_to_a_schema_less_import_works() {
    let dir = scratch_dir();
    let path = dir.join("employees.csv");
    std::fs::write(
        &path,
        "id,name,age,dept,title\n\
         1,alice,34,eng,manager\n\
         2,bob,28,eng,engineer\n\
         3,carol,45,sales,manager\n",
    )
    .expect("write");
    let program = format!(
        "import {:?} as employee.\n\
         manager_name(N) :- employee(name: N, title: \"manager\").\n\
         ?- manager_name(N).\n",
        path.to_str().unwrap()
    );
    let result = datalog::run(&program).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(
        answers,
        vec!["manager_name(\"alice\").", "manager_name(\"carol\")."]
    );
}

/// A source error names the offending file and row (§12/§13): a cell that will
/// not coerce to the explicit column type.
#[test]
fn a_bad_cell_reports_file_row_and_column() {
    let dir = scratch_dir();
    let path = dir.join("ages.csv");
    std::fs::write(&path, "30\nabc\n").expect("write");
    let program = format!(
        "import {:?} as p(age: int).\n?- p(A).\n",
        path.to_str().unwrap()
    );
    let errors = datalog::run(&program).expect_err("bad cell");
    let message = errors[0].to_string();
    assert!(message.contains("ages.csv"), "got: {message}");
    assert!(message.contains("row 2"), "got: {message}");
    assert!(message.contains("`abc` is not an int"), "got: {message}");
}

// --- Temporal columns (§13, `notes/temporal-values.md`) ---

fn arb_temporal_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        (0i64..9999, 1i64..=12, 1i64..=28).prop_map(|(y, m, d)| Value::Date(
            datalog::temporal::Date::from_ymd(y, m, d).expect("a real day")
        )),
        (
            0i64..9999,
            1i64..=12,
            1i64..=28,
            0i64..24,
            0i64..60,
            0i64..60,
            0i64..1_000_000
        )
            .prop_map(|(y, mo, d, h, mi, s, us)| Value::Timestamp(
                datalog::temporal::Timestamp::from_parts(y, mo, d, h, mi, s, us).expect("in range")
            )),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// **T6** — the anchor property, extended to the sigil (§13).
    ///
    /// A CSV of temporal cells imports to exactly the values the corresponding
    /// **`@`-sigilled literals** denote. This is the acceptance property the
    /// widened generators owe (`testing.md` rule 4): inference now reads a form
    /// the lexer alone does not, and the claim that an import still "means the
    /// facts you would write" has to survive that.
    #[test]
    fn t6_temporal_cells_import_as_their_literals(
        values in proptest::collection::vec(arb_temporal_value(), 1..6)
    ) {
        // One type per column, since a column is one type (§4): dates and
        // timestamps in one column would widen, which is a different claim.
        let kind_matches = |v: &Value| {
            std::mem::discriminant(v) == std::mem::discriminant(&values[0])
        };
        prop_assume!(values.iter().all(kind_matches));

        let rows: Vec<Vec<String>> = values.iter().map(|v| vec![cell_text(v)]).collect();
        let mut csv = String::from("d\n");
        for row in &rows {
            csv.push_str(&row[0]);
            csv.push('\n');
        }
        let path = scratch_dir().join("t.csv");
        std::fs::write(&path, csv).expect("write csv");
        let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");

        // Import set semantics: compare as sets, as every other anchor check does.
        let imported: std::collections::BTreeSet<Value> =
            loaded.rows.iter().map(|row| row[0].clone()).collect();
        let written: std::collections::BTreeSet<Value> = values.iter().cloned().collect();
        prop_assert_eq!(imported, written);
    }
}

/// The non-vacuity half of T6: the generator reaches both temporal types, and
/// the cells it writes are genuinely unsigilled — a generator that emitted `@`
/// would be testing the lexer rather than the reader.
#[test]
fn t6_generator_writes_unsigilled_cells_of_both_types() {
    let mut runner = proptest::test_runner::TestRunner::default();
    let (mut dates, mut timestamps) = (false, false);
    for _ in 0..100 {
        let value = arb_temporal_value()
            .new_tree(&mut runner)
            .expect("generates")
            .current();
        assert!(
            !cell_text(&value).contains('@'),
            "a CSV cell carries no sigil"
        );
        match value {
            Value::Date(_) => dates = true,
            Value::Timestamp(_) => timestamps = true,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(dates && timestamps, "the generator missed a temporal type");
}

/// A declared temporal type **coerces** and is deliberately more permissive
/// than inference (§13): the space-separated form real exports carry is a
/// string to inference and a timestamp to a schema.
#[test]
fn a_declared_timestamp_reads_what_inference_leaves_alone() {
    let path = scratch_dir().join("t.csv");
    std::fs::write(&path, "at\n2026-08-19 10:30:00\n").expect("write csv");

    let inferred = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(
        inferred.rows[0][0],
        Value::String("2026-08-19 10:30:00".to_string()),
        "inference is strict about the form"
    );

    let schema = [field("at", Some(TypeName::Timestamp))];
    let declared = load_table(path.to_str().unwrap(), None, Some(&schema)).expect("loads");
    assert_eq!(
        declared.rows[0][0],
        Value::Timestamp(
            datalog::temporal::Timestamp::from_parts(2026, 8, 19, 10, 30, 0, 0).expect("in range")
        )
    );
}

/// A column mixing dates and timestamps widens to `timestamp`, exactly as an
/// int/float mix widens — and for the same reason: the widening is exact.
#[test]
fn a_mixed_temporal_column_widens_to_timestamp() {
    let path = scratch_dir().join("t.csv");
    std::fs::write(&path, "at\n2026-08-19\n2026-08-20T10:30:00\n").expect("write csv");
    let loaded = load_table(path.to_str().unwrap(), None, None).expect("loads");
    assert_eq!(
        loaded.rows[0][0],
        Value::Timestamp(
            datalog::temporal::Timestamp::from_parts(2026, 8, 19, 0, 0, 0, 0).expect("in range")
        ),
        "a date widens at midnight"
    );
}
