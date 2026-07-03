//! Thin CLI/REPL binary for the `datalog` engine.
//!
//! For now this is a stub that prints a banner and exits. As the spec matures it
//! will grow argument parsing, a REPL, program loading, and query execution — all
//! delegating to the [`datalog`] library crate.

fn main() {
    println!(
        "datalog {} — LLM-targeted Datalog engine",
        datalog::version()
    );
    println!("(scaffold: REPL and query execution not yet implemented — see spec.md)");
}
