//! The DuckDB reader backend (§13; `duckdb` cargo feature, on by default).
//!
//! DuckDB does transport and dialect parsing only — typing stays with
//! `sources::table::finalize`:
//!
//! - **CSV** is read `all_varchar` with the RFC 4180 dialect pinned, so every
//!   cell arrives as untyped [`RawValue::Text`] and the literal-grammar
//!   inference is the sole authority. An unquoted empty cell (a DuckDB NULL) is
//!   the absent value (§4); `allow_quoted_nulls=false` keeps a quoted `""` an
//!   empty string, distinct from absence.
//! - **JSONL** is read in two phases: a full-scan `DESCRIBE` fixes the key
//!   set and order (the first record's keys, in order), then the data read
//!   forces every column to the `JSON` type so each value's own JSON text
//!   reaches us verbatim — numbers stay numbers, strings stay strings
//!   byte-exactly, and DuckDB's date/timestamp detection never rewrites
//!   anything. A missing key or an explicit `null` is the absent value (§4);
//!   arrays and objects remain structured errors.
//! - **Parquet** columns are natively typed (the file is the authority) and
//!   are mapped onto [`RawValue`]; a NULL is the absent value (§4); nested
//!   types are structured errors. `DATE` and `TIMESTAMP` columns become §4's
//!   temporal values: transport stays ISO text — DuckDB renders exactly the
//!   grammar [`crate::temporal`] reads — and the *typing* is ours, from the
//!   column's declared type rather than from the text. A zoned timestamp is
//!   converted to UTC and its offset dropped (§13), since §4's timestamp is
//!   civil. An `INTERVAL` stays text, with `UUID` and `JSON`: a duration is
//!   never inferred from any source, because reading DuckDB's `1 day 02:00:00`
//!   would mean carrying a second duration grammar beside §3's.

use duckdb::Connection;
use duckdb::types::ValueRef;

use super::table::RawValue;
use super::{DataFormat, FactSource, RawTable};
use crate::error::{Error, ErrorCode};

pub(crate) struct DuckDbSource {
    format: DataFormat,
}

impl DuckDbSource {
    pub(crate) fn new(format: DataFormat) -> DuckDbSource {
        DuckDbSource { format }
    }
}

impl FactSource for DuckDbSource {
    fn format(&self) -> &'static str {
        match self.format {
            DataFormat::Csv => "csv",
            DataFormat::Jsonl => "jsonl",
            DataFormat::Parquet => "parquet",
        }
    }

    fn read(&self, path: &str) -> Result<RawTable, Error> {
        let conn = Connection::open_in_memory().map_err(|e| {
            Error::new(
                ErrorCode::UnreadableSource,
                format!("`{path}`: cannot open DuckDB: {e}"),
            )
        })?;
        // URLs are read directly over httpfs — DuckDB's `read_csv`/`read_json`/
        // `read_parquet` all accept a URL, so no local file is ever written
        // (a read-only environment can still import from a URL). CSV needs its
        // column count up front, which comes from an in-memory blob fetch.
        let is_url = super::is_url(path);
        if is_url {
            ensure_httpfs(&conn, path)?;
        } else if std::fs::metadata(path).is_err() {
            return Err(Error::new(
                ErrorCode::FileNotFound,
                format!("`{path}`: file not found"),
            ));
        }
        match self.format {
            DataFormat::Csv => {
                let shape_bytes = if is_url {
                    fetch_bytes(&conn, path)?
                } else {
                    std::fs::read(path).map_err(|e| source_error(path, e))?
                };
                read_csv(&conn, path, &shape_bytes)
            }
            DataFormat::Jsonl => read_jsonl(&conn, path),
            DataFormat::Parquet => read_parquet(&conn, path),
        }
    }
}

/// Loads httpfs for URL reads (a runtime `INSTALL httpfs; LOAD httpfs;` on
/// first use — a network fetch). Persists on the connection for later reads.
fn ensure_httpfs(conn: &Connection, url: &str) -> Result<(), Error> {
    conn.execute_batch("INSTALL httpfs; LOAD httpfs;")
        .map_err(|e| {
            Error::new(
                ErrorCode::UnreadableSource,
                format!(
                    "`{url}`: could not load the httpfs extension for URL imports \
             (a network fetch on first use): {e}"
                ),
            )
        })
}

/// Fetches a URL's whole content into memory via httpfs (used only to compute
/// the CSV column count — the actual read streams from the URL directly).
fn fetch_bytes(conn: &Connection, url: &str) -> Result<Vec<u8>, Error> {
    let sql = format!("SELECT content FROM read_blob({})", sql_string(url));
    let mut stmt = conn.prepare(&sql).map_err(|e| source_error(url, e))?;
    let mut rows = stmt.query([]).map_err(|e| source_error(url, e))?;
    match rows.next().map_err(|e| source_error(url, e))? {
        Some(row) => match row.get_ref(0).map_err(|e| source_error(url, e))? {
            ValueRef::Blob(bytes) => Ok(bytes.to_vec()),
            other => Err(Error::new(
                ErrorCode::UnreadableSource,
                format!("`{url}`: unexpected fetch result {other:?}"),
            )),
        },
        None => Err(Error::new(
            ErrorCode::UnreadableSource,
            format!("`{url}`: the URL returned no content"),
        )),
    }
}

/// Escapes a string for a single-quoted SQL literal.
fn sql_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Escapes an identifier for a double-quoted SQL identifier.
fn sql_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn source_error(path: &str, e: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::UnreadableSource, format!("`{path}`: {e}"))
}

// --- CSV ---

/// Reads a CSV from `source` (a local path or an httpfs URL). `shape_bytes`
/// is the file content used only to determine the column count and row
/// terminator — the same bytes for a local file, or the fetched blob for a
/// URL. The row data itself streams from `source`.
fn read_csv(conn: &Connection, source: &str, shape_bytes: &[u8]) -> Result<RawTable, Error> {
    // An empty file is a legal (schema-required) empty table; DuckDB itself
    // refuses to read it.
    if shape_bytes.is_empty() {
        return Ok(RawTable {
            columns: None,
            rows: Vec::new(),
        });
    }
    // Sniffing is disabled entirely — DuckDB's dialect sniffer both is
    // non-deterministic surface and rejects legal edge shapes (a file that is
    // one multi-line quoted field). The RFC 4180 dialect is pinned; the only
    // two facts `read_csv` cannot be told to figure out alone are the column
    // count and the row terminator, which one quote-aware scan of the first
    // record supplies.
    let (width, new_line) = first_record_shape(shape_bytes);
    let columns_spec = (0..width)
        .map(|i| format!("'c{i}': 'VARCHAR'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT * FROM read_csv({}, header=false, all_varchar=true, \
         delim=',', quote='\"', escape='\"', auto_detect=false, \
         allow_quoted_nulls=false, new_line='{new_line}', columns={{{columns_spec}}})",
        sql_string(source)
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| source_error(source, e))?;
    let mut rows = stmt.query([]).map_err(|e| source_error(source, e))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| source_error(source, e))? {
        let mut cells = Vec::with_capacity(width);
        for col in 0..width {
            match row.get_ref(col).map_err(|e| source_error(source, e))? {
                ValueRef::Text(bytes) => cells.push(RawValue::Text(utf8(bytes, source)?)),
                // An unquoted empty cell is a DuckDB NULL → the absent value
                // (§4/§13). `allow_quoted_nulls=false` keeps a quoted `""` as an
                // empty-string `Text`, so absence and the empty string stay
                // distinct.
                ValueRef::Null => cells.push(RawValue::Absent),
                other => {
                    return Err(Error::new(
                        ErrorCode::UnconvertibleCell,
                        format!(
                            "`{source}`: unexpected non-text CSV cell {other:?} (all_varchar read)"
                        ),
                    ));
                }
            }
        }
        out.push(cells);
    }
    Ok(RawTable {
        columns: None,
        rows: out,
    })
}

/// Scans the first logical record (quote-aware, so embedded newlines and `""`
/// escapes don't fool it) for the column count and the row terminator (`\n`
/// or `\r\n`; a single-record file defaults to `\n`).
fn first_record_shape(bytes: &[u8]) -> (usize, &'static str) {
    let mut width = 1usize;
    let mut in_quotes = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if !in_quotes => in_quotes = true,
            b'"' if bytes.get(i + 1) == Some(&b'"') => i += 1, // an escaped `""`
            b'"' => in_quotes = false,
            b',' if !in_quotes => width += 1,
            b'\n' if !in_quotes => {
                let crlf = i > 0 && bytes[i - 1] == b'\r';
                return (width, if crlf { "\\r\\n" } else { "\\n" });
            }
            _ => {}
        }
        i += 1;
    }
    (width, "\\n")
}

// --- JSONL ---

fn read_jsonl(conn: &Connection, path: &str) -> Result<RawTable, Error> {
    // Phase 1: fix the key set and order with a full-scan describe. DuckDB
    // orders unified keys by first appearance, so this is the first record's
    // key order with later-only keys appended (§13).
    let describe = format!(
        "DESCRIBE SELECT * FROM read_json({}, format='newline_delimited', sample_size=-1)",
        sql_string(path)
    );
    let columns = describe_column_names(conn, &describe, path)?;
    if columns.is_empty() {
        return Ok(RawTable {
            columns: Some(columns),
            rows: Vec::new(),
        });
    }

    // Phase 2: force every column to the JSON type so each value's own JSON
    // text arrives verbatim, then classify per §13's JSONL rules.
    let column_spec = columns
        .iter()
        .map(|c| format!("{}: 'JSON'", sql_string(c)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT * FROM read_json({}, format='newline_delimited', columns={{{column_spec}}})",
        sql_string(path)
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| source_error(path, e))?;
    let mut rows = stmt.query([]).map_err(|e| source_error(path, e))?;
    let mut out = Vec::new();
    let mut row_number = 0usize;
    while let Some(row) = rows.next().map_err(|e| source_error(path, e))? {
        row_number += 1;
        let mut cells = Vec::with_capacity(columns.len());
        for (col, name) in columns.iter().enumerate() {
            match row.get_ref(col).map_err(|e| source_error(path, e))? {
                // A missing key surfaces as a NULL → the absent value (§4/§13);
                // an explicit JSON `null` is classified in `json_scalar`.
                ValueRef::Null => cells.push(RawValue::Absent),
                ValueRef::Text(bytes) => {
                    cells.push(json_scalar(&utf8(bytes, path)?, path, row_number, name)?)
                }
                other => {
                    return Err(Error::new(
                        ErrorCode::UnconvertibleCell,
                        format!("`{path}`: unexpected JSON column value {other:?}"),
                    ));
                }
            }
        }
        out.push(cells);
    }
    Ok(RawTable {
        columns: Some(columns),
        rows: out,
    })
}

/// Classifies one raw JSON value text per §13: strings stay strings (never
/// re-inferred), fraction/exponent-free numbers are i64-checked ints, other
/// numbers are floats; `null` is the absent value (§4); arrays and objects are
/// structured errors.
fn json_scalar(text: &str, path: &str, row: usize, key: &str) -> Result<RawValue, Error> {
    let scalar_error = |what: &str| {
        Error::new(
            ErrorCode::UnconvertibleCell,
            format!(
                "`{path}`: record {row}, key `{key}`: {what} (values must be JSON \
             scalars: string, number, or boolean)"
            ),
        )
    };
    match text.as_bytes().first() {
        Some(b'"') => Ok(RawValue::Str(unescape_json_string(text).map_err(|e| {
            Error::new(
                ErrorCode::UnconvertibleCell,
                format!("`{path}`: record {row}, key `{key}`: {e}"),
            )
        })?)),
        Some(b't') if text == "true" => Ok(RawValue::Bool(true)),
        Some(b'f') if text == "false" => Ok(RawValue::Bool(false)),
        // An explicit JSON `null` is the absent value (§4/§13), like a missing key.
        Some(b'n') if text == "null" => Ok(RawValue::Absent),
        Some(b'[') => Err(scalar_error("the value is an array")),
        Some(b'{') => Err(scalar_error("the value is a nested object")),
        Some(_) if !text.contains(['.', 'e', 'E']) => text
            .parse::<i64>()
            .map(RawValue::Int)
            .map_err(|_| scalar_error(&format!("`{text}` does not fit a 64-bit int"))),
        Some(_) => match text.parse::<f64>() {
            Ok(f) if f.is_finite() => Ok(RawValue::Float(f)),
            _ => Err(scalar_error(&format!("`{text}` is not a finite number"))),
        },
        None => Err(scalar_error("the value is empty")),
    }
}

/// Unescapes the contents of a raw JSON string literal (quotes included).
fn unescape_json_string(text: &str) -> Result<String, String> {
    let inner = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .ok_or_else(|| format!("malformed JSON string `{text}`"))?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let unit = json_hex4(&mut chars)?;
                let code = if (0xD800..0xDC00).contains(&unit) {
                    // A leading surrogate: the pair must follow.
                    if chars.next() != Some('\\') || chars.next() != Some('u') {
                        return Err("unpaired surrogate in JSON string".to_string());
                    }
                    let low = json_hex4(&mut chars)?;
                    if !(0xDC00..0xE000).contains(&low) {
                        return Err("unpaired surrogate in JSON string".to_string());
                    }
                    0x10000 + (((unit - 0xD800) as u32) << 10) + (low - 0xDC00) as u32
                } else {
                    unit as u32
                };
                out.push(
                    char::from_u32(code)
                        .ok_or_else(|| "invalid \\u escape in JSON string".to_string())?,
                );
            }
            other => return Err(format!("invalid JSON escape `\\{}`", other.unwrap_or(' '))),
        }
    }
    Ok(out)
}

fn json_hex4(chars: &mut std::str::Chars<'_>) -> Result<u16, String> {
    let mut value = 0u16;
    for _ in 0..4 {
        let digit = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| "invalid \\u escape in JSON string".to_string())?;
        value = value * 16 + digit as u16;
    }
    Ok(value)
}

// --- Parquet ---

fn read_parquet(conn: &Connection, path: &str) -> Result<RawTable, Error> {
    let from = format!("read_parquet({})", sql_string(path));
    let describe = format!("DESCRIBE SELECT * FROM {from}");
    let described = describe_columns(conn, &describe, path)?;

    // The file's types are the authority; map them onto the value types (§13).
    // Date and timestamp columns travel as ISO text and are *typed* here, from
    // the declared type — so a column of dates is a `date` column even where a
    // row's text would also read as something else.
    let mut selects = Vec::with_capacity(described.len());
    let mut temporal_columns = Vec::with_capacity(described.len());
    for (name, ty) in &described {
        let ident = sql_ident(name);
        let ty = ty.to_ascii_uppercase();
        temporal_columns.push(match ty.as_str() {
            "DATE" => Some(TemporalColumn::Date),
            _ if ty.starts_with("TIMESTAMP") => Some(TemporalColumn::Timestamp),
            _ => None,
        });
        let select = match ty.as_str() {
            "BOOLEAN" | "TINYINT" | "SMALLINT" | "INTEGER" | "BIGINT" | "HUGEINT" | "UTINYINT"
            | "USMALLINT" | "UINTEGER" | "UBIGINT" | "UHUGEINT" | "FLOAT" | "DOUBLE"
            | "VARCHAR" => ident,
            _ if ty.starts_with("DECIMAL") => format!("CAST({ident} AS DOUBLE)"),
            "DATE" | "UUID" | "INTERVAL" | "JSON" => format!("CAST({ident} AS VARCHAR)"),
            _ if ty.starts_with("TIME") => format!("CAST({ident} AS VARCHAR)"),
            _ => {
                return Err(Error::new(
                    ErrorCode::UnsupportedColumn,
                    format!(
                        "`{path}`: column `{name}` has type {ty}, which does not map onto \
                     the value types (symbol/string/int/float/bool)"
                    ),
                ));
            }
        };
        selects.push(select);
    }
    let sql = format!("SELECT {} FROM {from}", selects.join(", "));
    let mut stmt = conn.prepare(&sql).map_err(|e| source_error(path, e))?;
    let mut rows = stmt.query([]).map_err(|e| source_error(path, e))?;
    let columns: Vec<String> = described.iter().map(|(name, _)| name.clone()).collect();
    let mut out = Vec::new();
    let mut row_number = 0usize;
    while let Some(row) = rows.next().map_err(|e| source_error(path, e))? {
        row_number += 1;
        let mut cells = Vec::with_capacity(columns.len());
        for (col, name) in columns.iter().enumerate() {
            let value = row.get_ref(col).map_err(|e| source_error(path, e))?;
            let mapped = map_typed_value(value, path, row_number, name)?;
            cells.push(match temporal_columns[col] {
                Some(kind) => read_temporal_cell(mapped, kind, path, row_number, name)?,
                None => mapped,
            });
        }
        out.push(cells);
    }
    Ok(RawTable {
        columns: Some(columns),
        rows: out,
    })
}

/// Maps one natively-typed DuckDB value onto [`RawValue`].
fn map_typed_value(
    value: ValueRef<'_>,
    path: &str,
    row: usize,
    column: &str,
) -> Result<RawValue, Error> {
    let cell_error = |what: String| {
        Error::new(
            ErrorCode::UnconvertibleCell,
            format!("`{path}`: row {row}, column `{column}`: {what}"),
        )
    };
    let int128 = |n: i128| {
        i64::try_from(n)
            .map(RawValue::Int)
            .map_err(|_| cell_error(format!("`{n}` does not fit a 64-bit int")))
    };
    match value {
        // A Parquet/DB NULL is the absent value (§4/§13).
        ValueRef::Null => Ok(RawValue::Absent),
        ValueRef::Boolean(b) => Ok(RawValue::Bool(b)),
        ValueRef::TinyInt(n) => Ok(RawValue::Int(n as i64)),
        ValueRef::SmallInt(n) => Ok(RawValue::Int(n as i64)),
        ValueRef::Int(n) => Ok(RawValue::Int(n as i64)),
        ValueRef::BigInt(n) => Ok(RawValue::Int(n)),
        ValueRef::HugeInt(n) => int128(n),
        ValueRef::UTinyInt(n) => Ok(RawValue::Int(n as i64)),
        ValueRef::USmallInt(n) => Ok(RawValue::Int(n as i64)),
        ValueRef::UInt(n) => Ok(RawValue::Int(n as i64)),
        ValueRef::UBigInt(n) => int128(n as i128),
        ValueRef::UHugeInt(n) => i128::try_from(n)
            .map_err(|_| cell_error(format!("`{n}` does not fit a 64-bit int")))
            .and_then(int128),
        ValueRef::Float(f) => finite(f as f64, path, row, column),
        ValueRef::Double(f) => finite(f, path, row, column),
        ValueRef::Text(bytes) => Ok(RawValue::Str(utf8(bytes, path)?)),
        other => Err(cell_error(format!(
            "value {other:?} does not map onto the value types"
        ))),
    }
}

/// Which temporal type a source column declared.
#[derive(Debug, Clone, Copy)]
enum TemporalColumn {
    Date,
    Timestamp,
}

/// Types a cell from a declared temporal column (§13).
///
/// The permissive reader is used deliberately: DuckDB renders a timestamp with
/// a space separator and a zoned one with an offset, and both are forms §13
/// coerces rather than infers. The offset is *applied* and dropped — the
/// instant survives, the displayed clock reading may change, and §13 says so
/// rather than leaving it to be discovered.
fn read_temporal_cell(
    value: RawValue,
    kind: TemporalColumn,
    path: &str,
    row: usize,
    column: &str,
) -> Result<RawValue, Error> {
    let RawValue::Str(text) = &value else {
        // A NULL is already the absent value, and nothing else can arrive from
        // a column DuckDB rendered as text.
        return Ok(value);
    };
    let parsed = match kind {
        TemporalColumn::Date => crate::temporal::parse_date(text.trim()).map(RawValue::Date),
        TemporalColumn::Timestamp => {
            crate::temporal::parse_timestamp_lenient(text).map(RawValue::Timestamp)
        }
    };
    parsed.map_err(|error| {
        Error::new(
            ErrorCode::UnconvertibleCell,
            format!(
                "`{path}`: row {row}, column `{column}`: `{text}` is not a temporal value \
             this engine can hold ({})",
                error.message()
            ),
        )
    })
}

fn finite(f: f64, path: &str, row: usize, column: &str) -> Result<RawValue, Error> {
    if f.is_finite() {
        Ok(RawValue::Float(f))
    } else {
        Err(Error::new(
            ErrorCode::UnconvertibleCell,
            format!("`{path}`: row {row}, column `{column}`: `{f}` is not a finite float"),
        ))
    }
}

// --- Shared helpers ---

fn utf8(bytes: &[u8], path: &str) -> Result<String, Error> {
    std::str::from_utf8(bytes).map(str::to_string).map_err(|_| {
        Error::new(
            ErrorCode::UnreadableSource,
            format!("`{path}`: the file is not valid UTF-8"),
        )
    })
}

/// Runs a `DESCRIBE` and returns `(column_name, column_type)` pairs.
fn describe_columns(
    conn: &Connection,
    describe_sql: &str,
    path: &str,
) -> Result<Vec<(String, String)>, Error> {
    let mut stmt = conn
        .prepare(describe_sql)
        .map_err(|e| source_error(path, e))?;
    let mut rows = stmt.query([]).map_err(|e| source_error(path, e))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| source_error(path, e))? {
        let name = match row.get_ref(0).map_err(|e| source_error(path, e))? {
            ValueRef::Text(bytes) => utf8(bytes, path)?,
            other => {
                return Err(Error::new(
                    ErrorCode::UnreadableSource,
                    format!("`{path}`: unexpected DESCRIBE output {other:?}"),
                ));
            }
        };
        let ty = match row.get_ref(1).map_err(|e| source_error(path, e))? {
            ValueRef::Text(bytes) => utf8(bytes, path)?,
            other => {
                return Err(Error::new(
                    ErrorCode::UnreadableSource,
                    format!("`{path}`: unexpected DESCRIBE output {other:?}"),
                ));
            }
        };
        out.push((name, ty));
    }
    Ok(out)
}

fn describe_column_names(
    conn: &Connection,
    describe_sql: &str,
    path: &str,
) -> Result<Vec<String>, Error> {
    Ok(describe_columns(conn, describe_sql, path)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_unescaping_covers_the_escape_table() {
        assert_eq!(unescape_json_string("\"a\"").unwrap(), "a");
        assert_eq!(
            unescape_json_string("\"a\\\"b\\\\c\\/d\\n\\t\\r\\b\\f\"").unwrap(),
            "a\"b\\c/d\n\t\r\u{0008}\u{000C}"
        );
        assert_eq!(unescape_json_string("\"\\u0041\"").unwrap(), "A");
        // A surrogate pair.
        assert_eq!(unescape_json_string("\"\\uD83D\\uDE00\"").unwrap(), "😀");
        assert!(unescape_json_string("\"\\uD83D\"").is_err());
        assert!(unescape_json_string("\"\\q\"").is_err());
    }

    #[test]
    fn json_scalars_classify_per_spec() {
        let ok = |text: &str| json_scalar(text, "t.jsonl", 1, "k").unwrap();
        let fails = |text: &str| json_scalar(text, "t.jsonl", 1, "k").unwrap_err();

        assert_eq!(ok("\"42\""), RawValue::Str("42".to_string()));
        assert_eq!(ok("42"), RawValue::Int(42));
        assert_eq!(ok("-7"), RawValue::Int(-7));
        assert_eq!(ok("2.5"), RawValue::Float(2.5));
        assert_eq!(ok("1e3"), RawValue::Float(1000.0));
        assert_eq!(ok("true"), RawValue::Bool(true));
        assert_eq!(ok("false"), RawValue::Bool(false));
        // A JSON null is the absent value (§4/§13), not an error.
        assert_eq!(ok("null"), RawValue::Absent);
        assert!(fails("[1]").to_string().contains("array"));
        assert!(fails("{\"a\":1}").to_string().contains("nested"));
        assert!(
            fails("99999999999999999999")
                .to_string()
                .contains("64-bit int")
        );
    }
}
