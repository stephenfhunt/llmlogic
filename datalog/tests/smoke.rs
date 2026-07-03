//! Integration smoke test: exercises the library's public surface from outside the
//! crate. Expands into real end-to-end program/query tests as the engine lands.

#[test]
fn library_reports_version() {
    let v = datalog::version();
    assert!(!v.is_empty(), "version string should not be empty");
    // Matches the package version compiled into the crate.
    assert_eq!(v, env!("CARGO_PKG_VERSION"));
}
