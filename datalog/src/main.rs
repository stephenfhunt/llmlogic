//! Thin CLI for the `datalog` engine (`spec.md` §14).
//!
//! Contract:
//! - Usage: `datalog [<program-file> | -] [-q <query>]…`. The positional source
//!   is a program file, or `-` to read the program from stdin; it is optional
//!   (with only `-q` flags the base program is empty). Each `-q` appends a
//!   one-shot query: a bare atom / comma-body becomes `?- <arg>.`, and a
//!   define-and-select rule (`head :- body`) appends the rule plus a synthesized
//!   `?- <head>.`. Multiple `-q` apply in CLI order.
//! - On success, each query's answers print to **stdout** as canonical Datalog
//!   facts (so output is valid input); a program with no queries prints nothing.
//!   Exit code **0**.
//! - On any program error (lex / parse / lowering / type / evaluation), the
//!   structured errors print to **stderr**, one per line. Exit code **1**.
//! - On a usage problem (bad arguments, unreadable file), a usage message prints
//!   to stderr. Exit code **2**.
//!
//! The binary stays deliberately thin: argument parsing lives here, everything
//! else delegates to [`datalog::run_with_queries`].

use std::io::Read;
use std::process::ExitCode;

/// Parsed command line: an optional program source and the ordered `-q` queries.
struct Cli {
    /// The positional source argument (`file` or `-`), if given.
    source: Option<String>,
    /// The `-q` query arguments, in CLI order.
    queries: Vec<String>,
}

const USAGE: &str = "\
usage: datalog [<program-file> | -] [-q <query>]…
  Evaluates a Datalog program and prints query answers as canonical facts.
  The source is a file, or `-` for stdin, or omitted for an empty base program.
  Each -q appends a one-shot query: a bare atom (or comma-body) is answered
  directly; a `head :- body` rule is defined and its head queried.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("datalog: {message}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let base = match cli.source.as_deref() {
        Some(arg) => match read_source(arg) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("datalog: {message}");
                return ExitCode::from(2);
            }
        },
        None => String::new(),
    };

    match datalog::run_with_queries(&base, &cli.queries) {
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

/// Parses the argument vector into a [`Cli`]: repeated `-q <value>` pairs and at
/// most one positional source. Rejects unknown flags, a second positional
/// argument, and a `-q` with no value.
fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut source: Option<String> = None;
    let mut queries: Vec<String> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-q" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "`-q` requires a query argument".to_string())?;
                queries.push(value.clone());
            }
            // `-` is the stdin source, not a flag; any other `-`-prefixed token
            // is an unknown flag.
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown flag `{flag}`"));
            }
            positional => {
                if source.is_some() {
                    return Err(format!("unexpected extra argument `{positional}`"));
                }
                source = Some(positional.to_string());
            }
        }
    }
    // A completely empty invocation (no source, no queries) is a user who wants
    // help, not a request to run the empty program.
    if source.is_none() && queries.is_empty() {
        return Err("no program given".to_string());
    }
    Ok(Cli { source, queries })
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
