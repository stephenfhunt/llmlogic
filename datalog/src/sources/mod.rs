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
//! A loaded table is materialized whole and becomes ordinary base facts
//! (`ir::Program.facts`): the evaluator never touches a source, and imported
//! tuples anchor provenance leaves (§11). *Which* tables are loaded is the
//! caller's: [`load_imports_where`] reads only the ones a program's goals reach
//! and checks the rest without reading them (`api.rs`, §17 2026-09-12).

// Without the reader feature, `RawValue` and parts of `finalize` are defined
// but never constructed — that is the escape-hatch build, not dead design.
#[cfg_attr(not(feature = "duckdb"), allow(dead_code))]
mod table;

#[cfg(feature = "duckdb")]
mod duckdb;

pub use table::LoadedTable;
pub(crate) use table::{RawTable, finalize};

use crate::ast::{FieldDecl, ImportKind, Program, StatementKind};
use crate::error::{Error, ErrorCode};

/// A reader backend: turns a resolved path into a [`RawTable`]. I/O and
/// syntax only — typing belongs to [`finalize`].
pub(crate) trait FactSource {
    /// Short format label for error messages (`"csv"`, `"parquet"`, …).
    #[allow(dead_code)] // used by error paths as backends grow
    fn format(&self) -> &'static str;
    fn read(&self, path: &str) -> Result<RawTable, Error>;
}

/// Loads every data import of a (module-resolved) program, in source order —
/// the alignment [`crate::lower::lower_with_sources`] expects. Collects errors
/// across all imports so one run surfaces every source problem (§12).
pub fn load_imports(program: &Program) -> Result<Vec<LoadedTable>, Vec<Error>> {
    let tables = load_imports_where(program, &|_| true)?;
    Ok(tables.into_iter().flatten().collect())
}

/// [`load_imports`], reading only the data imports `wanted` selects — by
/// position among the program's data imports, the alignment
/// [`crate::ir::Program::imports`] has — and `None` in place of the rest.
///
/// A skipped import is still **checked short of reading it**: a reserved
/// database path, an unsupported format, a build without the reader, and a local
/// file that does not exist all fail exactly as they would if it were read. So a
/// typo in an import path is an error whether or not a goal reaches the relation;
/// what skipping saves is the rows (§13, §17 2026-09-12). A URL is not probed.
pub fn load_imports_where(
    program: &Program,
    wanted: &dyn Fn(usize) -> bool,
) -> Result<Vec<Option<LoadedTable>>, Vec<Error>> {
    let mut tables = Vec::new();
    let mut errors = Vec::new();
    for statement in &program.statements {
        let StatementKind::Import(import) = &statement.kind else {
            continue;
        };
        let ImportKind::Data { table, schema, .. } = &import.kind else {
            continue; // module imports are already spliced away
        };
        let table = table.as_ref().map(|(name, _)| name.as_str());
        let loaded = if wanted(tables.len()) {
            load_table(&import.path, table, schema.as_deref()).map(Some)
        } else {
            check_table(&import.path, table).map(|()| None)
        };
        match loaded {
            Ok(loaded) => tables.push(loaded),
            // The import statement's path is the place, for every way loading
            // it can fail — a missing file, an unreadable one, a schema that
            // does not match what arrived. Stamping here rather than at each
            // raising site is what gives the whole §13 surface a position:
            // `sources/` reports from deep inside the reader, and this is the
            // frame that still knows which import is being loaded (§12).
            Err(source_errors) => errors.extend(
                source_errors
                    .into_iter()
                    .map(|error| error.or_span(import.path_span)),
            ),
        }
    }
    if errors.is_empty() {
        Ok(tables)
    } else {
        Err(errors)
    }
}

/// Loads one data import end to end: dispatch on the path, read, then apply
/// the §13 `finalize` rules. `path` is already resolved (absolute, or a URL);
/// `table` is the reserved database selection; `schema` the explicit override.
pub fn load_table(
    path: &str,
    table: Option<&str>,
    schema: Option<&[FieldDecl]>,
) -> Result<LoadedTable, Vec<Error>> {
    let backend = open_table(path, table)?;
    let raw = backend.read(path).map_err(|e| vec![e])?;
    finalize(raw, schema, path)
}

/// Everything [`load_table`] would refuse before reading a row: the reserved
/// database forms, the format dispatch (which is also where a build without the
/// reader fails), and a local file that is not there. Reads nothing.
fn check_table(path: &str, table: Option<&str>) -> Result<(), Vec<Error>> {
    open_table(path, table)?;
    if !is_url(path) && std::fs::metadata(path).is_err() {
        return Err(vec![file_not_found(path)]);
    }
    Ok(())
}

/// The reader for one import, or the reason there is none.
fn open_table(path: &str, table: Option<&str>) -> Result<Box<dyn FactSource>, Vec<Error>> {
    if table.is_some() || has_database_extension(path) {
        return Err(vec![Error::new(
            ErrorCode::UnsupportedFormat,
            format!(
                "`{path}`: database imports are not yet implemented (the `table \"…\"` \
             syntax is reserved; spec §13)"
            ),
        )]);
    }
    backend_for(path).map_err(|e| vec![e])
}

/// A local import path that does not exist — one message for a read import and
/// a skipped one alike.
#[cfg_attr(not(feature = "duckdb"), allow(dead_code))]
pub(crate) fn file_not_found(path: &str) -> Error {
    Error::new(ErrorCode::FileNotFound, format!("`{path}`: file not found"))
}

/// Picks the reader for a path by extension/scheme (§13 format table). URLs
/// dispatch on the path portion's extension (query string stripped); the
/// DuckDB backend fetches the bytes via httpfs and then reads them through the
/// identical local path (one dialect + typing rulebook — §13).
fn backend_for(path: &str) -> Result<Box<dyn FactSource>, Error> {
    let for_extension = if is_url(path) {
        url_path_part(path)
    } else {
        path
    };
    let extension = std::path::Path::new(for_extension)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("dl") => Err(Error::new(
            ErrorCode::UnsupportedFormat,
            format!(
                "`{path}`: a `.dl` file is a module import — drop the `as` clause: \
             `import \"{path}\".`"
            ),
        )),
        Some("csv") => backend(duckdb_backend(DataFormat::Csv), path),
        Some("jsonl" | "ndjson") => backend(duckdb_backend(DataFormat::Jsonl), path),
        Some("parquet") => backend(duckdb_backend(DataFormat::Parquet), path),
        _ => Err(Error::new(
            ErrorCode::UnsupportedFormat,
            format!(
                "`{path}`: unsupported import format; supported: .csv, .jsonl/.ndjson, \
             .parquet (and http(s) URLs)"
            ),
        )),
    }
}

/// The path portion of a URL — everything before `?` (query) or `#`
/// (fragment) — so the extension dispatch ignores query strings.
fn url_path_part(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    &url[..end]
}

/// The formats the DuckDB backend reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataFormat {
    Csv,
    Jsonl,
    Parquet,
}

pub(crate) fn is_url(path: &str) -> bool {
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
        Error::new(
            ErrorCode::UnsupportedFormat,
            format!(
                "`{path}`: this build has no import support — imports need the `duckdb` \
             feature (on by default; rebuild without `--no-default-features`)"
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A skipped import reads no rows but is refused for everything a read would
    /// refuse before its first row — so skipping cannot hide a broken path, and
    /// the message is the same one a read gives.
    #[test]
    fn a_skipped_import_is_checked_but_not_read() {
        let dir = std::env::temp_dir().join(format!("datalog-skip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let present = dir.join("present.jsonl");
        std::fs::write(&present, "not json at all\n").expect("write");
        let missing = dir.join("missing.jsonl");
        let program = |path: &std::path::Path| {
            crate::parser::parse(&format!("import \"{}\" as t(a: int).\n", path.display()))
                .expect("parses")
        };

        // Skipped, a file whose rows would not parse is never opened.
        let tables = load_imports_where(&program(&present), &|_| false).expect("not read");
        assert_eq!(tables.len(), 1);
        assert!(tables[0].is_none());

        // Skipped or read, a missing file is the same error.
        let skipped = load_imports_where(&program(&missing), &|_| false).expect_err("missing");
        assert!(
            skipped[0].to_string().contains("file not found"),
            "{skipped:?}"
        );
        #[cfg(feature = "duckdb")]
        {
            let read = load_imports_where(&program(&missing), &|_| true).expect_err("missing");
            assert_eq!(skipped[0].to_string(), read[0].to_string());
        }

        // An unsupported format is refused without being read.
        let odd = crate::parser::parse("import \"data.xlsx\" as t(a: int).\n").expect("parses");
        let errors = load_imports_where(&odd, &|_| false).expect_err("unsupported");
        assert!(
            errors[0].to_string().contains("unsupported import format"),
            "{errors:?}"
        );
    }

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
    fn a_url_dispatches_on_its_path_extension_ignoring_the_query_string() {
        // The path portion decides the format; a query string is ignored.
        assert_eq!(
            url_path_part("https://h.com/a.csv?token=x"),
            "https://h.com/a.csv"
        );
        assert_eq!(
            url_path_part("https://h.com/a.parquet#frag"),
            "https://h.com/a.parquet"
        );
    }

    #[test]
    fn a_url_with_an_unsupported_extension_is_rejected_before_any_network() {
        let errors =
            load_table("https://example.com/data.xlsx", None, None).expect_err("unsupported url");
        assert!(
            errors[0].to_string().contains("unsupported import format"),
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
