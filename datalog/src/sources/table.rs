//! The shared §13 semantics layer: raw backend output → a typed, deduplicated
//! fact table.
//!
//! Backends read; this module types. CSV type inference is **defined by the
//! language's own literal grammar** (§13/§17, 2026-07-23): a cell is int,
//! float, or bool iff [`crate::lexer::lex`] reads it as exactly that literal
//! (optionally signed), otherwise it is a string — so no second classifier
//! exists, and an import means precisely the facts you would get by writing
//! its cells as in-program literals.
//!
//! That classifier lives in [`crate::lexer`], not here, because §8's `as` cast
//! converts *from* `string` with the same one. Its home is the grammar that
//! defines it, so neither caller owns it and the two cannot drift apart.

use crate::ast::{FieldDecl, TypeName};
use crate::error::{Error, ErrorCode};
use crate::ir::{F64, Value};
use crate::lexer::{CellClass, classify_cell, classify_symbol};
use crate::temporal;

/// One loaded data import after every §13 rule has been applied: field names
/// are valid and unique, rows are rectangular and column-uniform, and the row
/// set is sorted and deduplicated (set semantics at import time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTable {
    pub fields: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// A backend-produced table before typing.
///
/// `columns` is `Some` for self-describing sources (JSONL keys, Parquet or
/// database columns) and `None` for CSV, where header extraction is
/// [`finalize`]'s decision.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawTable {
    pub columns: Option<Vec<String>>,
    pub rows: Vec<Vec<RawValue>>,
}

/// One backend-produced cell.
///
/// `Text` is an untyped CSV cell — the literal-grammar inference applies.
/// The typed variants come from sources that are their own authority (JSON,
/// Parquet, databases); a `Str` is definitely a string and is never
/// re-inferred (a JSON `"42"` stays a string).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RawValue {
    Text(String),
    Str(String),
    Int(i64),
    /// Always finite: backends reject NaN/±inf at read.
    Float(f64),
    Bool(bool),
    /// A temporal value a **typed** source declared (§13): a Parquet or
    /// database `DATE`/`TIMESTAMP` column. An untyped CSV cell arrives as
    /// [`Text`](Self::Text) and is typed by the literal grammar instead.
    Date(crate::temporal::Date),
    Timestamp(crate::temporal::Timestamp),
    /// A missing value from any source (§4/§13): an empty (unquoted) CSV cell, a
    /// missing JSON key or explicit `null`, a Parquet/DB `NULL`. Type-neutral in
    /// inference and coerced to [`Value::Absent`] under any column type.
    Absent,
}

/// A legal field name is a single identifier token spelled exactly as given
/// (so keywords, uppercase-initial names, and padded text all fail).
fn is_legal_field_name(name: &str) -> bool {
    classify_symbol(name).as_deref() == Some(name)
}

/// Applies every §13 rule to a backend's raw table. `source` names the import
/// in errors (the path as resolved).
pub(crate) fn finalize(
    raw: RawTable,
    schema: Option<&[FieldDecl]>,
    source: &str,
) -> Result<LoadedTable, Vec<Error>> {
    let (fields, data, first_data_row) = arrange(raw, schema, source)?;

    let mut errors = Vec::new();
    let arity = fields.len();
    for (index, row) in data.iter().enumerate() {
        if row.len() != arity {
            errors.push(Error::new(
                ErrorCode::SourceSchemaMismatch,
                format!(
                    "in `{source}`: row {} has {} column(s), expected {arity}",
                    first_data_row + index,
                    row.len(),
                ),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let declared: Vec<Option<TypeName>> = match schema {
        Some(schema) => schema.iter().map(|f| f.ty).collect(),
        None => vec![None; arity],
    };

    let mut columns: Vec<Vec<Value>> = Vec::with_capacity(arity);
    for (col, declared_ty) in declared.iter().enumerate() {
        match type_column(
            &data,
            col,
            *declared_ty,
            &fields[col],
            first_data_row,
            source,
        ) {
            Ok(values) => columns.push(values),
            Err(mut column_errors) => errors.append(&mut column_errors),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut rows: Vec<Vec<Value>> = (0..data.len())
        .map(|r| columns.iter().map(|c| c[r].clone()).collect())
        .collect();
    rows.sort();
    rows.dedup();
    Ok(LoadedTable { fields, rows })
}

/// [`arrange`]'s result: the field names, the data rows (reordered to field
/// order where binding is by name), and the 1-based source row number of the
/// first data row (for error messages).
type Arranged = (Vec<String>, Vec<Vec<RawValue>>, usize);

/// Resolves field names and the data-row window: header extraction for CSV,
/// name binding for self-describing sources.
fn arrange(
    raw: RawTable,
    schema: Option<&[FieldDecl]>,
    source: &str,
) -> Result<Arranged, Vec<Error>> {
    match (raw.columns, schema) {
        // A self-describing source with no records (an empty JSONL file) has no
        // keys to bind by name. Under an explicit schema it is exactly the empty
        // relation; without one nothing names its fields — the empty-CSV rule.
        (Some(columns), schema) if columns.is_empty() && raw.rows.is_empty() => match schema {
            Some(schema) => {
                let fields: Vec<String> = schema.iter().map(|f| f.name.name.clone()).collect();
                validate_field_names(&fields, source)?;
                Ok((fields, Vec::new(), 1))
            }
            None => Err(vec![Error::new(
                ErrorCode::SourceSchemaMismatch,
                format!(
                    "in `{source}`: the source has no records, so nothing names its fields; \
                 an explicit schema is required"
                ),
            )]),
        },
        // Self-describing source, inferred schema: the source's names win.
        (Some(columns), None) => {
            validate_field_names(&columns, source)?;
            Ok((columns, raw.rows, 1))
        }
        // Self-describing source, explicit schema: bind by name (set
        // equality), reorder to schema order.
        (Some(columns), Some(schema)) => {
            let fields: Vec<String> = schema.iter().map(|f| f.name.name.clone()).collect();
            validate_field_names(&fields, source)?;
            let mut errors = Vec::new();
            let positions: Vec<usize> = fields
                .iter()
                .filter_map(|name| {
                    let position = columns.iter().position(|c| c == name);
                    if position.is_none() {
                        errors.push(Error::new(
                            ErrorCode::SourceSchemaMismatch,
                            format!(
                                "in `{source}`: the explicit schema names `{name}`, which the \
                             source does not have (source fields: {})",
                                columns.join(", ")
                            ),
                        ));
                    }
                    position
                })
                .collect();
            for column in &columns {
                if !fields.contains(column) {
                    errors.push(Error::new(
                        ErrorCode::SourceSchemaMismatch,
                        format!(
                            "in `{source}`: the source has field `{column}`, which the \
                         explicit schema does not name (schema fields: {})",
                            fields.join(", ")
                        ),
                    ));
                }
            }
            if !errors.is_empty() {
                return Err(errors);
            }
            let rows = raw
                .rows
                .into_iter()
                .map(|row| positions.iter().map(|&p| row[p].clone()).collect())
                .collect();
            Ok((fields, rows, 1))
        }
        // CSV, inferred schema: the first row is the header.
        (None, None) => {
            let mut rows = raw.rows.into_iter();
            let Some(header) = rows.next() else {
                return Err(vec![Error::new(
                    ErrorCode::SourceSchemaMismatch,
                    format!(
                        "in `{source}`: the file is empty; a header row (or an explicit \
                     schema) is required"
                    ),
                )]);
            };
            let names: Vec<String> = header.iter().map(raw_text).collect();
            validate_field_names(&names, source)?;
            Ok((names, rows.collect(), 2))
        }
        // CSV, explicit schema: every row is data — except a first row whose
        // cells exactly equal the schema's field names, which is a header and
        // is skipped (§13, decided 2026-07-23).
        (None, Some(schema)) => {
            let fields: Vec<String> = schema.iter().map(|f| f.name.name.clone()).collect();
            validate_field_names(&fields, source)?;
            let mut rows = raw.rows;
            let mut first_data_row = 1;
            if rows
                .first()
                .is_some_and(|row| row.iter().map(raw_text).collect::<Vec<_>>() == fields)
            {
                rows.remove(0);
                first_data_row = 2;
            }
            Ok((fields, rows, first_data_row))
        }
    }
}

/// The text of an untyped cell (header rows are always untyped CSV text).
fn raw_text(value: &RawValue) -> String {
    match value {
        RawValue::Text(t) | RawValue::Str(t) => t.clone(),
        RawValue::Int(n) => n.to_string(),
        RawValue::Float(f) => f.to_string(),
        RawValue::Bool(b) => b.to_string(),
        // Unsigilled, as everywhere text and temporal meet (§8/§13). Only
        // reachable for a typed source's header position, which is not a data
        // cell — but the spelling still has to be the one grammar's.
        RawValue::Date(d) => d.to_string(),
        RawValue::Timestamp(t) => t.to_string(),
        // A header/name position that is missing has no text — the empty string,
        // which is not a legal field name (caught by `validate_field_names`).
        RawValue::Absent => String::new(),
    }
}

fn validate_field_names(names: &[String], source: &str) -> Result<(), Vec<Error>> {
    let mut errors = Vec::new();
    for (index, name) in names.iter().enumerate() {
        if !is_legal_field_name(name) {
            errors.push(Error::new(
                ErrorCode::SourceSchemaMismatch,
                format!(
                    "in `{source}`: field {} (`{name}`) is not a legal field name \
                 (§3: lowercase-initial identifier); give the import an explicit \
                 schema to rename it",
                    index + 1
                ),
            ));
        }
        if names[..index].contains(name) {
            errors.push(Error::new(
                ErrorCode::SourceSchemaMismatch,
                format!(
                    "in `{source}`: duplicate field name `{name}`; give the import an \
                 explicit schema to rename it"
                ),
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// The inferred or declared type of one column, then its materialized values.
fn type_column(
    data: &[Vec<RawValue>],
    col: usize,
    declared: Option<TypeName>,
    field: &str,
    first_data_row: usize,
    source: &str,
) -> Result<Vec<Value>, Vec<Error>> {
    let ty = match declared {
        Some(ty) => ty,
        None => infer_column(data, col, field, source)?,
    };

    let mut errors = Vec::new();
    let mut values = Vec::with_capacity(data.len());
    for (index, row) in data.iter().enumerate() {
        match coerce(&row[col], ty) {
            Ok(value) => values.push(value),
            Err(reason) => errors.push(Error::new(
                ErrorCode::UnconvertibleCell,
                format!(
                    "in `{source}`: row {}, column `{field}`: {reason}",
                    first_data_row + index,
                ),
            )),
        }
    }
    if errors.is_empty() {
        Ok(values)
    } else {
        Err(errors)
    }
}

/// §13 column inference: unify the cells' classifications. Untyped (CSV)
/// columns fall back to string on any conflict — the cells' own text is the
/// value. Typed sources widen int/float and otherwise conflict as an error
/// (the strict-typing pillar; there is no text to fall back to).
fn infer_column(
    data: &[Vec<RawValue>],
    col: usize,
    field: &str,
    source: &str,
) -> Result<TypeName, Vec<Error>> {
    let mut inferred: Option<TypeName> = None;
    for (index, row) in data.iter().enumerate() {
        let cell_ty = match &row[col] {
            // Absent is type-neutral (§4): it does not participate in the
            // column's type, so a numeric column with gaps still infers int/float.
            RawValue::Absent => continue,
            RawValue::Text(t) => match classify_cell(t) {
                CellClass::Int(_) => TypeName::Int,
                CellClass::Float(_) => TypeName::Float,
                CellClass::Bool(_) => TypeName::Bool,
                CellClass::Date(_) => TypeName::Date,
                CellClass::Timestamp(_) => TypeName::Timestamp,
                CellClass::Str => TypeName::String,
            },
            RawValue::Str(_) => TypeName::String,
            RawValue::Int(_) => TypeName::Int,
            RawValue::Float(_) => TypeName::Float,
            RawValue::Bool(_) => TypeName::Bool,
            RawValue::Date(_) => TypeName::Date,
            RawValue::Timestamp(_) => TypeName::Timestamp,
        };
        inferred = Some(match (inferred, cell_ty) {
            (None, ty) => ty,
            (Some(a), b) if a == b => a,
            (Some(TypeName::Int), TypeName::Float) | (Some(TypeName::Float), TypeName::Int) => {
                TypeName::Float
            }
            // A date widens to a timestamp exactly as an int widens to a float,
            // and for the same reason: the widening is *exact* (midnight), so
            // nothing is lost by unifying the column (§13).
            (Some(TypeName::Date), TypeName::Timestamp)
            | (Some(TypeName::Timestamp), TypeName::Date) => TypeName::Timestamp,
            (Some(a), b) => {
                // A conflict. Untyped text always has the string reading;
                // typed sources do not.
                if matches!(row[col], RawValue::Text(_)) {
                    return Ok(TypeName::String);
                }
                return Err(vec![Error::new(
                    ErrorCode::UnsupportedColumn,
                    format!(
                        "in `{source}`: column `{field}` mixes {} and {} (row {}); the \
                     value space has no mixed columns — declare an explicit type",
                        type_label(a),
                        type_label(b),
                        index + 1,
                    ),
                )]);
            }
        });
    }
    // A column with no non-absent cell (an empty table, or an all-absent
    // column) has no inferable type. The fallback string never reaches a value:
    // every cell coerces to `absent` regardless (`coerce`), and imports pin no
    // declared field type, so a use site — else nothing — resolves the column
    // (§4/§13, 2026-07-24).
    Ok(inferred.unwrap_or(TypeName::String))
}

/// Converts one cell to a declared/inferred column type, or returns the clause
/// explaining why it could not (the caller prefixes source, row and column).
fn coerce(value: &RawValue, ty: TypeName) -> Result<Value, String> {
    let fail = |value: &RawValue| Err(format!("{} is not {}", render(value), type_label(ty)));
    // A missing value inhabits any column (§4): it is coerced to `absent`
    // regardless of the column's type, and is never a type violation. Real
    // cross-type cells (a `"abc"` in an int column) still fail below.
    if matches!(value, RawValue::Absent) {
        return Ok(Value::Absent);
    }
    match ty {
        TypeName::Int => match value {
            RawValue::Text(t) => match classify_cell(t) {
                CellClass::Int(n) => Ok(Value::Int(n)),
                _ => fail(value),
            },
            RawValue::Int(n) => Ok(Value::Int(*n)),
            _ => fail(value),
        },
        TypeName::Float => match value {
            RawValue::Text(t) => match classify_cell(t) {
                CellClass::Float(f) => new_float(f).ok_or_else(|| not_finite(value)),
                CellClass::Int(n) => widen_int(n),
                _ => fail(value),
            },
            RawValue::Float(f) => new_float(*f).ok_or_else(|| not_finite(value)),
            RawValue::Int(n) => widen_int(*n),
            _ => fail(value),
        },
        TypeName::Bool => match value {
            RawValue::Text(t) => match classify_cell(t) {
                CellClass::Bool(b) => Ok(Value::Bool(b)),
                _ => fail(value),
            },
            RawValue::Bool(b) => Ok(Value::Bool(*b)),
            _ => fail(value),
        },
        TypeName::String => match value {
            RawValue::Text(t) | RawValue::Str(t) => Ok(Value::String(t.clone())),
            _ => fail(value),
        },
        TypeName::Symbol => match value {
            RawValue::Text(t) | RawValue::Str(t) => match classify_symbol(t) {
                Some(name) => Ok(Value::Symbol(name)),
                None => fail(value),
            },
            _ => fail(value),
        },
        // A declared temporal type *coerces*, and is deliberately more
        // permissive than inference (§13): `parse_timestamp_lenient` is what
        // reads the space-separated and zone-suffixed forms real exports carry,
        // which inference leaves as strings on purpose.
        TypeName::Date => match value {
            RawValue::Date(date) => Ok(Value::Date(*date)),
            RawValue::Text(t) | RawValue::Str(t) => temporal::parse_date(t.trim())
                .map(Value::Date)
                .or(fail(value)),
            _ => fail(value),
        },
        TypeName::Timestamp => match value {
            RawValue::Timestamp(timestamp) => Ok(Value::Timestamp(*timestamp)),
            // A date in a timestamp column is midnight — the exact widening
            // `date as timestamp` performs, and what a mixed column infers to.
            RawValue::Date(date) => Ok(Value::Timestamp(date.at_midnight())),
            RawValue::Text(t) | RawValue::Str(t) => temporal::parse_timestamp_lenient(t)
                .map(Value::Timestamp)
                .or(fail(value)),
            _ => fail(value),
        },
        TypeName::Duration => match value {
            RawValue::Text(t) | RawValue::Str(t) => temporal::parse_duration(t.trim())
                .map(Value::Duration)
                .or(fail(value)),
            _ => fail(value),
        },
    }
}

fn new_float(f: f64) -> Option<Value> {
    F64::new(f).ok().map(Value::Float)
}

/// Widens an integer cell into a **float** column — which happens when the
/// column holds both integers and floats, so inference unified it to `float`
/// (§13) — rejecting the widening when it would not be exact.
///
/// Above 2⁵³ an `i64` generally has no exact `f64`, and this is the *import*
/// path over untrusted data: large integer identifiers are precisely what CSVs
/// and Parquet files carry, and one float cell elsewhere in the column is enough
/// to drag the whole column to `float`. Silently rounding an ID would corrupt
/// every join on it, so it is a structured source error naming the cell instead
/// (§13, 2026-07-25). The exactness test itself is
/// [`crate::ir::i64_as_exact_f64`], shared with §8's `as` cast; only the message
/// is this path's own.
fn widen_int(n: i64) -> Result<Value, String> {
    let widened = crate::ir::i64_as_exact_f64(n).ok_or_else(|| {
        format!(
            "`{n}` cannot be widened to a float without losing precision (this column \
             mixes integers and floats, so it is float; give the column an explicit \
             `int` type, or keep the values below 2^53)"
        )
    })?;
    new_float(widened).ok_or_else(|| format!("`{n}` widens to a non-finite float"))
}

/// The clause for a cell that is numeric but not a finite float (NaN/∞).
fn not_finite(value: &RawValue) -> String {
    format!("{} is not a finite float", render(value))
}

fn render(value: &RawValue) -> String {
    match value {
        RawValue::Text(t) | RawValue::Str(t) => format!("`{t}`"),
        RawValue::Int(n) => format!("`{n}`"),
        RawValue::Float(f) => format!("`{f}`"),
        RawValue::Bool(b) => format!("`{b}`"),
        RawValue::Date(d) => format!("`@{d}`"),
        RawValue::Timestamp(t) => format!("`@{t}`"),
        // Absent never reaches `render` (it coerces to a value, never fails),
        // but name it for completeness.
        RawValue::Absent => "absent".to_string(),
    }
}

fn type_label(ty: TypeName) -> &'static str {
    match ty {
        TypeName::Symbol => "a symbol",
        TypeName::String => "a string",
        TypeName::Int => "an int",
        TypeName::Float => "a float",
        TypeName::Bool => "a bool",
        TypeName::Date => "a date",
        TypeName::Timestamp => "a timestamp",
        TypeName::Duration => "a duration",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FieldDecl, Ident, Span};

    fn text_row(cells: &[&str]) -> Vec<RawValue> {
        cells
            .iter()
            .map(|c| RawValue::Text(c.to_string()))
            .collect()
    }

    fn csv(rows: &[&[&str]]) -> RawTable {
        RawTable {
            columns: None,
            rows: rows.iter().map(|r| text_row(r)).collect(),
        }
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

    fn ok(table: RawTable, schema: Option<&[FieldDecl]>) -> LoadedTable {
        finalize(table, schema, "test.csv").expect("finalize succeeds")
    }

    fn err(table: RawTable, schema: Option<&[FieldDecl]>) -> Vec<Error> {
        finalize(table, schema, "test.csv").expect_err("finalize fails")
    }

    // --- Literal-grammar cell classification (§13 inference) ---

    #[test]
    fn all_int_column_infers_int() {
        let table = ok(csv(&[&["n"], &["1"], &["-42"], &["007"]]), None);
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Int(-42)],
                vec![Value::Int(1)],
                vec![Value::Int(7)],
            ]
        );
    }

    #[test]
    fn int_float_mix_widens_to_float() {
        let table = ok(csv(&[&["x"], &["1"], &["2.5"]]), None);
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Float(F64::new(1.0).unwrap())],
                vec![Value::Float(F64::new(2.5).unwrap())],
            ]
        );
    }

    #[test]
    fn bool_column_infers_bool_only_on_exact_literals() {
        let table = ok(csv(&[&["b"], &["true"], &["false"]]), None);
        assert_eq!(
            table.rows,
            vec![vec![Value::Bool(false)], vec![Value::Bool(true)]]
        );

        // `TRUE` is not the language's bool literal, so the column is strings.
        let table = ok(csv(&[&["b"], &["TRUE"], &["false"]]), None);
        assert_eq!(
            table.rows,
            vec![
                vec![Value::String("TRUE".to_string())],
                vec![Value::String("false".to_string())],
            ]
        );
    }

    #[test]
    fn absent_cell_is_type_neutral_and_keeps_the_column_typed() {
        // An empty (unquoted) cell reaches `finalize` as `RawValue::Absent`
        // (§4/§13): it does not force the column to string — the other cells
        // still type it int, and the gap materializes as `absent`.
        let table = ok(
            RawTable {
                columns: None,
                rows: vec![
                    text_row(&["n"]),
                    text_row(&["1"]),
                    vec![RawValue::Absent],
                    text_row(&["3"]),
                ],
            },
            None,
        );
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Absent],
                vec![Value::Int(1)],
                vec![Value::Int(3)],
            ]
        );
    }

    #[test]
    fn a_quoted_empty_string_cell_stays_a_string() {
        // A quoted `""` arrives as `RawValue::Text("")` (distinct from absent);
        // an empty string is a string, so the column is string.
        let table = ok(csv(&[&["n"], &["1"], &[""]]), None);
        assert_eq!(
            table.rows,
            vec![
                vec![Value::String(String::new())],
                vec![Value::String("1".to_string())],
            ]
        );
    }

    #[test]
    fn an_all_absent_column_materializes_all_absent() {
        // No non-absent cell ⇒ no inferable type; every cell is absent, and the
        // predicate's column is left to use-site flow (unpinned here).
        let table = ok(
            RawTable {
                columns: None,
                rows: vec![
                    text_row(&["c"]),
                    vec![RawValue::Absent],
                    vec![RawValue::Absent],
                ],
            },
            None,
        );
        assert_eq!(table.rows, vec![vec![Value::Absent]]);
    }

    #[test]
    fn non_literal_spellings_stay_verbatim_strings() {
        // NaN/inf are not literals (§3 has no NaN token); nor is `1 2`.
        let table = ok(csv(&[&["x"], &["NaN"], &["inf"], &["1 2"]]), None);
        let strings: Vec<&Value> = table.rows.iter().map(|r| &r[0]).collect();
        assert_eq!(
            strings,
            [
                &Value::String("1 2".to_string()),
                &Value::String("NaN".to_string()),
                &Value::String("inf".to_string()),
            ]
        );
    }

    #[test]
    fn scientific_notation_is_a_float_literal_iff_the_lexer_says_so() {
        // Whatever the lexer accepts as a float literal, the import accepts —
        // the two can never disagree because there is only one rulebook.
        let cell = "1e5";
        let lexes_as_float = matches!(classify_cell(cell), CellClass::Float(_));
        let table = ok(csv(&[&["x"], &[cell]]), None);
        match &table.rows[0][0] {
            Value::Float(_) => assert!(lexes_as_float),
            Value::String(s) => {
                assert!(!lexes_as_float);
                assert_eq!(s, cell);
            }
            other => panic!("unexpected value {other:?}"),
        }
    }

    #[test]
    fn whitespace_padding_follows_the_lexer() {
        // The lexer skips whitespace around a token, so ` 42 ` is the int 42 —
        // one rulebook, no trimming pass of our own.
        let table = ok(csv(&[&["n"], &[" 42 "]]), None);
        assert_eq!(table.rows, vec![vec![Value::Int(42)]]);
    }

    // --- Explicit schemas ---

    #[test]
    fn explicit_string_keeps_a_numeric_cell_verbatim() {
        let schema = [field("code", Some(TypeName::String))];
        let table = ok(csv(&[&["42"]]), Some(&schema));
        assert_eq!(table.rows, vec![vec![Value::String("42".to_string())]]);
    }

    #[test]
    fn explicit_symbol_reads_bare_identifiers() {
        let schema = [field("color", Some(TypeName::Symbol))];
        let table = ok(csv(&[&["red"], &["blue"]]), Some(&schema));
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Symbol("blue".to_string())],
                vec![Value::Symbol("red".to_string())],
            ]
        );

        // Uppercase-initial text is a variable, not a symbol literal.
        let errors = err(csv(&[&["Red"]]), Some(&schema));
        assert!(
            errors[0].to_string().contains("not a symbol"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn absent_is_exempt_from_a_declared_type_but_real_violations_still_error() {
        let schema = [field("age", Some(TypeName::Int))];
        // An absent cell under `int` coerces to absent, not a type error…
        let table = ok(
            RawTable {
                columns: None,
                rows: vec![vec![RawValue::Int(30)], vec![RawValue::Absent]],
            },
            Some(&schema),
        );
        assert_eq!(table.rows, vec![vec![Value::Absent], vec![Value::Int(30)]]);
        // …but a genuinely non-int cell still fails.
        let errors = err(csv(&[&["30"], &["abc"]]), Some(&schema));
        assert!(
            errors[0].to_string().contains("not an int"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn explicit_int_reports_the_offending_cell_row_and_column() {
        let schema = [field("age", Some(TypeName::Int))];
        let errors = err(csv(&[&["30"], &["abc"]]), Some(&schema));
        let message = errors[0].to_string();
        assert!(message.contains("row 2"), "got: {message}");
        assert!(message.contains("column `age`"), "got: {message}");
        assert!(message.contains("`abc` is not an int"), "got: {message}");
    }

    #[test]
    fn explicit_float_widens_int_cells() {
        let schema = [field("x", Some(TypeName::Float))];
        let table = ok(csv(&[&["1"], &["2.5"]]), Some(&schema));
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Float(F64::new(1.0).unwrap())],
                vec![Value::Float(F64::new(2.5).unwrap())],
            ]
        );
    }

    #[test]
    fn header_is_skipped_iff_it_equals_the_schema_field_names() {
        let schema = [
            field("parent", Some(TypeName::String)),
            field("child", Some(TypeName::String)),
        ];
        // Matching first row: a header, skipped.
        let table = ok(
            csv(&[&["parent", "child"], &["alice", "bob"]]),
            Some(&schema),
        );
        assert_eq!(table.rows.len(), 1);

        // Non-matching first row: data.
        let table = ok(csv(&[&["eve", "adam"], &["alice", "bob"]]), Some(&schema));
        assert_eq!(table.rows.len(), 2);
    }

    #[test]
    fn untyped_schema_fields_still_infer() {
        let schema = [field("n", None)];
        let table = ok(csv(&[&["1"], &["2"]]), Some(&schema));
        assert_eq!(table.rows, vec![vec![Value::Int(1)], vec![Value::Int(2)]]);
    }

    // --- Headers and field names ---

    #[test]
    fn header_names_become_fields() {
        let table = ok(csv(&[&["parent", "child"], &["alice", "bob"]]), None);
        assert_eq!(table.fields, ["parent", "child"]);
    }

    #[test]
    fn an_empty_file_needs_a_header_or_schema() {
        let errors = err(csv(&[]), None);
        assert!(errors[0].to_string().contains("empty"), "got: {errors:?}");
        // …but with an explicit schema an empty file is a legal empty table.
        let schema = [field("a", Some(TypeName::Int))];
        let table = ok(csv(&[]), Some(&schema));
        assert!(table.rows.is_empty());
        assert_eq!(table.fields, ["a"]);
    }

    #[test]
    fn a_self_describing_source_with_no_records_needs_a_schema() {
        let empty = || RawTable {
            columns: Some(Vec::new()),
            rows: Vec::new(),
        };
        let errors = err(empty(), None);
        assert!(
            errors[0].to_string().contains("no records"),
            "got: {errors:?}"
        );
        let schema = [field("a", Some(TypeName::Int)), field("b", None)];
        let table = ok(empty(), Some(&schema));
        assert!(table.rows.is_empty());
        assert_eq!(table.fields, ["a", "b"]);
    }

    #[test]
    fn a_header_only_file_is_a_legal_empty_table() {
        let table = ok(csv(&[&["a", "b"]]), None);
        assert!(table.rows.is_empty());
        assert_eq!(table.fields, ["a", "b"]);
    }

    #[test]
    fn illegal_header_names_suggest_an_explicit_schema() {
        for bad in ["First Name", "AGE", "import", "1st"] {
            let errors = err(csv(&[&[bad], &["x"]]), None);
            assert!(
                errors[0].to_string().contains("explicit"),
                "for {bad:?}, got: {errors:?}"
            );
        }
    }

    /// An integer that cannot be widened to a float exactly is a structured
    /// error, not a silent rounding. This only arises when the column *mixes*
    /// integers and floats — one float cell drags the whole column to `float`
    /// (§13) — and on the import path the large integers are typically
    /// identifiers, where rounding would corrupt every join on them.
    #[test]
    fn a_lossy_int_to_float_widening_is_rejected() {
        // 2^53 + 1: the first integer with no exact f64.
        let errors = err(csv(&[&["v"], &["9007199254740993"], &["2.5"]]), None);
        let message = errors[0].to_string();
        assert!(
            message.contains("9007199254740993") && message.contains("losing precision"),
            "got: {message}"
        );
    }

    /// 2^53 itself *is* exactly representable, so it widens. The check is
    /// exactness, not a blanket magnitude cutoff.
    #[test]
    fn an_exact_int_to_float_widening_is_allowed() {
        // Rows come back sorted, so compare the whole (small) table.
        let table = ok(csv(&[&["v"], &["9007199254740992"], &["2.5"]]), None);
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Float(F64::new(2.5).unwrap())],
                vec![Value::Float(F64::new(9007199254740992.0).unwrap())],
            ]
        );
    }

    /// Widening is only reachable through a mixed column: an all-integer column
    /// stays `int`, so large identifiers import losslessly.
    #[test]
    fn an_all_int_column_keeps_large_values_exactly() {
        let table = ok(csv(&[&["v"], &["9007199254740993"]]), None);
        assert_eq!(table.rows[0][0], Value::Int(9007199254740993));
    }

    #[test]
    fn duplicate_header_names_are_an_error() {
        let errors = err(csv(&[&["a", "a"], &["1", "2"]]), None);
        assert!(
            errors[0].to_string().contains("duplicate"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn ragged_rows_report_their_row_number() {
        let errors = err(csv(&[&["a", "b"], &["1", "2"], &["3"]]), None);
        let message = errors[0].to_string();
        assert!(message.contains("row 3"), "got: {message}");
        assert!(
            message.contains("1 column(s), expected 2"),
            "got: {message}"
        );
    }

    // --- Typed (self-describing) sources ---

    fn typed(columns: &[&str], rows: Vec<Vec<RawValue>>) -> RawTable {
        RawTable {
            columns: Some(columns.iter().map(|c| c.to_string()).collect()),
            rows,
        }
    }

    #[test]
    fn json_strings_are_never_reinferred() {
        let table = ok(
            typed(
                &["code"],
                vec![
                    vec![RawValue::Str("42".to_string())],
                    vec![RawValue::Str("true".to_string())],
                ],
            ),
            None,
        );
        assert_eq!(
            table.rows,
            vec![
                vec![Value::String("42".to_string())],
                vec![Value::String("true".to_string())],
            ]
        );
    }

    #[test]
    fn typed_int_float_mix_widens() {
        let table = ok(
            typed(
                &["x"],
                vec![vec![RawValue::Int(1)], vec![RawValue::Float(2.5)]],
            ),
            None,
        );
        assert_eq!(
            table.rows,
            vec![
                vec![Value::Float(F64::new(1.0).unwrap())],
                vec![Value::Float(F64::new(2.5).unwrap())],
            ]
        );
    }

    #[test]
    fn typed_string_number_mix_is_an_error_not_a_fallback() {
        let errors = err(
            typed(
                &["x"],
                vec![vec![RawValue::Int(1)], vec![RawValue::Str("x".to_string())]],
            ),
            None,
        );
        assert!(errors[0].to_string().contains("mixes"), "got: {errors:?}");
    }

    #[test]
    fn explicit_schema_binds_typed_sources_by_name_in_schema_order() {
        let schema = [
            field("b", Some(TypeName::Int)),
            field("a", Some(TypeName::Int)),
        ];
        let table = ok(
            typed(&["a", "b"], vec![vec![RawValue::Int(1), RawValue::Int(2)]]),
            Some(&schema),
        );
        assert_eq!(table.fields, ["b", "a"]);
        assert_eq!(table.rows, vec![vec![Value::Int(2), Value::Int(1)]]);
    }

    #[test]
    fn schema_source_field_set_mismatch_names_both_sides() {
        let schema = [field("z", Some(TypeName::Int))];
        let errors = err(typed(&["a"], vec![vec![RawValue::Int(1)]]), Some(&schema));
        let all = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("names `z`"), "got: {all}");
        assert!(all.contains("field `a`"), "got: {all}");
    }

    // --- Set semantics ---

    #[test]
    fn rows_are_sorted_and_deduplicated() {
        let table = ok(csv(&[&["n"], &["2"], &["1"], &["2"]]), None);
        assert_eq!(table.rows, vec![vec![Value::Int(1)], vec![Value::Int(2)]]);
    }
}
