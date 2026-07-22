//! Thin CLI for the `datalog` engine (`spec.md` §14, minimal Phase-D form).
//!
//! Contract:
//! - One argument: a program file, or `-` to read the program from stdin.
//! - On success, each query's answers print to **stdout** as canonical Datalog
//!   facts (so output is valid input); a program with no queries prints
//!   nothing. Exit code **0**.
//! - On any program error (lex / parse / lowering / type / evaluation), the
//!   structured errors print to **stderr**, one per line. Exit code **1**.
//! - On a usage problem (wrong arguments, unreadable file), a usage message
//!   prints to stderr. Exit code **2**.
//!
//! The full agent CLI (`-q`, `--format json`, the skill definition) is roadmap
//! step 6; this binary stays deliberately thin and delegates to
//! [`datalog::run`].

use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [source_arg] = args.as_slice() else {
        eprintln!("usage: datalog <program-file | ->");
        eprintln!("  reads a Datalog program, evaluates it, and prints query answers as facts");
        return ExitCode::from(2);
    };

    let source = match read_source(source_arg) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("datalog: {message}");
            return ExitCode::from(2);
        }
    };

    match datalog::run(&source) {
        Ok(result) => {
            print!("{}", result.output());
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}

/// Reads the program from a file, or from stdin when the argument is `-`.
fn read_source(arg: &str) -> Result<String, String> {
    if arg == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(arg).map_err(|e| format!("cannot read `{arg}`: {e}"))
    }
}
