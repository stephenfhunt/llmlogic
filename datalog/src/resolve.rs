//! Module resolution (`spec.md` §13): splices `import "lib.dl".` statements
//! in place, before lowering.
//!
//! Semantics (§17, 2026-07-23):
//! - **Splice-in-place, DFS pre-order**: a module import is replaced by the
//!   imported file's (recursively resolved) statements, preserving source
//!   order — textual-inclusion semantics, no namespacing.
//! - **Once-only by canonical path**: a file splices the first time it is
//!   reached; diamonds deduplicate and cycles terminate harmlessly (the
//!   visited mark is set *before* recursing). The root file is pre-seeded, so
//!   a cycle back to it is also harmless.
//! - **Per-file relative resolution**: each file's imports — module *and*
//!   data — resolve against that file's own directory. The resolver rewrites
//!   every data import's path accordingly, so the source layer is
//!   path-context-free. URL paths (a `://` scheme) pass through untouched.
//! - **Libraries define, they don't ask**: a query in an imported file is a
//!   structured error naming the file.
//!
//! Cross-file error attribution: spans stay per-file byte offsets; the
//! `origins`/`files` side tables are the designed hook for §12's future
//! file:offset rendering (not built out yet).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ast::{ImportKind, Program, Statement, StatementKind};

/// The reserved virtual path prefix for builtin modules (§13).
const STD_PREFIX: &str = "std/";

use crate::error::Error;
use crate::parser::parse;

/// The root program with module imports spliced away: only data imports,
/// declares, clauses, and (root-file) queries remain.
#[derive(Debug)]
pub struct ResolvedProgram {
    pub program: Program,
    /// Per-statement origin: an index into `files`. Parallel to
    /// `program.statements`.
    pub origins: Vec<usize>,
    /// `files[0]` is the root (its path, or `<stdin>` when there is none);
    /// the rest are imported modules in first-visit order.
    pub files: Vec<String>,
}

/// Splices every module import of `root`, resolving paths relative to
/// `source_path`'s directory (or the working directory when `None` —
/// stdin/`-q`-only programs).
pub fn resolve_modules(
    root: Program,
    source_path: Option<&Path>,
) -> Result<ResolvedProgram, Vec<Error>> {
    let root_dir = source_path.and_then(parent_dir);
    let mut resolver = Resolver {
        files: vec![
            source_path
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<stdin>".to_string()),
        ],
        visited: HashSet::new(),
        statements: Vec::new(),
        origins: Vec::new(),
        errors: Vec::new(),
    };
    // Seed the root so a cycle back to it splices nothing.
    if let Some(path) = source_path
        && let Ok(canonical) = path.canonicalize()
    {
        resolver.visited.insert(canonical);
    }
    resolver.splice(root, root_dir.as_deref(), 0, true);

    if resolver.errors.is_empty() {
        Ok(ResolvedProgram {
            program: Program {
                statements: resolver.statements,
            },
            origins: resolver.origins,
            files: resolver.files,
        })
    } else {
        Err(resolver.errors)
    }
}

struct Resolver {
    files: Vec<String>,
    visited: HashSet<PathBuf>,
    statements: Vec<Statement>,
    origins: Vec<usize>,
    errors: Vec<Error>,
}

impl Resolver {
    fn splice(&mut self, program: Program, dir: Option<&Path>, file: usize, is_root: bool) {
        for mut statement in program.statements {
            match &mut statement.kind {
                StatementKind::Import(import) if import.kind == ImportKind::Module => {
                    // `std/` is a reserved virtual prefix, resolved before the
                    // filesystem is consulted (§13): a `std` module is not a
                    // file, and a real `./std/` directory must not be able to
                    // shadow one silently.
                    if let Some(module) = import.path.strip_prefix(STD_PREFIX) {
                        match self.classify_std(module, &import.path, dir, file) {
                            Some(module) => {
                                import.kind = ImportKind::Std { module };
                                self.push(statement, file);
                            }
                            None => continue,
                        }
                        continue;
                    }
                    self.splice_module(&import.path, dir, file);
                }
                StatementKind::Import(import) => {
                    // A data import: rewrite its path to this file's context
                    // so the source layer never needs to know where the
                    // statement came from.
                    import.path = resolve_path(&import.path, dir);
                    self.push(statement, file);
                }
                StatementKind::Query(_) if !is_root => {
                    self.errors.push(Error::source(format!(
                        "in `{}`: query statements are not allowed in imported modules \
                         (libraries define relations; the importing program asks)",
                        self.files[file]
                    )));
                }
                _ => self.push(statement, file),
            }
        }
    }

    /// Validates a `std/…` import, or records why it is not one.
    ///
    /// Two refusals, both loud: an unknown module lists the ones that exist,
    /// and a real `./std/…` file that the prefix would shadow is an error
    /// rather than a silent preference for either reading.
    fn classify_std(
        &mut self,
        module: &str,
        path: &str,
        dir: Option<&Path>,
        file: usize,
    ) -> Option<String> {
        let context = format!("in `{}`", self.files[file]);
        if crate::stdlib::module(module).is_none() {
            self.errors.push(Error::source(format!(
                "{context}: there is no `{path}` module; `std/` provides: {}",
                crate::stdlib::module_names()
                    .iter()
                    .map(|name| format!("std/{name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
            return None;
        }
        let shadow = PathBuf::from(resolve_path(path, dir));
        if shadow.exists() {
            self.errors.push(Error::source(format!(
                "{context}: `{path}` names the reserved `std/` module prefix, but \
                 `{}` also exists on disk; `std/` always wins, so rename the file \
                 to make the program mean one thing",
                shadow.display()
            )));
            return None;
        }
        Some(module.to_string())
    }

    fn splice_module(&mut self, path: &str, dir: Option<&Path>, file: usize) {
        let in_file = |name: &str| format!("in `{name}`");
        let context = in_file(&self.files[file]);

        if is_url(path) {
            self.errors.push(Error::source(format!(
                "{context}: module imports are local files only (`{path}` is a URL); \
                 URLs import data, with `as`"
            )));
            return;
        }
        if Path::new(path).extension().and_then(|e| e.to_str()) != Some("dl") {
            self.errors.push(Error::source(format!(
                "{context}: `import \"{path}\".` without `as` is a module import, which \
                 takes a `.dl` file; importing data requires `as`: \
                 `import \"{path}\" as <relation>.`"
            )));
            return;
        }

        let resolved = PathBuf::from(resolve_path(path, dir));
        let canonical = match resolved.canonicalize() {
            Ok(canonical) => canonical,
            Err(e) => {
                self.errors.push(Error::source(format!(
                    "{context}: cannot read module `{}`: {e}",
                    resolved.display()
                )));
                return;
            }
        };
        // Once-only inclusion: mark before recursing, so diamonds splice a
        // single copy and cycles terminate.
        if !self.visited.insert(canonical) {
            return;
        }

        let source = match std::fs::read_to_string(&resolved) {
            Ok(source) => source,
            Err(e) => {
                self.errors.push(Error::source(format!(
                    "{context}: cannot read module `{}`: {e}",
                    resolved.display()
                )));
                return;
            }
        };
        let module_label = resolved.display().to_string();
        let module_index = self.files.len();
        self.files.push(module_label.clone());
        match parse(&source) {
            Ok(module) => {
                let module_dir = parent_dir(&resolved);
                self.splice(module, module_dir.as_deref(), module_index, false);
            }
            Err(errors) => {
                let wrap = in_file(&module_label);
                self.errors
                    .extend(errors.into_iter().map(|e| prefix_error(e, &wrap)));
            }
        }
    }

    fn push(&mut self, statement: Statement, file: usize) {
        self.statements.push(statement);
        self.origins.push(file);
    }
}

/// Resolves a data/module path against the importing file's directory.
/// Absolute paths and URLs pass through; with no directory (stdin), relative
/// paths stay relative and resolve against the working directory.
fn resolve_path(path: &str, dir: Option<&Path>) -> String {
    if is_url(path) || Path::new(path).is_absolute() {
        return path.to_string();
    }
    match dir {
        Some(dir) => dir.join(path).display().to_string(),
        None => path.to_string(),
    }
}

/// A file's directory for resolving its imports; `.` when the path has no
/// parent component (a bare file name).
fn parent_dir(path: &Path) -> Option<PathBuf> {
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Some(PathBuf::from(".")),
        Some(parent) => Some(parent.to_path_buf()),
        None => None,
    }
}

fn is_url(path: &str) -> bool {
    path.contains("://")
}

/// Prefixes an error's message with the module it came from, keeping its kind,
/// span, position and suggestion intact.
fn prefix_error(mut error: Error, context: &str) -> Error {
    error.message = format!("{context}: {}", error.message);
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh scratch directory per call (no `tempfile` dep).
    fn scratch_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "datalog-resolve-tests-{}-{}",
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

    fn resolve_file(path: &Path) -> ResolvedProgram {
        let source = std::fs::read_to_string(path).expect("read root");
        let program = parse(&source).expect("root parses");
        resolve_modules(program, Some(path)).expect("resolves")
    }

    fn resolve_file_err(path: &Path) -> Vec<Error> {
        let source = std::fs::read_to_string(path).expect("read root");
        let program = parse(&source).expect("root parses");
        resolve_modules(program, Some(path)).expect_err("resolution fails")
    }

    fn statement_count(resolved: &ResolvedProgram) -> usize {
        resolved.program.statements.len()
    }

    #[test]
    fn a_module_import_splices_in_place() {
        let dir = scratch_dir();
        write(&dir, "lib.dl", "parent(\"alice\", \"bob\").\n");
        let root = write(
            &dir,
            "main.dl",
            "import \"lib.dl\".\nancestor(X, Y) :- parent(X, Y).\n",
        );
        let resolved = resolve_file(&root);
        assert_eq!(statement_count(&resolved), 2);
        // The spliced fact takes the import's position (pre-order).
        assert!(matches!(
            resolved.program.statements[0].kind,
            StatementKind::Clause(_)
        ));
        assert_eq!(resolved.origins, vec![1, 0]);
        assert_eq!(resolved.files.len(), 2);
    }

    #[test]
    fn a_diamond_splices_the_shared_module_once() {
        let dir = scratch_dir();
        write(&dir, "c.dl", "base(1).\n");
        write(&dir, "a.dl", "import \"c.dl\".\nleft(X) :- base(X).\n");
        write(&dir, "b.dl", "import \"c.dl\".\nright(X) :- base(X).\n");
        let root = write(&dir, "main.dl", "import \"a.dl\".\nimport \"b.dl\".\n");
        let resolved = resolve_file(&root);
        // c's fact once, a's rule, b's rule.
        assert_eq!(statement_count(&resolved), 3);
    }

    #[test]
    fn an_import_cycle_terminates_and_takes_the_union() {
        let dir = scratch_dir();
        write(&dir, "x.dl", "import \"y.dl\".\nx_fact(1).\n");
        write(&dir, "y.dl", "import \"x.dl\".\ny_fact(2).\n");
        let root = write(&dir, "main.dl", "import \"x.dl\".\n");
        let resolved = resolve_file(&root);
        assert_eq!(statement_count(&resolved), 2);
    }

    #[test]
    fn a_cycle_back_to_the_root_is_harmless() {
        let dir = scratch_dir();
        write(&dir, "lib.dl", "import \"main.dl\".\nlib_fact(1).\n");
        let root = write(&dir, "main.dl", "import \"lib.dl\".\nmain_fact(2).\n");
        let resolved = resolve_file(&root);
        assert_eq!(statement_count(&resolved), 2);
    }

    #[test]
    fn module_paths_resolve_relative_to_the_importing_file() {
        let dir = scratch_dir();
        std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
        write(&dir, "sub/inner.dl", "inner(1).\n");
        // `sub/mid.dl` imports its sibling by bare name.
        write(&dir, "sub/mid.dl", "import \"inner.dl\".\nmid(2).\n");
        let root = write(&dir, "main.dl", "import \"sub/mid.dl\".\n");
        let resolved = resolve_file(&root);
        assert_eq!(statement_count(&resolved), 2);
    }

    #[test]
    fn data_import_paths_are_rewritten_to_the_importing_files_dir() {
        let dir = scratch_dir();
        write(&dir, "lib.dl", "import \"data/facts.csv\" as fact.\n");
        let root = write(&dir, "main.dl", "import \"lib.dl\".\n");
        let resolved = resolve_file(&root);
        let StatementKind::Import(import) = &resolved.program.statements[0].kind else {
            panic!("expected the data import to survive");
        };
        assert_eq!(
            PathBuf::from(&import.path),
            dir.join("data/facts.csv"),
            "resolved against lib.dl's directory"
        );
    }

    #[test]
    fn url_data_imports_pass_through_untouched() {
        let program = parse("import \"https://example.com/d.csv\" as d.\n").expect("parses");
        let resolved = resolve_modules(program, None).expect("resolves");
        let StatementKind::Import(import) = &resolved.program.statements[0].kind else {
            panic!("expected an import");
        };
        assert_eq!(import.path, "https://example.com/d.csv");
    }

    #[test]
    fn a_query_in_an_imported_module_is_an_error_naming_the_file() {
        let dir = scratch_dir();
        write(&dir, "lib.dl", "p(1).\n?- p(X).\n");
        let root = write(&dir, "main.dl", "import \"lib.dl\".\n?- p(X).\n");
        let errors = resolve_file_err(&root);
        let message = errors[0].to_string();
        assert!(message.contains("lib.dl"), "got: {message}");
        assert!(
            message.contains("not allowed in imported modules"),
            "got: {message}"
        );
    }

    #[test]
    fn a_root_query_is_still_legal() {
        let program = parse("p(1).\n?- p(X).\n").expect("parses");
        let resolved = resolve_modules(program, None).expect("resolves");
        assert_eq!(resolved.program.statements.len(), 2);
    }

    #[test]
    fn a_module_import_of_a_data_extension_suggests_as() {
        let program = parse("import \"x.csv\".\n").expect("parses");
        let errors = resolve_modules(program, None).expect_err("data ext without as");
        assert!(
            errors[0]
                .to_string()
                .contains("importing data requires `as`"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn a_url_module_import_is_an_error() {
        let program = parse("import \"https://example.com/lib.dl\".\n").expect("parses");
        let errors = resolve_modules(program, None).expect_err("url module");
        assert!(
            errors[0].to_string().contains("local files only"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn a_missing_module_names_the_importing_file() {
        let dir = scratch_dir();
        let root = write(&dir, "main.dl", "import \"nope.dl\".\n");
        let errors = resolve_file_err(&root);
        let message = errors[0].to_string();
        assert!(message.contains("main.dl"), "got: {message}");
        assert!(message.contains("cannot read module"), "got: {message}");
    }

    #[test]
    fn parse_errors_in_a_module_are_wrapped_with_its_file() {
        let dir = scratch_dir();
        write(&dir, "broken.dl", "p(1\n");
        let root = write(&dir, "main.dl", "import \"broken.dl\".\n");
        let errors = resolve_file_err(&root);
        assert!(
            errors.iter().all(|e| e.to_string().contains("broken.dl")),
            "got: {errors:?}"
        );
    }
}
