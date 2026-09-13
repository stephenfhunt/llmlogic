//! Thin CLI for the `datalog` engine (`spec.md` §14).
//!
//! Contract:
//! - Usage: `datalog [<program-file> | -] [-q <query>]…`. The positional source
//!   is a program file, or `-` to read the program from stdin; it is optional
//!   (with only `-q` flags the base program is empty). Each `-q` appends a
//!   one-shot query: a bare atom / comma-body becomes `?- <arg>.`, and a
//!   define-and-select rule (`head :- body`) appends the rule plus a synthesized
//!   `?- <head>.`. Multiple `-q` apply in CLI order.
//! - Each query's answers print to **stdout** as canonical Datalog facts (so
//!   output is valid input). Any non-fatal warnings (e.g. a
//!   referenced-but-undefined predicate) print to **stderr**, keeping stdout a
//!   clean fact stream, and never change the exit code. Warnings known before
//!   evaluation print *before* it runs, so a program the termination lint flags
//!   (§10) is announced even when its fixpoint never arrives.
//! - **The exit code answers the question** (§14, grep's vocabulary): **0** when
//!   at least one query printed a row — or when the program had no queries to
//!   ask, since nothing was asked; **1** when every query ran and none produced
//!   an answer; **2** when the run did not answer at all, which covers both a
//!   usage problem (bad arguments, unreadable file) and a program error (lex /
//!   parse / lowering / type / evaluation), each printed to stderr.
//! - The vocabulary is a **range**: `0` and `1` are answers, `≥ 2` means the run
//!   did not answer. A caller branches on that boundary, so a later code can
//!   refine `2` or number §15's withholding without reinterpreting either.
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
            return ExitCode::from(DID_NOT_ANSWER);
        }
    };

    let base = match cli.source.as_deref() {
        Some(arg) => match read_source(arg) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("datalog: {message}");
                return ExitCode::from(DID_NOT_ANSWER);
            }
        },
        None => String::new(),
    };

    // The program file's own path threads §13 relative-path resolution;
    // stdin (`-`) and `-q`-only programs resolve against the working
    // directory.
    let source_path = cli
        .source
        .as_deref()
        .filter(|arg| *arg != "-")
        .map(std::path::Path::new);

    // Static warnings print as they are found, before evaluation — a program the
    // termination lint flags (§10) may never reach a fixpoint, and a warning
    // withheld until then is a warning nobody ever reads. `reported` then keeps
    // them from printing twice, since `RunResult::warnings` carries them too.
    let mut reported = 0usize;
    let result =
        datalog::run_with_queries_at_reporting(&base, source_path, &cli.queries, &mut |warning| {
            eprintln!("{warning}");
            reported += 1;
        });

    match result {
        Ok(result) => {
            // Line by line into a buffer, not joined into one string first: the
            // answer is already held once, in `result`.
            let mut out = std::io::BufWriter::new(std::io::stdout().lock());
            if let Err(error) = result
                .write_output(&mut out)
                .and_then(|()| std::io::Write::flush(&mut out))
            {
                eprintln!("error: could not write the answers: {error}");
                return ExitCode::from(DID_NOT_ANSWER);
            }
            drop(out);
            // Warnings go to stderr so the stdout fact stream stays valid Datalog
            // input; they never change the exit code, which answers whether the
            // run produced rows and not whether it was happy about them (§14).
            for warning in result.warnings.iter().skip(reported) {
                eprintln!("{warning}");
            }
            // A program with no queries asked nothing, so "no rows" is not an
            // answer to anything and the run reports success (§14) — which is
            // what keeps `datalog p.dl` usable as a plain load-and-run check.
            //
            // **Only `answers` is read, and that is the ruling, not an
            // accident** (§17, 2026-08-21): an explanation is commentary, not a
            // row, so appending `?why` to `datalog roster.dl -q '…' && deploy`
            // cannot change which way the pipeline branches. It is the
            // exit-code half of E5's comment-stripping guard, and a run whose
            // only goals are explanations exits 0 for the reason above.
            if result.answers.is_empty() || result.answers.iter().any(|rows| !rows.is_empty()) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(NO_ROWS)
            }
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            ExitCode::from(DID_NOT_ANSWER)
        }
    }
}

/// Every query ran and none produced an answer (§14). Still an *answer* — the
/// run completed and the question's answer was "none".
const NO_ROWS: u8 = 1;

/// The run did not answer: a usage problem or a program error (§14). The
/// vocabulary is a range, so `≥ 2` is the boundary a caller branches on and a
/// later code may refine this one without reinterpreting it.
const DID_NOT_ANSWER: u8 = 2;

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
