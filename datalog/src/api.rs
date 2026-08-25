//! Programmatic / agent API (`spec.md` §14).
//!
//! [`run`] is the first production entry path: `parse → lower → typecheck →
//! eval`, then each query answered in canonical Datalog. It is the in-process
//! form of what the `datalog` binary does; integration and system tests drive
//! it directly.
//!
//! ## Query output shape (spec §14)
//!
//! Answers print as **valid Datalog facts** so output composes as input (the
//! Datalog-in/Datalog-out closure, `testing.md` D1):
//!
//! - A query whose positive atoms account for **every** answer variable
//!   re-emits those atoms with the bindings substituted —
//!   `?- ancestor("alice", Who).` yields `ancestor("alice", "bob").` … — which
//!   covers a single atom carrying the whole projection (with any number of
//!   non-binding literals beside it) and a **ground** conjunction, whose atoms
//!   print once if the body holds.
//! - A body with **no answer variables** and nothing substitutable to show
//!   answers `holds(true).` when it holds. §5's ban on 0-arity atoms is why the
//!   yes carries an argument.
//! - Any **other** body emits synthesized `answer/N` facts over the query's
//!   answer variables.
//!
//! **Silence means an empty answer, and for a body with no answer variables it
//! means no** — the ground form says yes by printing its atoms and no by
//! printing nothing, as `?- p("a").` always has (§17, 2026-08-17).
//!
//! A computed argument reaches the **first** case: a query constant-folds a
//! *ground* compound argument rather than hoisting it
//! (`lower::ArgMode::FoldGround`), so `?- p("a", 1 + 1).` stays the single atom
//! it reads as. Hoisting made the body two literals with no named variables, so
//! an ordinary-looking query printed nothing (`bugs/005`).
//!
//! Facts are deduplicated and printed in the canonical order, by relation name
//! then value — so a ground conjunction's output does not depend on the order
//! its atoms were written in.
//!
//! The full agent CLI — `-q`, `--format json`, the skill definition — is
//! roadmap step 6; JSON stays reserved for the machine-readable edges (§12
//! errors, §11 provenance).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::ast::StatementKind;
use crate::engine::{Model, Provenance, eval_with, trace_failure};
use crate::error::{AggregateSite, Error, Warning};
use crate::ir;
use crate::lower::{check_program, lower_with_sources};
use crate::parser::parse;
use crate::print::{print_atom, print_explanation, print_ground_fact};
use crate::provenance::{Explained, ProofTree};
use crate::resolve::resolve_modules;
use crate::sources::load_imports;
use crate::typecheck::typecheck;

/// The result of a successful [`run`]: the least model plus each query's
/// canonical answer lines, in program (statement) order.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub model: Model,
    /// One entry per query, each a list of canonical fact lines (no trailing
    /// newline).
    pub answers: Vec<Vec<String>>,
    /// One entry per explanation goal (§11), each a block of `%`-comment lines.
    ///
    /// Kept apart from `answers` because an explanation is **not a row**: it
    /// rides in the same text stream and is invisible to the exit code, so
    /// appending `?why` to a check cannot change what the check answers (§17,
    /// 2026-08-21). Stripping these lines leaves the fact stream byte for byte
    /// (`testing.md` E5).
    pub explanations: Vec<Vec<String>>,
    /// Non-fatal diagnostics (e.g. referenced-but-undefined predicates). The
    /// program still ran; these belong on stderr, never in the fact stream.
    pub warnings: Vec<Warning>,
}

impl RunResult {
    /// All answer lines across every query, then every explanation block,
    /// joined with newlines (a trailing newline is added iff there is any
    /// output). This is the binary's stdout.
    ///
    /// Answers first, explanations after, both in program order: an explanation
    /// is commentary on a run and reads after the rows it is about.
    pub fn output(&self) -> String {
        let mut out = String::new();
        for lines in self.answers.iter().chain(&self.explanations) {
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }
}

/// Runs a program end to end with no file context: module and data imports
/// resolve against the working directory (§13). Equivalent to
/// [`run_at`]`(src, None)`.
pub fn run(src: &str) -> Result<RunResult, Vec<Error>> {
    run_at(src, None)
}

/// Runs a program end to end: `parse → resolve modules → lower → typecheck →
/// eval`, then answers every query. Returns every error found at the first
/// failing stage (each stage collects all of its own errors).
///
/// `source_path` is the program file itself when there is one; each file's
/// imports resolve relative to that file's directory (`None` — stdin or
/// `-q`-only programs — resolves against the working directory).
pub fn run_at(src: &str, source_path: Option<&Path>) -> Result<RunResult, Vec<Error>> {
    run_at_reporting(src, source_path, &mut |_| {})
}

/// [`run_at`], reporting each **static** warning to `report` as soon as it is
/// known — before evaluation starts.
///
/// The timing is the point, and it is load-bearing rather than cosmetic. A
/// program flagged by the termination lint (§10) may never reach a fixpoint, and
/// [`RunResult`] is only returned once it has, so a caller that reads warnings
/// off the result learns nothing about the very program the warning is for. That
/// is `bugs/004`'s symptom exactly: no output at all.
///
/// `RunResult::warnings` still carries the full list, static and post-evaluation
/// (§9's aggregate skips) alike, so a caller that does not care about timing —
/// and every existing one — needs no change. The reported ones are its **prefix**,
/// in order, which is what lets the CLI print the rest without repeating itself.
pub fn run_at_reporting(
    src: &str,
    source_path: Option<&Path>,
    report: &mut dyn FnMut(&Warning),
) -> Result<RunResult, Vec<Error>> {
    let ast = parse(src)?;
    let resolved = resolve_modules(ast, source_path)?;
    // Every stage after this one works over the IR and never holds the program
    // text, so it records a span and leaves the position to be resolved here
    // (§17, 2026-08-24). Spans are **per-file** byte offsets (`resolve.rs`), so
    // that is only sound while there is one file: a spliced-in module's offset
    // counts from its own text, and resolving it against the root would print a
    // confidently wrong line. With more than one file the span is dropped.
    let single_file = resolved.files.len() == 1;
    let place = |mut errors: Vec<Error>| {
        if single_file {
            crate::error::locate_all(&mut errors, src);
        } else {
            crate::error::forget_spans(&mut errors);
        }
        errors
    };
    let one = |error: Error| place(vec![error]);
    let tables = load_imports(&resolved.program).map_err(&place)?;
    let program = lower_with_sources(&resolved.program, &tables).map_err(&place)?;
    typecheck(&program).map_err(&place)?;
    let mut warnings = check_program(&program);
    for warning in &warnings {
        report(warning);
    }
    // The run provisions its own recorder from its goals (§17, 2026-08-21):
    // `?why` needs a derivation store, `?whynot` needs the model and a re-solve,
    // and a run with no goals at all needs neither — which is the case that was
    // paying 70–78% of peak RSS for nothing.
    let model = eval_with(&program, program.provenance()).map_err(&one)?;
    warnings.extend(absent_skip_warnings(&model, &program));

    let mut answers = Vec::with_capacity(program.queries.len());
    for (position, query) in program.queries.iter().enumerate() {
        // (body index) → (op, total skipped, groups that skipped), for the
        // aggregates written in *this* query. A query's premises are built and
        // then dropped on the floor by `answer`; this is the only place they
        // are read, and it is what makes §9's skip report reach a query (§14).
        let mut sites: BTreeMap<usize, (&'static str, usize, usize)> = BTreeMap::new();
        let mut lost_sites: BTreeMap<usize, (&'static str, usize)> = BTreeMap::new();
        let rows = model
            .answer_reporting(query, &mut |premises| {
                for (idx, premise) in premises.iter().enumerate() {
                    match premise {
                        Some(crate::provenance::Premise::Aggregate { op, skipped, .. })
                            if *skipped > 0 =>
                        {
                            let entry = sites.entry(idx).or_insert((op.keyword(), 0, 0));
                            entry.1 += skipped;
                            entry.2 += 1;
                        }
                        Some(crate::provenance::Premise::Builtin {
                            lost: Some(lost), ..
                        }) if !assignment_is_guarded(&query.body, idx) => {
                            let entry = lost_sites.entry(idx).or_insert((lost.to.keyword(), 0));
                            entry.1 += lost.count as usize;
                        }
                        _ => {}
                    }
                }
            })
            .map_err(&one)?;
        let site = || AggregateSite::Query(position + 1);
        warnings.extend(sites.into_values().map(|(op, skipped, groups)| {
            Warning::AbsentSkippedInAggregate {
                op,
                site: site(),
                skipped,
                groups,
            }
        }));
        warnings.extend(lost_sites.into_values().map(|(to, failed)| {
            Warning::ConversionFailedOnData {
                to,
                site: site(),
                failed,
            }
        }));
        answers.push(answer_lines(query, &rows, &program));
    }
    // The cross case, decided before the loop so at most one extra fixpoint is
    // ever run: `?whynot` over a fact that turns out to hold wants a proof, and
    // its own sigil provisioned no recorder. Re-evaluating reuses the lowered
    // program, so it costs the fixpoint and not the parse or the imports.
    let needs_recorded = model.provenance() == Provenance::Unrecorded
        && program
            .explanations
            .iter()
            .any(|explanation| model.contains(&explanation.goal));
    let recorded = if needs_recorded {
        Some(eval_with(&program, Provenance::Recorded).map_err(&one)?)
    } else {
        None
    };

    let mut explanations = Vec::with_capacity(program.explanations.len());
    for explanation in &program.explanations {
        let (answer, reran) = match recorded.as_ref() {
            Some(recorded) if model.contains(&explanation.goal) => {
                (ProofTree::explain(recorded, &explanation.goal), true)
            }
            _ if model.provenance() == Provenance::Recorded => {
                (ProofTree::explain(&model, &explanation.goal), false)
            }
            // Unprovisioned and the goal does not hold: the model alone answers
            // that, and no store was needed to say so.
            _ => (Explained::DoesNotHold, false),
        };
        // The trace is solved out of the finished model, so the arm that needs
        // no store is the one that costs a search — which is the shape that
        // makes `?whynot` cheap to provision for (§17, 2026-08-21).
        let trace = match &answer {
            Explained::DoesNotHold => {
                Some(trace_failure(&program, &model, &explanation.goal).map_err(&one)?)
            }
            Explained::Proof(_) | Explained::Unrecorded => None,
        };
        explanations.push(print_explanation(
            explanation.sigil,
            &explanation.goal,
            &answer,
            trace.as_ref(),
            reran,
            &program,
        ));
    }

    Ok(RunResult {
        model,
        answers,
        explanations,
        warnings,
    })
}

/// Builds the combined program source for the agent CLI: the `base` program
/// followed by one appended query (or rule + synthesized query) per `-q`
/// argument, in order (`spec.md` §14, `-q` semantics).
///
/// Each `-q` argument is classified by **parsing** it (never by splitting on
/// `:-`, which a string literal may contain):
///
/// - A **define-and-select rule** — a single clause with a non-empty body, e.g.
///   `gp(X,Z) :- parent(X,Y), parent(Y,Z)` — appends the rule plus a synthesized
///   query over its head, `?- gp(X, Z).`.
/// - An **explanation goal** — `?why dead("x")`, `?whynot calls("a","b")` — is
///   already a §5 statement and is passed through verbatim (§11). This is why
///   the surface needs no flag of its own: `-q` classifies by parsing, so the
///   sigil is one more arm.
/// - Anything else (a **bare atom** `ancestor("alice", X)`, a **comma-body**
///   `adult(N), N != "bob"`) is a query body and appends `?- <arg>.`.
///
/// A trailing `.` on the argument is optional. A malformed `-q` returns its own
/// parse errors, attributed to that argument rather than to the synthesized
/// combined source.
pub fn program_with_queries(base: &str, queries: &[String]) -> Result<String, Vec<Error>> {
    let mut source = base.to_string();
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
    }
    for query in queries {
        source.push_str(&query_source(query)?);
    }
    Ok(source)
}

/// Renders one `-q` argument to the program text it contributes (see
/// [`program_with_queries`]). The returned string ends with a newline.
fn query_source(arg: &str) -> Result<String, Vec<Error>> {
    // Normalize: trim surrounding whitespace and any single trailing `.`.
    let core = arg.trim();
    let core = core.strip_suffix('.').unwrap_or(core).trim_end();

    // An explanation goal carries its own sigil, so it is passed through as the
    // statement it already is (§17, 2026-08-21). It is classified first because
    // `?why p(…)` would otherwise reach the query-body path below and be lexed
    // as a stray `?`.
    if core.starts_with("?why") || core.starts_with("?whynot") {
        let goal = format!("{core}.");
        parse(&goal)?;
        return Ok(format!("{goal}\n"));
    }

    // A define-and-select rule parses as one *rule* with a non-empty body —
    // however many clauses that rule desugars to, since the parser expands a
    // top-level `;` into one clause per disjunct sharing the head (§17,
    // 2026-07-22). A bare atom parses as a clause with an *empty* body (a fact),
    // which we treat as a query body, not a rule.
    if let Ok(program) = parse(&format!("{core}."))
        && let Some((first, rest)) = program.statements.split_first()
        && let StatementKind::Clause(first_clause) = &first.kind
        && !first_clause.body.is_empty()
    {
        let head = print_atom(&first_clause.head);
        // Every clause must share that head, or this is two unrelated rules
        // rather than one disjunctive one — which belongs on the query-body path
        // below, where it becomes a parse error attributed to the argument,
        // instead of here where it would silently select only the first head.
        // Compared as printed text: the canonical printer is span-free, and the
        // shared head means the disjuncts print identically.
        let one_rule = rest.iter().all(|statement| match &statement.kind {
            StatementKind::Clause(clause) => {
                !clause.body.is_empty() && print_atom(&clause.head) == head
            }
            _ => false,
        });
        if one_rule {
            return Ok(format!("{core}.\n?- {head}.\n"));
        }
    }

    // Otherwise it is a query body. Validate it as one so a genuinely malformed
    // `-q` surfaces its own parse error here.
    let query = format!("?- {core}.");
    parse(&query)?;
    Ok(format!("{query}\n"))
}

/// Runs a `base` program plus one-shot `-q` queries with no file context.
/// Equivalent to [`run_with_queries_at`]`(base, None, queries)`.
pub fn run_with_queries(base: &str, queries: &[String]) -> Result<RunResult, Vec<Error>> {
    run_with_queries_at(base, None, queries)
}

/// Runs a `base` program plus one-shot `-q` queries end to end: builds the
/// combined source with [`program_with_queries`], then [`run_at`]s it. This is
/// the agent CLI's entry point (`datalog [<file>|-] [-q …]`); `source_path` is
/// the base program's file, threading §13 relative-path resolution (the
/// appended `-q` text has no paths of its own).
pub fn run_with_queries_at(
    base: &str,
    source_path: Option<&Path>,
    queries: &[String],
) -> Result<RunResult, Vec<Error>> {
    run_with_queries_at_reporting(base, source_path, queries, &mut |_| {})
}

/// [`run_with_queries_at`] with [`run_at_reporting`]'s eager warning channel —
/// what the CLI uses, so a warning about a program that may not terminate
/// reaches stderr before the fixpoint that may not finish.
pub fn run_with_queries_at_reporting(
    base: &str,
    source_path: Option<&Path>,
    queries: &[String],
    report: &mut dyn FnMut(&Warning),
) -> Result<RunResult, Vec<Error>> {
    let source = program_with_queries(base, queries)?;
    run_at_reporting(&source, source_path, report)
}

/// Reports what a run's derivations know and its answers do not: every
/// aggregate that skipped an `absent` input (§9's skip-but-report rule) and every
/// conversion that lost a value (§12's malformed-not-missing report), read back
/// out of the recorded provenance rather than tracked separately — [`crate::provenance::Premise::Aggregate`] already carries the
/// count, and this is what makes it visible before the `?why` surface (§11)
/// exists.
///
/// One warning per aggregate *site* — a `(rule, body index)` pair — summed over
/// every group that site produced. Sites are keyed in `BTreeMap` order so the
/// warning stream is deterministic like everything else in §14.
///
/// Aggregates written in a **query** are covered by
/// [`query_skip_warnings`], which reads the premises `Model::answer` builds and
/// used to discard; a query records no derivations, so this scan cannot see it.
fn absent_skip_warnings(model: &Model, program: &ir::Program) -> Vec<Warning> {
    // Only a program that aggregates or converts has anything to report here;
    // skipping the derivation scan keeps this free for everything else. It is
    // the same predicate that provisions the recorder, shared so the two cannot
    // drift — if this scan ever runs on an unprovisioned model it reports
    // nothing, silently.
    if !program.reports_through_provenance() {
        return Vec::new();
    }
    debug_assert_eq!(
        model.provenance(),
        crate::engine::Provenance::Recorded,
        "a program whose warnings are read out of derivations must provision them"
    );

    // (rule, body index) → (op, total skipped, groups that skipped).
    let mut sites: BTreeMap<(u32, usize), (&'static str, usize, usize)> = BTreeMap::new();
    // (rule, body index) → (target type, total values lost).
    let mut lost_sites: BTreeMap<(u32, usize), (&'static str, usize)> = BTreeMap::new();
    for fact in model.facts() {
        for derivation in model.derivations_of(&fact) {
            for (idx, premise) in derivation.premises.iter().enumerate() {
                if let crate::provenance::Premise::Aggregate { op, skipped, .. } = premise
                    && *skipped > 0
                {
                    let entry =
                        sites
                            .entry((derivation.rule.0, idx))
                            .or_insert((op.keyword(), 0, 0));
                    entry.1 += skipped;
                    entry.2 += 1;
                }
                if let crate::provenance::Premise::Builtin {
                    lost: Some(lost), ..
                } = premise
                    && !assignment_is_guarded(&program.rules[derivation.rule.0 as usize].body, idx)
                {
                    let entry = lost_sites
                        .entry((derivation.rule.0, idx))
                        .or_insert((lost.to.keyword(), 0));
                    entry.1 += lost.count as usize;
                }
            }
        }
    }

    let rule_site = |rule_id: u32| {
        let rule = &program.rules[rule_id as usize];
        let info = program.pred_info(rule.head.pred);
        AggregateSite::Rule(format!("{}/{}", info.name, info.arity))
    };

    let skips = sites
        .into_iter()
        .map(
            |((rule_id, _), (op, skipped, groups))| Warning::AbsentSkippedInAggregate {
                op,
                site: rule_site(rule_id),
                skipped,
                groups,
            },
        );
    let losses = lost_sites.into_iter().map(|((rule_id, _), (to, failed))| {
        Warning::ConversionFailedOnData {
            to,
            site: rule_site(rule_id),
            failed,
        }
    });
    skips.chain(losses).collect()
}

/// Whether the assignment at `body[idx]` has its result **guarded** by a
/// presence test in the same body — `V = X as int, V is [not] absent`.
///
/// A guarded assignment needs no malformed-value warning: the program already
/// asks the question the warning would answer, and §16.9 recommends exactly this
/// idiom for reading a mostly-numeric column. Warning there would be a
/// diagnostic firing on a *correct* program, which a sibling engine measured
/// leading a model to a destructive fix (`notes/tsdl-cross-project-review.md`) —
/// the same hazard that keeps this report dynamic rather than static (§12).
fn assignment_is_guarded(body: &[ir::BodyLiteral], idx: usize) -> bool {
    let ir::BodyLiteralKind::Compare { lhs, rhs, .. } = &body[idx].kind else {
        return false;
    };
    // The assigned variable is whichever side is a bare variable.
    let bound = [lhs, rhs].into_iter().find_map(|expr| match expr {
        ir::Expr::Term(ir::Term::Var(var)) => Some(*var),
        _ => None,
    });
    let Some(bound) = bound else { return false };
    body.iter().any(|literal| {
        matches!(
            &literal.kind,
            ir::BodyLiteralKind::Presence {
                expr: ir::Expr::Term(ir::Term::Var(var)),
                ..
            } if *var == bound
        )
    })
}

/// Renders one query's answer rows to canonical fact lines per the §14 output
/// shape. `rows` are the projected answer-variable bindings (in
/// [`ir::Query::projection`] order), as returned by [`Model::answer`]. An empty
/// projection makes `rows` the body's **truth value**: one empty row when it
/// holds, none when it does not.
fn answer_lines(query: &ir::Query, rows: &[Vec<ir::Value>], program: &ir::Program) -> Vec<String> {
    // Row position of each projected slot — this is the order `Model::answer`
    // lays out each row.
    let position_of: HashMap<u32, usize> = query
        .projection
        .iter()
        .enumerate()
        .map(|(position, &slot)| (slot, position))
        .collect();

    let atoms: Vec<&ir::Atom> = query
        .body
        .iter()
        .filter_map(|literal| match &literal.kind {
            ir::BodyLiteralKind::Atom(atom) => Some(atom),
            _ => None,
        })
        .collect();
    let atom_vars: BTreeSet<u32> = atoms
        .iter()
        .flat_map(|atom| atom.args.iter())
        .filter_map(|term| match term {
            ir::Term::Var(var) => Some(var.0),
            ir::Term::Const(_) => None,
        })
        .collect();
    let projected: BTreeSet<u32> = query.projection.iter().copied().collect();

    // Substituted-atom form. Printing a real predicate's name is only honest
    // when the atoms account for **every** answer variable, so this is set
    // equality rather than the one-directional "every argument is projected":
    // a body can bind a variable no atom mentions (an aggregate result, an
    // `=`-assignment), and emitting the atoms would silently drop that column.
    //
    // One atom may carry a whole projection; several may carry only a ground
    // yes, which `atom_vars` being empty is exactly the test for. The
    // combination that matched a *multi-row* answer is not recoverable from
    // several atoms' tuples alone — a filter that pruned rows leaves no trace
    // in them — so those bodies fall through to `answer/N` (§17, 2026-08-17).
    if !atoms.is_empty() && atom_vars == projected && (atoms.len() == 1 || atom_vars.is_empty()) {
        let position_of = &position_of;
        let mut facts: Vec<(&str, Vec<ir::Value>)> = rows
            .iter()
            .flat_map(|row| {
                atoms.iter().map(move |atom| {
                    let tuple = atom
                        .args
                        .iter()
                        .map(|term| match term {
                            ir::Term::Const(value) => value.clone(),
                            ir::Term::Var(var) => row[position_of[&var.0]].clone(),
                        })
                        .collect();
                    (program.pred_info(atom.pred).name.as_str(), tuple)
                })
            })
            .collect();
        // By name then value, so a ground conjunction prints the same bytes
        // however its atoms were ordered.
        facts.sort();
        facts.dedup();
        return facts
            .iter()
            .map(|(name, values)| print_ground_fact(name, values))
            .collect();
    }

    // No answer variables, and nothing substitutable to show: the body's whole
    // content is a yes. Withholding it is what made an existence check
    // indistinguishable from a failing one — `?- p("a"), q("b").` printed
    // nothing either way, and so did `?- not banned("bob").` and `?- p(_).`.
    if projected.is_empty() {
        return if rows.is_empty() {
            Vec::new()
        } else {
            vec![print_ground_fact("holds", &[ir::Value::Bool(true)])]
        };
    }

    // Otherwise: synthesized `answer/N` facts over the answer variables.
    rows.iter()
        .map(|row| print_ground_fact("answer", row))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn single_atom_query_substitutes_bindings() {
        let src = "\
parent(\"alice\", \"bob\").
parent(\"bob\", \"carol\").
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
?- ancestor(\"alice\", Who).
";
        let result = run(src).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "ancestor(\"alice\", \"bob\").".to_string(),
                "ancestor(\"alice\", \"carol\").".to_string(),
            ]]
        );
    }

    /// A non-binding filter beside the atom keeps the substituted form: the
    /// atom still accounts for every answer variable, and `>=` binds nothing
    /// (§17, 2026-08-03, built 2026-08-17).
    #[test]
    fn a_filtered_single_atom_query_still_substitutes() {
        let src = "\
age(\"alice\", 30).
age(\"bob\", 15).
?- age(N, A), A >= 18.
";
        let result = run(src).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["age(\"alice\", 30).".to_string()]]
        );
    }

    /// The widening's own boundary: an aggregate binds a variable no atom
    /// mentions, so substituting the atom would drop that column. Set equality
    /// is what catches it — the old one-directional test did not.
    #[test]
    fn an_aggregate_bound_variable_falls_to_the_synthesized_form() {
        let src = "\
age(\"alice\", 30).
banned(\"carol\").
?- age(N, A), C = count { X | banned(X) }.
";
        let result = run(src).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["answer(\"alice\", 30, 1).".to_string()]]
        );
    }

    #[test]
    fn ground_single_atom_query_prints_when_it_holds() {
        let holds = run("p(\"a\").\n?- p(\"a\").").expect("runs");
        assert_eq!(holds.answers, vec![vec!["p(\"a\").".to_string()]]);
        let absent = run("p(\"a\").\n?- p(\"b\").").expect("runs");
        assert_eq!(absent.answers, vec![Vec::<String>::new()]);
    }

    #[test]
    fn inline_arithmetic_evaluates_like_a_hand_hoisted_assignment() {
        // The feature invariant: an inline arithmetic argument evaluates the
        // same as writing the assignment out by hand.
        let inline = run("number(1).\nnumber(2).\nsucc(N, N + 1) :- number(N).\n?- succ(X, Y).")
            .expect("runs");
        let hoisted =
            run("number(1).\nnumber(2).\nsucc(N, M) :- number(N), M = N + 1.\n?- succ(X, Y).")
                .expect("runs");
        assert_eq!(inline.answers, hoisted.answers);
        assert_eq!(
            inline.answers,
            vec![vec!["succ(1, 2).".to_string(), "succ(2, 3).".to_string(),]]
        );
    }

    /// `bugs/001`: a computed argument under `not` used to be read as a
    /// wildcard, so the rule silently degraded to `not q(_)` — "no `q` fact at
    /// all" — and one unrelated `q(3)` suppressed every row.
    ///
    /// For `X = 1`, `not q(2)` holds; for `X = 2`, `not q(3)` fails; for
    /// `X = 3`, `not q(4)` holds.
    #[test]
    fn a_computed_argument_under_negation_is_matched_not_wildcarded() {
        let facts = "p(1).\np(2).\np(3).\nq(3).\n";
        let expected = vec![vec!["r(1).".to_string(), "r(3).".to_string()]];
        let inline = run(&format!("{facts}r(X) :- p(X), not q(X + 1).\n?- r(X).")).expect("runs");
        assert_eq!(inline.answers, expected);
    }

    /// The §5 claim that inline arithmetic and a hand-written assignment lower
    /// to the same IR, extended to negated atoms — where it used to fail three
    /// different ways at once (`bugs/001`): the inline form returned the wrong
    /// rows, and both hand-hoisted spellings were rejected outright.
    #[test]
    fn negated_inline_arithmetic_agrees_with_both_hoisted_spellings() {
        let facts = "p(1).\np(2).\np(3).\nq(3).\n";
        let inline =
            run(&format!("{facts}r(X) :- p(X), not q(X + 1).\n?- r(X).")).expect("inline runs");
        let binder_first = run(&format!(
            "{facts}r(X) :- p(X), Y = X + 1, not q(Y).\n?- r(X)."
        ))
        .expect("binder-first runs");
        let binder_last = run(&format!(
            "{facts}r(X) :- p(X), not q(Y), Y = X + 1.\n?- r(X)."
        ))
        .expect("binder-last runs");
        assert_eq!(inline.answers, binder_first.answers);
        assert_eq!(inline.answers, binder_last.answers);
    }

    /// The same defect reached through an aggregate result rather than
    /// arithmetic — also a lowering-generated slot, also hoisted (`bugs/001`).
    /// `count { Y | p(Y) }` is 2, so `not q(2)` holds for every `X`.
    #[test]
    fn an_aggregate_argument_under_negation_is_matched_not_wildcarded() {
        let program = "p(1).\np(2).\nq(3).\nr(X) :- p(X), not q(count { Y | p(Y) }).\n?- r(X).";
        let result = run(program).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["r(1).".to_string(), "r(2).".to_string()]]
        );
    }

    /// The widening does not reach genuine wildcards: `_` under a negation is
    /// bound nowhere, so it stays open and existential (§7).
    #[test]
    fn a_wildcard_under_negation_stays_existential() {
        let result =
            run("p(1).\np(2).\nq(2, 9).\nr(X) :- p(X), not q(X, _).\n?- r(X).").expect("runs");
        assert_eq!(result.answers, vec![vec!["r(1).".to_string()]]);
    }

    /// A *named* variable no literal binds is still unsafe — the one thing the
    /// scheduler cannot decide, since to it an unbound slot is a wildcard.
    #[test]
    fn a_named_variable_bound_nowhere_is_still_unsafe_under_negation() {
        let errors = run("p(1).\nr(X) :- p(X), not q(Y).\n?- r(X).").expect_err("Y is unsafe");
        assert!(
            errors.iter().any(|e| {
                let msg = e.to_string();
                msg.contains("negated atom") && msg.contains("`Y`")
            }),
            "unexpected errors: {errors:?}"
        );
    }

    // --- Absent value, end-to-end through the pipeline (§4/§8) ---

    /// A `measurement` fixture with one present and one missing amount. The
    /// missing cell is the `absent` literal in a fact — the same value an empty
    /// import cell will produce (§13).
    const SPARSE: &str = "m(\"a\", 5).\nm(\"b\", absent).\n";

    #[test]
    fn presence_test_partitions_rows() {
        // `is not absent` and `is absent` each select exactly one row; together
        // they cover both (unlike `=`/`!=`, which are both false on absent).
        let present = run(&format!(
            "{SPARSE}has(F) :- m(F, A), A is not absent.\n?- has(F)."
        ))
        .expect("runs");
        assert_eq!(present.answers, vec![vec!["has(\"a\").".to_string()]]);

        let missing = run(&format!(
            "{SPARSE}mis(F) :- m(F, A), A is absent.\n?- mis(F)."
        ))
        .expect("runs");
        assert_eq!(missing.answers, vec![vec!["mis(\"b\").".to_string()]]);
    }

    #[test]
    fn threshold_silently_excludes_absent() {
        // A comparison with an absent operand is false, so the missing row drops
        // out — no type error despite the column being int.
        let result =
            run(&format!("{SPARSE}high(F) :- m(F, A), A >= 5.\n?- high(F).")).expect("runs");
        assert_eq!(result.answers, vec![vec!["high(\"a\").".to_string()]]);
    }

    #[test]
    fn both_eq_and_ne_are_false_on_absent() {
        // A comparison against a *value* is silently false when the operand is
        // absent — the two-valued rule (§8), and the reason presence has its own
        // operator. `m("b", absent)` is selected by neither `A = 9` nor `A != 9`,
        // even though one of them holds for every ordinary value.
        for (op, expected) in [("=", Vec::new()), ("!=", vec!["q(\"a\").".to_string()])] {
            let result = run(&format!("{SPARSE}q(F) :- m(F, A), A {op} 9.\n?- q(F)."))
                .unwrap_or_else(|e| panic!("runs: {e:?}"));
            assert_eq!(
                result.answers,
                vec![expected],
                "`A {op} 9` never selects the absent row"
            );
        }
    }

    #[test]
    fn comparing_against_the_literal_absent_is_a_steered_error() {
        // Writing the literal is the mistake the presence operator exists to
        // prevent: `A = absent` and `A != absent` are *both* always false, so
        // rather than silently answering nothing, lowering steers to `is absent`.
        for op in ["=", "!=", "<", ">="] {
            let errors = run(&format!(
                "{SPARSE}q(F) :- m(F, A), A {op} absent.\n?- q(F)."
            ))
            .expect_err("comparing against the literal `absent` is rejected");
            let message = errors[0].to_string();
            assert!(
                message.contains("is always false") && message.contains("X is not absent"),
                "expected a presence-test steer for `{op}`, got: {message}"
            );
        }
        // The producer form is untouched: `X = absent` binding an unbound `X`.
        let produced = run(&format!(
            "{SPARSE}z(F, X) :- m(F, _), X = absent.\n?- z(F, X)."
        ))
        .expect("the producer form still lowers");
        assert_eq!(
            produced.answers,
            vec![vec![
                "z(\"a\", absent).".to_string(),
                "z(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn absent_flows_to_the_head_and_round_trips() {
        // Naming a column returns every row; the missing one prints as the
        // reserved literal `absent` (Datalog-out is Datalog-in).
        let result = run(&format!("{SPARSE}rec(F, A) :- m(F, A).\n?- rec(F, A).")).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "rec(\"a\", 5).".to_string(),
                "rec(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn arithmetic_annihilates_through_a_computed_column() {
        let result = run(&format!(
            "{SPARSE}plus(F, A) :- m(F, X), A = X + 10.\n?- plus(F, A)."
        ))
        .expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "plus(\"a\", 15).".to_string(),
                "plus(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn absent_keys_do_not_join() {
        // Missing foreign keys must not cartesian-blow-up: absent unifies with
        // nothing, including another absent.
        let src = "a(\"x\", absent).\nb(\"y\", absent).\nj(P, Q) :- a(P, K), b(Q, K).\n?- j(P, Q).";
        let result = run(src).expect("runs");
        assert_eq!(result.answers, vec![Vec::<String>::new()]);
    }

    /// The third row of §4's four-site table: an aggregate **group key** stays
    /// **semantic**, so a group keyed on `absent` is real but empty. Verified
    /// against SQL, where a correlated subquery keyed on `NULL` counts nothing.
    ///
    /// Pinned alongside `absent_keys_do_not_join` and the negation properties
    /// because §4 now asserts all four sites normatively, and this is the one
    /// the 2026-07-29 session did *not* change — a silent drift here would make
    /// the table wrong without failing anything else.
    #[test]
    fn an_absent_group_key_makes_a_real_but_empty_group() {
        let src = "k(absent).\nk(\"a\").\nv(absent, 99).\nv(\"a\", 1).\n\
                   g(K, N) :- k(K), N = count { C | v(K, C) }.\n?- g(K, N).";
        let result = run(src).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "g(absent, 0).".to_string(),
                "g(\"a\", 1).".to_string()
            ]]
        );
    }

    #[test]
    fn a_literal_absent_in_a_body_atom_is_an_error_steering_to_is_absent() {
        let errors =
            run(&format!("{SPARSE}q(F) :- m(F, absent).\n?- q(F).")).expect_err("rejected");
        let joined = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("is absent"), "got: {joined}");
        assert!(joined.contains("cannot be matched"), "got: {joined}");
    }

    #[test]
    fn a_literal_absent_in_a_head_or_fact_is_allowed() {
        // Production sites accept the literal: the fact above, plus a rule head.
        let result = run(&format!(
            "{SPARSE}tag(F, absent) :- m(F, _).\n?- tag(F, A)."
        ))
        .expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "tag(\"a\", absent).".to_string(),
                "tag(\"b\", absent).".to_string(),
            ]]
        );
    }

    #[test]
    fn disjunction_is_equivalent_to_separate_rules() {
        let disjunctive = run("a(1).\nb(2).\np(X) :- a(X) ; b(X).\n?- p(X).").expect("runs");
        let separate = run("a(1).\nb(2).\np(X) :- a(X).\np(X) :- b(X).\n?- p(X).").expect("runs");
        assert_eq!(disjunctive.answers, separate.answers);
        assert_eq!(
            disjunctive.answers,
            vec![vec!["p(1).".to_string(), "p(2).".to_string(),]]
        );
    }

    // --- Surface-spelling equivalence (testing.md C8) ---
    //
    // Two ways of writing one program must answer identically. Both claims
    // below were carried by the unit tests above them until this session; the
    // record for such claims is that the ones with properties held and the ones
    // without became `bugs/001` and `bugs/002`.

    proptest! {
        /// **C8** — `;` disjunction means the same as writing the disjuncts as
        /// separate rules (§5; the parser expands one clause per disjunct,
        /// §17 2026-07-22). Generalizes
        /// `disjunction_is_equivalent_to_separate_rules` above.
        #[test]
        fn disjunction_equals_separate_rules(
            (disjunctive, separate) in crate::testgen::arb_disjunction_spellings()
        ) {
            let a = run(&disjunctive).map(|r| r.answers);
            let b = run(&separate).map(|r| r.answers);
            match (a, b) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "the two spellings disagreed on acceptance: {:?} vs {:?}\n\
                     --- disjunctive ---\n{}\n--- separate ---\n{}",
                    a.is_ok(), b.is_ok(), &disjunctive, &separate
                ),
            }
        }
    }

    /// Non-vacuity for `a_runs_output_appended_to_it_changes_nothing`
    /// (`testing.md` rule 2). A program that answers **nothing** appends an
    /// empty string and re-runs trivially, so the property is satisfied by a
    /// generator that never answers — and checked against the property's
    /// sentence, what has to be reached is a run whose *output* is non-empty,
    /// not merely one that succeeds.
    ///
    /// It asserts the stronger thing too: that some generated program answers
    /// under a **real relation's name** rather than the `answer/N` fallback, so
    /// the appended facts land in a relation a rule already derives. That is the
    /// case where the second run could differ, and without it the property would
    /// only ever append inert `answer(…)` rows.
    #[test]
    fn closure_generator_produces_runs_that_answer() {
        use proptest::strategy::{Strategy, ValueTree};
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = crate::testgen::arb_closure_program();
        let (mut answered, mut named_relation) = (0, 0);
        for _ in 0..300 {
            let program = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            let Ok(result) = run(&program) else { continue };
            let lines: Vec<&String> = result.answers.iter().flatten().collect();
            if lines.is_empty() {
                continue;
            }
            answered += 1;
            if lines.iter().any(|l| !l.starts_with("answer(")) {
                named_relation += 1;
            }
        }
        assert!(
            answered > 0,
            "no generated program answered anything — the closure property \
             appends an empty string and proves nothing"
        );
        assert!(
            named_relation > 0,
            "every answer used the `answer/N` fallback — the appended facts are \
             inert, so the property never reaches a relation a rule derives"
        );
    }

    proptest! {
        /// **§14's closure property at the program level** — a run's own output,
        /// appended to the program that produced it, re-runs to the same
        /// answers. "Datalog out is Datalog in", stated about a *program* rather
        /// than a fact set.
        ///
        /// D1 closes fact sets through print → parse → lower, which is the value
        /// layer; this is the claim pillar 3 actually makes, and it existed only
        /// as one hand-written example in `tests/pipeline.rs`. It is B2's
        /// fixpoint idempotence carried through the text layer, and it is not
        /// implied by B2: B2 saturates the *IR*, where this appends what the
        /// **printer** emitted and the **parser** read back, so a rendering that
        /// loses or renames a value fails here and not there.
        ///
        /// The appended output is deliberately not inert. Under §14 a query over
        /// a single relation answers under **that relation's name**, so the
        /// appended facts land in a relation the program already derives — and
        /// in one shape, in a relation another rule *negates*, which is where a
        /// second run could plausibly differ.
        ///
        /// **Mutation — the honest result, after four aims.** This property has
        /// no kill of its own, and the reason is structural rather than a defect
        /// in the property: it is a *composition* claim over stages that each
        /// already carry a dedicated guard. Rendering a value wrongly reddens
        /// **D1** (and this) — `Value::Int` printing as `1.0` fails both.
        /// Getting §14's answer *shape* wrong reddens **C8**'s
        /// `c8_an_answer_shape_neither_drops_nor_collapses_a_row` and not this.
        /// Emitting the ground yes as the arity-0 `holds.` — unparseable under
        /// §5 — reddens **`tests/system.rs`** and not this.
        ///
        /// Recorded rather than smoothed over, because "no unique kill" is the
        /// expected result for an end-to-end claim over well-guarded stages, and
        /// the alternative reading — that it asserts nothing — is wrong: it is
        /// the only check that the stages compose, and it is what a *future*
        /// change to any of them has to keep true.
        ///
        /// A fifth aim did find something, and it is why the `holds(true).`
        /// shape is in the generator: the `holds.` mutation is invisible to
        /// `cargo test --lib`, so a mutation check run with that filter — as
        /// several in this session's first pass were — would have recorded a
        /// kill that did not happen. **Mutation-verify against the full suite.**
        #[test]
        fn a_runs_output_appended_to_it_changes_nothing(
            program in crate::testgen::arb_closure_program()
        ) {
            let Ok(first) = run(&program) else { return Ok(()) };
            let appended: String = first
                .answers
                .iter()
                .flat_map(|lines| lines.iter())
                .map(|line| format!("{line}\n"))
                .collect();
            let second = run(&format!("{program}{appended}"))
                .map_err(|e| TestCaseError::fail(format!(
                    "a program's own output did not re-parse as a program:\n\
                     --- program ---\n{program}--- appended ---\n{appended}--- {e:?}"
                )))?;
            prop_assert_eq!(
                &second.answers, &first.answers,
                "re-running with its own output changed the answers\n\
                 --- program ---\n{}--- appended ---\n{}",
                &program, &appended
            );
        }

        /// **E5 — comment-stripping is the closure guard.** Proof trees are not
        /// facts (§17, 2026-08-16, decided in the negative), so there is no
        /// fact-shaped provenance output to run D1 over. What replaces it:
        /// stripping every comment from a program's output must leave
        /// **byte-for-byte** what the same program prints without its goals.
        ///
        /// Blocked since 2026-08-16 on there being nothing that could *ask* —
        /// E7 (every proof line is a comment) is its lexical precondition and
        /// landed 2026-08-21 with the rendering; this is the program-level half,
        /// unblocked by the §5 goal form (§17, 2026-08-21).
        ///
        /// It guards two things at once, which is why it is cheap: that an
        /// explanation never leaks a fact into the stream, and that
        /// **provisioning the recorder does not change what a run prints** —
        /// `?why` evaluates with the store on and the plain run without it
        /// (E9 states the engine-level half).
        #[test]
        fn e5_stripping_the_comments_leaves_the_output_without_the_goals(
            program in crate::testgen::arb_closure_program()
        ) {
            let Ok(plain) = run(&program) else { return Ok(()) };
            let Some(fact) = plain.answers.iter().flatten().next() else {
                return Ok(());
            };
            // Both sigils over a fact that holds, and one over a fact that does
            // not — so the trace arm is exercised beside the proof arm.
            let held = fact.trim_end_matches('.');
            let goals = format!("?why {held}.\n?whynot {held}.\n?whynot n(\"zzz\", -99).\n");
            let explained = run(&format!("{program}{goals}"))
                .map_err(|e| TestCaseError::fail(format!(
                    "a program with goals did not run:\n                     --- program ---\n{program}--- goals ---\n{goals}--- {e:?}"
                )))?;

            let output = explained.output();
            prop_assert!(
                output.lines().any(|line| line.starts_with('%')),
                "no explanation was printed, so stripping is vacuous:\n{}",
                &output
            );
            let stripped: String = output
                .lines()
                .filter(|line| !line.starts_with('%'))
                .map(|line| format!("{line}\n"))
                .collect();
            prop_assert_eq!(
                stripped, plain.output(),
                "an explanation changed the fact stream\n                 --- program ---\n{}--- goals ---\n{}",
                &program, &goals
            );
        }
    }

    proptest! {
        /// **The structural laws of the relational algebra the language
        /// embodies** — join idempotence and union idempotence (§5/§6).
        /// Distribution is *not* here: §5 makes a body top-level DNF with no
        /// parentheses, so `(q ; r), s` has no spelling and the law has no two
        /// sides to compare (the generator's doc records how that was found).
        ///
        /// Each is a "these two spellings mean the same thing" claim, so by
        /// `testing.md` rule 1 each wants a property; none had one. B5 covers
        /// the *commutativity* of a conjunction and B6 the commutativity of a
        /// union, which left the idempotent and distributive laws as the two
        /// sides of the algebra with nothing looking at them.
        ///
        /// **Join idempotence is the pointed one.** Its only written form in
        /// this crate was `repeating_a_body_literal_drops_absent_rows`, which
        /// pins the case where the law *fails* — deliberately, since idempotence
        /// over `absent` is a join property and restoring it means giving up
        /// `NULL ≠ NULL` (§4). A law recorded only as its own exception reads as
        /// an accident; stating the positive over absent-free data is what makes
        /// the asymmetry legible as a decision.
        ///
        /// **Mutation, and the honest result.** No single-site mutation
        /// isolates these two arms, and finding that out is worth more than a
        /// tidier record. Rebinding rather than re-checking an already-bound
        /// variable in `try_match` — the change that should break a self-join —
        /// reddens eight other tests including B1 and the group-by oracle, and
        /// the reason is structural: **the engine has no code path for a
        /// repeated literal or a duplicated rule.** Both laws are consequences
        /// of set semantics and the join loop, not behaviours implemented
        /// anywhere, so there is nothing to mutate that touches only them.
        ///
        /// That makes these regression guards against a *future* optimisation —
        /// a body-literal deduplicator, a rule-level CSE — rather than guards on
        /// current code, which is a legitimate thing for a property to be as
        /// long as the record says so rather than implying a kill it never had.
        #[test]
        fn the_structural_laws_hold(
            (law, base, variant) in crate::testgen::arb_structural_law_spellings()
        ) {
            let a = run(&base).map(|r| r.answers);
            let b = run(&variant).map(|r| r.answers);
            match (a, b) {
                (Ok(a), Ok(b)) => prop_assert_eq!(
                    a, b,
                    "{:?} does not hold\n--- base ---\n{}\n--- variant ---\n{}",
                    law, &base, &variant
                ),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "{:?}: the two spellings disagreed on acceptance: {:?} vs {:?}\n\
                     --- base ---\n{}\n--- variant ---\n{}",
                    law, a.is_ok(), b.is_ok(), &base, &variant
                ),
            }
        }
    }

    proptest! {
        /// **C8** — a `-q` rule answers exactly as the same rule written into
        /// the file would (§14: `-q` "is sugar for appending `?- …` to the
        /// loaded program").
        ///
        /// This was `bugs/002`'s executable acceptance criterion, `#[ignore]`d
        /// and failing until the classifier stopped matching a *single*
        /// statement (resolved 2026-07-26).
        #[test]
        fn dash_q_rule_equals_the_same_rule_in_a_file(
            (base, rule, head) in crate::testgen::arb_dash_q_rule()
        ) {
            let from_file = run(&format!("{base}{rule}.\n?- {head}.\n")).map(|r| r.answers);
            let from_q = run_with_queries(&base, std::slice::from_ref(&rule)).map(|r| r.answers);
            match (from_file, from_q) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "`-q` and the file form disagreed on acceptance ({:?} vs {:?}) \
                     for rule `{}`", a.is_ok(), b.is_ok(), &rule
                ),
            }
        }
    }

    proptest! {
        /// **C8** — `<` and `min`/`max` answer the same question about order.
        /// For two distinct constants of one type, the value `<` puts first is
        /// the value `min` returns, over all five primitives (§4/§8/§9).
        ///
        /// `bugs/006`'s acceptance criterion, and the guard that keeps §4's
        /// order **single-homed**: it is implemented three times over — `<` in
        /// `apply_compare`, the `min`/`max` fold, and the printer's sort — all
        /// deriving from one `Ord` on `Value`, with nothing to notice if one
        /// drifts. The defect was not a disagreement about order but a
        /// disagreement about *scope*: `min` ordered all five types while the
        /// type checker let `<` see only int and float. It could not be written
        /// until the check widened, which is exactly why it was worth writing.
        #[test]
        fn ordered_comparison_and_minmax_agree_on_every_type(
            (compared, folded) in crate::testgen::arb_order_agreement_spellings()
        ) {
            let a = run(&compared).map(|r| r.answers);
            let b = run(&folded).map(|r| r.answers);
            match (a, b) {
                (Ok(a), Ok(b)) => prop_assert_eq!(
                    a, b,
                    "`<` and the aggregate disagree\n\
                     --- compared ---\n{}\n--- folded ---\n{}",
                    &compared, &folded
                ),
                (a, b) => prop_assert!(
                    false,
                    "the two spellings disagreed on acceptance: {:?} vs {:?}\n\
                     --- compared ---\n{}\n--- folded ---\n{}",
                    a.is_ok(), b.is_ok(), &compared, &folded
                ),
            }
        }
    }

    proptest! {
        /// **C8** — a **query** argument written as arithmetic answers exactly
        /// as the same argument written as its value: `?- n("a", 1 + 1).` and
        /// `?- n("a", 2).` are one question (§5/§8/§14).
        ///
        /// `bugs/005`'s executable acceptance criterion. The defect was that
        /// the first printed *nothing* while the second printed the fact,
        /// because hoisting turned a single-atom query into a two-literal body
        /// with no named variables — the one shape §14 emitted nothing for.
        ///
        /// The A15 analogue makes the *IR-identity* claim over rule bodies and
        /// excludes queries, because a hand-written variable becomes an answer
        /// column where lowering's anonymous slot does not. That exclusion
        /// stays; this is the **output** claim, which is the one §14's shape
        /// rule can break.
        #[test]
        fn a_computed_query_argument_answers_like_its_value(
            (computed, folded) in crate::testgen::arb_ground_query_spellings()
        ) {
            let a = run(&computed).map(|r| r.answers);
            let b = run(&folded).map(|r| r.answers);
            match (a, b) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "the two spellings disagreed on acceptance: {:?} vs {:?}\n\
                     --- computed ---\n{}\n--- folded ---\n{}",
                    a.is_ok(), b.is_ok(), &computed, &folded
                ),
            }
        }
    }

    proptest! {
        /// **C8**, the same claim widened past the query: folding a ground
        /// compound argument *anywhere* — fact, rule head, rule body, query —
        /// does not change what the program answers.
        ///
        /// Weaker than the property above and deliberately kept separate: over
        /// arbitrary generated programs the interesting case (a query that
        /// both computes and *matches*) is vanishingly rare, so this one would
        /// pass with or without `bugs/005`'s fix. It guards the positions the
        /// targeted generator does not reach, and nothing more.
        #[test]
        fn folding_a_ground_argument_anywhere_does_not_change_the_answer(
            program in crate::testgen::arb_ast_program()
        ) {
            let inline = crate::print::print_program(&program);
            let folded = crate::print::print_program(
                &crate::testgen::fold_ground_atom_args(&program)
            );
            let a = run(&inline).map(|r| r.answers);
            let b = run(&folded).map(|r| r.answers);
            match (a, b) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
                (Err(_), Err(_)) => {}
                (a, b) => prop_assert!(
                    false,
                    "the two spellings disagreed on acceptance: {:?} vs {:?}\n\
                     --- inline ---\n{}\n--- folded ---\n{}",
                    a.is_ok(), b.is_ok(), &inline, &folded
                ),
            }
        }
    }

    proptest! {
        /// **C8**: a substituted answer is the *same answer* the synthesized form
        /// would have given — every row present, under the real relation's name.
        ///
        /// The oracle filters the generator's own fact list, so it never calls
        /// `answer_lines` and cannot agree with a wrong shape rule
        /// (`testing.md`'s oracle corollary). What it catches is a **dropped
        /// column or a collapsed row**: substituting an atom that does not
        /// account for every answer variable maps two distinct answer rows to one
        /// output line, which is a silently smaller answer — the failure the
        /// one-directional guard admitted and set equality closes.
        #[test]
        fn c8_an_answer_shape_neither_drops_nor_collapses_a_row(
            case in crate::testgen::arb_answer_shape_case()
        ) {
            let result = run(&case.program).expect("generated programs run");
            prop_assert_eq!(
                &result.answers,
                &vec![case.expected.clone()],
                "shape {} disagreed\n--- program ---\n{}",
                case.shape, &case.program
            );
        }
    }

    proptest! {
        /// **C8**: a **named** query means exactly the rule whose head is the
        /// projection, plus a query over that head (§14, §17 2026-08-17). The
        /// claim is an equivalence between two spellings, so it ships as a
        /// property — and its oracle is the desugaring itself, written out
        /// textually here rather than obtained by asking lowering what the
        /// projection was.
        ///
        /// The second assertion is the independent half: the rows come from the
        /// generator's own fact list, so a desugaring that dropped a column or a
        /// row would have to fool both spellings *and* the fact list.
        #[test]
        fn c8_a_named_query_matches_its_desugared_rule(
            case in crate::testgen::arb_answer_shape_case()
        ) {
            let named = format!("{}?- ans: {}.\n", case.edb, case.body);
            // §5 bans a 0-arity atom, so a body with no answer variables
            // desugars to the ground head `ans(true)` — the one place the head
            // is not simply the projection.
            let desugared = if case.projection.is_empty() {
                format!("{}ans(true) :- {}.\n?- ans(true).\n", case.edb, case.body)
            } else {
                let head = format!("ans({})", case.projection.join(", "));
                format!("{}{head} :- {}.\n?- {head}.\n", case.edb, case.body)
            };
            let named_answers = run(&named)
                .map(|r| r.answers)
                .unwrap_or_else(|e| panic!("the named form runs: {e:?}"));
            let desugared_answers = run(&desugared)
                .map(|r| r.answers)
                .unwrap_or_else(|e| panic!("the desugared form runs: {e:?}"));
            prop_assert_eq!(
                &named_answers,
                &desugared_answers,
                "shape {} disagreed between spellings\n--- named ---\n{}\n--- desugared ---\n{}",
                case.shape, &named, &desugared
            );
            prop_assert_eq!(
                &named_answers,
                &vec![case.expected_named.clone()],
                "shape {} did not publish the projection\n--- named ---\n{}",
                case.shape, &named
            );
        }
    }

    /// The non-vacuity guard for the generator above: it must reach all three
    /// shapes and must actually *answer*, since a generator whose cases answered
    /// nothing would satisfy the property forever.
    #[test]
    fn answer_shape_cases_reach_every_shape_and_answer() {
        use proptest::strategy::{Strategy, ValueTree};
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = crate::testgen::arb_answer_shape_case();
        let mut seen = [0usize; 4];
        let mut answered = 0;
        for _ in 0..300 {
            let case = strategy.new_tree(&mut runner).expect("generates").current();
            seen[case.shape as usize] += 1;
            if !case.expected.is_empty() {
                answered += 1;
            }
        }
        assert!(
            seen.iter().all(|count| *count > 0),
            "generator missed a shape: {seen:?}"
        );
        assert!(
            answered > 200,
            "only {answered}/300 cases answered — the property would be near-vacuous"
        );
    }

    #[test]
    fn constant_folding_in_a_fact() {
        let result = run("p(1 + 1).\n?- p(X).").expect("runs");
        assert_eq!(result.answers, vec![vec!["p(2).".to_string()]]);
    }

    /// `bugs/005`: `?- p("a", 1 + 1).` printed nothing while `?- p("a", 2).`
    /// printed the fact — the same question, two spellings, two answers, and
    /// the empty one indistinguishable from "no such fact". A query now folds a
    /// *ground* compound argument instead of hoisting it, so it stays the
    /// single atom the user wrote (§14 reads output shape off the body).
    #[test]
    fn a_ground_computed_argument_answers_like_the_folded_spelling() {
        let facts = "p(\"a\", 2).\n";
        let folded = run(&format!("{facts}?- p(\"a\", 2).")).expect("folded runs");
        let computed = run(&format!("{facts}?- p(\"a\", 1 + 1).")).expect("computed runs");
        assert_eq!(folded.answers, computed.answers);
        assert_eq!(computed.answers, vec![vec!["p(\"a\", 2).".to_string()]]);
    }

    /// The same defect one variable short of ground: the computed argument is
    /// ground even though the atom is not, so folding leaves a single atom and
    /// the substituted form prints — where hoisting made it a two-literal body
    /// and downgraded the answer to `answer("a")`.
    #[test]
    fn a_ground_computed_argument_folds_in_a_non_ground_query() {
        let result = run("p(\"a\", 2).\np(\"b\", 3).\n?- p(X, 1 + 1).").expect("runs");
        assert_eq!(result.answers, vec![vec!["p(\"a\", 2).".to_string()]]);
    }

    /// The difference that must *survive* the fix (`bugs/005`, acceptance 3):
    /// naming a value is a request to see it, so a hand-written assignment
    /// makes the value visible. Under the 2026-08-17 shape the atom accounts
    /// for `V`, so it is visible in the column it occupies and the two
    /// spellings of this question converge — which is the widening working, not
    /// a loss: no information is dropped, only a distinction between spellings.
    /// Where the assignment's variable sits *outside* every atom the projection
    /// difference is still observable, and the case below is that one.
    #[test]
    fn a_hand_written_assignment_still_shows_its_value() {
        let result = run("p(\"a\", 2).\n?- V = 1 + 1, p(\"a\", V).").expect("runs");
        assert_eq!(result.answers, vec![vec!["p(\"a\", 2).".to_string()]]);
    }

    /// An assignment-bound variable no atom mentions: the atom cannot carry it,
    /// so the synthesized form keeps the column. This is what keeps A15's query
    /// exclusion load-bearing now that the case above converged.
    #[test]
    fn an_assignment_outside_every_atom_keeps_the_synthesized_form() {
        let result = run("p(\"a\").\n?- V = 1 + 1, p(\"a\").").expect("runs");
        assert_eq!(result.answers, vec![vec!["answer(2).".to_string()]]);
    }

    /// A *non-ground* computed argument still hoists, so the answer stays the
    /// synthesized form — folding is scoped to what it can evaluate, and this
    /// is the boundary.
    #[test]
    fn a_non_ground_computed_argument_still_hoists() {
        let result = run("n(1).\nn(2).\np(2).\n?- n(X), p(X + 1).").expect("runs");
        assert_eq!(result.answers, vec![vec!["answer(1).".to_string()]]);
    }

    #[test]
    fn errors_from_each_stage_propagate() {
        // Parse error.
        assert!(run("p(1)").is_err());
        // Lowering (safety) error.
        assert!(run("p(X) :- q(Y).").is_err());
        // Type error.
        assert!(run("age(\"a\", 30).\nage(\"b\", \"old\").").is_err());
    }

    // --- `-q` one-shot queries (spec §14, step 6) ---

    fn build(base: &str, queries: &[&str]) -> String {
        let queries: Vec<String> = queries.iter().map(|q| q.to_string()).collect();
        program_with_queries(base, &queries).expect("builds")
    }

    #[test]
    fn bare_atom_query_becomes_a_query_line() {
        assert_eq!(
            build("", &["ancestor(\"alice\", X)"]),
            "?- ancestor(\"alice\", X).\n"
        );
    }

    #[test]
    fn comma_body_query_becomes_one_query_line() {
        // A conjunction is a query body, not a valid statement, so it must fall
        // through to the `?- …` branch rather than being misclassified.
        assert_eq!(
            build("", &["adult(N), N != \"bob\""]),
            "?- adult(N), N != \"bob\".\n"
        );
    }

    #[test]
    fn define_and_select_rule_appends_rule_then_head_query() {
        // The rule text is kept verbatim; only the synthesized head query is
        // canonically printed (note the space after the comma).
        assert_eq!(
            build("", &["gp(X,Z) :- parent(X,Y), parent(Y,Z)"]),
            "gp(X,Z) :- parent(X,Y), parent(Y,Z).\n?- gp(X, Z).\n"
        );
    }

    #[test]
    fn a_disjunctive_rule_is_one_rule() {
        // `bugs/002`: the parser expands a top-level `;` into one clause per
        // disjunct sharing the head, so this arrives as *two* statements and
        // used to fall through to the query path — reporting a syntax error
        // about a grammar the user did not write.
        assert_eq!(
            build("", &["r(X) :- p(X), X < 5 ; p(X), s(X)"]),
            "r(X) :- p(X), X < 5 ; p(X), s(X).\n?- r(X).\n"
        );
        let result = run_with_queries(
            "p(1). p(9). s(9).",
            &["r(X) :- p(X), X < 5 ; p(X), s(X)".to_string()],
        )
        .expect("runs");
        assert_eq!(
            result.answers,
            vec![vec!["r(1).".to_string(), "r(9).".to_string()]]
        );
    }

    #[test]
    fn two_unrelated_rules_are_not_a_define_and_select() {
        // Outside the documented `-q` contract: heads differ, so this must not
        // silently select only `a`. It lands on the query-body path, where it is
        // a parse error attributed to the argument.
        let queries = ["a(X) :- p(X). b(X) :- p(X)".to_string()];
        assert!(program_with_queries("p(1).", &queries).is_err());
    }

    #[test]
    fn trailing_period_is_normalized() {
        assert_eq!(build("", &["p(X)."]), build("", &["p(X)"]));
    }

    #[test]
    fn base_program_is_preserved_and_newline_separated() {
        assert_eq!(build("p(1).", &["p(X)"]), "p(1).\n?- p(X).\n");
    }

    #[test]
    fn multiple_queries_apply_in_order() {
        let source = build(
            "",
            &["gp(X,Z) :- parent(X,Y), parent(Y,Z)", "gp(X, \"dave\")"],
        );
        assert_eq!(
            source,
            "gp(X,Z) :- parent(X,Y), parent(Y,Z).\n?- gp(X, Z).\n?- gp(X, \"dave\").\n"
        );
    }

    #[test]
    fn a_colon_dash_inside_a_string_is_not_a_rule() {
        // Classification parses the arg; it must not string-split on `:-`.
        assert_eq!(
            build("", &["note(\"build :- run\")"]),
            "?- note(\"build :- run\").\n"
        );
    }

    #[test]
    fn a_malformed_query_is_an_error() {
        let queries = vec!["p(".to_string()];
        assert!(program_with_queries("", &queries).is_err());
    }

    #[test]
    fn run_with_queries_answers_over_an_in_memory_base() {
        let base = "\
parent(\"alice\", \"bob\").
parent(\"bob\", \"carol\").
anc(X, Y) :- parent(X, Y).
anc(X, Y) :- parent(X, Z), anc(Z, Y).
";
        let result = run_with_queries(base, &["anc(\"alice\", Who)".to_string()]).expect("runs");
        assert_eq!(
            result.answers,
            vec![vec![
                "anc(\"alice\", \"bob\").".to_string(),
                "anc(\"alice\", \"carol\").".to_string(),
            ]]
        );
    }

    #[test]
    fn run_with_queries_evaluates_a_define_and_select_rule() {
        let base = "parent(\"a\", \"b\").\nparent(\"b\", \"c\").\n";
        let result = run_with_queries(base, &["gp(X,Z) :- parent(X,Y), parent(Y,Z)".to_string()])
            .expect("runs");
        assert_eq!(result.answers, vec![vec!["gp(\"a\", \"c\").".to_string()]]);
    }

    #[test]
    fn output_is_valid_input_closure() {
        // Run once, feed the printed answers back as a program, query again.
        let first = run("parent(\"a\", \"b\").\nparent(\"b\", \"c\").\nanc(X, Y) :- parent(X, Y).\nanc(X, Y) :- parent(X, Z), anc(Z, Y).\n?- anc(X, Y).")
            .expect("runs");
        let piped = first.output();
        // The printed `anc(...)` facts parse and load again.
        let second = run(&format!("{piped}?- anc(\"a\", Who).")).expect("re-runs");
        assert_eq!(
            second.answers,
            vec![vec![
                "anc(\"a\", \"b\").".to_string(),
                "anc(\"a\", \"c\").".to_string()
            ]]
        );
    }
}
