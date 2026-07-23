//! External fact sources (`spec.md` §13).
//!
//! A data import (`import "<path>" as rel.`) is loaded here, before lowering:
//! the backend reads the source into an untyped-or-typed [`RawTable`], and the
//! shared [`finalize`] layer applies every §13 rule — header handling, the
//! literal-grammar type inference, explicit-schema coercion, field-name
//! validation, and set-semantics dedup — exactly once, so no backend can
//! diverge on semantics.
//!
//! The only backend is DuckDB (`sources/duckdb.rs`, the default-on `duckdb`
//! cargo feature; §17 2026-07-23): transport and dialect parsing only. CSV is
//! read `all_varchar`, keeping the engine the sole typing authority. Without
//! the feature, loading any data import is a structured [`Error::Source`]
//! naming the feature.
//!
//! Loaded tables are eagerly materialized and become ordinary base facts
//! (`ir::Program.facts`): the evaluator never touches a source, and imported
//! tuples anchor provenance leaves (§11).

// Without the reader feature, `RawValue` and parts of `finalize` are defined
// but never constructed — that is the escape-hatch build, not dead design.
#[cfg_attr(not(feature = "duckdb"), allow(dead_code))]
mod table;

#[cfg(feature = "duckdb")]
mod duckdb;

pub use table::LoadedTable;
pub(crate) use table::{RawTable, finalize};

use crate::ast::FieldDecl;
use crate::error::Error;

/// A reader backend: turns a resolved path into a [`RawTable`]. I/O and
/// syntax only — typing belongs to [`finalize`].
pub(crate) trait FactSource {
    /// Short format label for error messages (`"csv"`, `"parquet"`, …).
    #[allow(dead_code)] // used by error paths as backends grow
    fn format(&self) -> &'static str;
    fn read(&self, path: &str) -> Result<RawTable, Error>;
}

/// Loads one data import end to end: dispatch on the path, read, then apply
/// the §13 `finalize` rules. `path` is already resolved (absolute, or a URL);
/// `table` is the reserved database selection; `schema` the explicit override.
pub fn load_table(
    path: &str,
    table: Option<&str>,
    schema: Option<&[FieldDecl]>,
) -> Result<LoadedTable, Vec<Error>> {
    if table.is_some() || has_database_extension(path) {
        return Err(vec![Error::Source(format!(
            "`{path}`: database imports are not yet implemented (the `table \"…\"` \
             syntax is reserved; spec §13)"
        ))]);
    }
    let backend = backend_for(path).map_err(|e| vec![e])?;
    let raw = backend.read(path).map_err(|e| vec![e])?;
    finalize(raw, schema, path)
}

/// Picks the reader for a path by extension/scheme (§13 format table).
fn backend_for(path: &str) -> Result<Box<dyn FactSource>, Error> {
    if is_url(path) {
        return Err(Error::Source(format!(
            "`{path}`: URL imports are not yet wired up (spec §13; local files only for now)"
        )));
    }
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("dl") => Err(Error::Source(format!(
            "`{path}`: a `.dl` file is a module import — drop the `as` clause: \
             `import \"{path}\".`"
        ))),
        Some("csv") => backend(duckdb_backend(DataFormat::Csv), path),
        Some("jsonl" | "ndjson") => backend(duckdb_backend(DataFormat::Jsonl), path),
        Some("parquet") => backend(duckdb_backend(DataFormat::Parquet), path),
        _ => Err(Error::Source(format!(
            "`{path}`: unsupported import format; supported: .csv, .jsonl/.ndjson, \
             .parquet (and http(s) URLs)"
        ))),
    }
}

/// The formats the DuckDB backend reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataFormat {
    Csv,
    Jsonl,
    Parquet,
}

fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

fn has_database_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".duckdb", ".db", ".sqlite", ".sqlite3"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

#[cfg(feature = "duckdb")]
fn duckdb_backend(format: DataFormat) -> Option<Box<dyn FactSource>> {
    Some(Box::new(duckdb::DuckDbSource::new(format)))
}

#[cfg(not(feature = "duckdb"))]
fn duckdb_backend(_format: DataFormat) -> Option<Box<dyn FactSource>> {
    None
}

fn backend(backend: Option<Box<dyn FactSource>>, path: &str) -> Result<Box<dyn FactSource>, Error> {
    backend.ok_or_else(|| {
        Error::Source(format!(
            "`{path}`: this build has no import support — imports need the `duckdb` \
             feature (on by default; rebuild without `--no-default-features`)"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_paths_and_table_clauses_are_reserved() {
        for (path, table) in [
            ("analytics.duckdb", None),
            ("things.sqlite", None),
            ("data.csv", Some("orders")),
        ] {
            let errors = load_table(path, table, None).expect_err("reserved");
            assert!(
                errors[0].to_string().contains("not yet implemented"),
                "got: {errors:?}"
            );
        }
    }

    #[test]
    fn url_imports_are_not_yet_wired() {
        let errors = load_table("https://example.com/data.csv", None, None).expect_err("url");
        assert!(
            errors[0].to_string().contains("URL imports"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn a_dl_path_in_data_position_suggests_the_module_form() {
        let errors = load_table("lib/family.dl", None, None).expect_err("module path");
        assert!(
            errors[0].to_string().contains("drop the `as` clause"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn unknown_extensions_list_the_supported_formats() {
        let errors = load_table("data.xlsx", None, None).expect_err("unsupported");
        assert!(
            errors[0].to_string().contains("unsupported import format"),
            "got: {errors:?}"
        );
    }
}
