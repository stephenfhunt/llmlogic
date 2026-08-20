//! Integration tests: source text → query answers through the full public
//! pipeline (`datalog::run`), over the spec §16 corpus. These exercise the
//! library surface from outside the crate; the system tests
//! (`tests/system.rs`) run the compiled binary over the same programs.

use std::fs;

fn corpus(name: &str) -> String {
    fs::read_to_string(format!("tests/programs/{name}")).expect("corpus file exists")
}

fn answers(src: &str) -> Vec<String> {
    datalog::run(src)
        .unwrap_or_else(|e| panic!("run failed: {e:?}"))
        .answers
        .into_iter()
        .flatten()
        .collect()
}

#[test]
fn ancestry_16_1() {
    assert_eq!(
        answers(&corpus("16_1_ancestry.dl")),
        vec![
            "ancestor(\"alice\", \"bob\").",
            "ancestor(\"alice\", \"carol\").",
            "ancestor(\"alice\", \"dave\").",
        ]
    );
}

#[test]
fn negation_16_2() {
    assert_eq!(
        answers(&corpus("16_2_negation.dl")),
        vec!["root(\"alice\")."]
    );
}

#[test]
fn arithmetic_16_3() {
    assert_eq!(
        answers(&corpus("16_3_arithmetic.dl")),
        vec!["adult(\"alice\").", "adult(\"carol\")."]
    );
}

#[test]
fn aggregation_16_4() {
    assert_eq!(
        answers(&corpus("16_4_aggregation.dl")),
        vec!["child_count(\"alice\", 2).", "child_count(\"bob\", 1)."]
    );
}

#[test]
fn aggregation_skips_absent_and_typechecks() {
    // sum/avg skip absent inputs; min extends to strings; the whole program
    // (including typecheck) accepts the aggregates (§9).
    let src = "\
        measure(\"iron\", 5). measure(\"iron\", 3). measure(\"iron\", absent).\n\
        total(N, S) :- measure(N, _), S = sum { A | measure(N, A) }.\n\
        mean(N, M)  :- measure(N, _), M = avg { A | measure(N, A) }.\n\
        name(\"bob\"). name(\"alice\").\n\
        first(F) :- F = min { X | name(X) }.\n\
        ?- total(N, S).\n\
        ?- mean(N, M).\n\
        ?- first(F).";
    assert_eq!(
        answers(src),
        vec![
            "total(\"iron\", 8).",
            "mean(\"iron\", 4.0).",
            "first(\"alice\").",
        ]
    );
}

#[test]
fn count_counts_bindings_present_written_explicitly() {
    // count counts bindings (absent included); the present count is written
    // out with an explicit `is not absent` filter (§9).
    let src = "\
        m(\"a\", 1). m(\"a\", absent). m(\"a\", 2).\n\
        thing(\"a\").\n\
        bindings(T, C) :- thing(T), C = count { A | m(T, A) }.\n\
        present(T, C)  :- thing(T), C = count { A | m(T, A), A is not absent }.\n\
        ?- bindings(T, C).\n\
        ?- present(T, C).";
    assert_eq!(
        answers(src),
        vec!["bindings(\"a\", 3).", "present(\"a\", 2)."]
    );
}

/// An aggregate in a **query** answers over the variables the query body binds
/// — not over every named slot. An aggregate's goal-local variables are named
/// but exist only inside its sub-join (§9), so `?- N = count { C | m(T, C) }.`
/// answers over `N` alone. (Projecting them used to leave an unbound slot in the
/// answer row.)
#[test]
fn an_aggregate_in_a_query_projects_only_body_bound_variables() {
    let facts = "m(\"a\", 1). m(\"a\", 2). m(\"b\", 9).\nthing(\"a\"). thing(\"b\").\n";

    // Goal-local `T` and `C`: one global count, projected over `N` alone.
    assert_eq!(
        answers(&format!("{facts}?- N = count {{ C | m(T, C) }}.")),
        vec!["answer(3)."]
    );
    // `T` bound outside the aggregate is a group key and *is* projected.
    assert_eq!(
        answers(&format!("{facts}?- thing(T), N = count {{ C | m(T, C) }}.")),
        vec!["answer(\"a\", 2).", "answer(\"b\", 1)."]
    );
    // Composing under §8: the aggregate inside arithmetic, and as a filter.
    assert_eq!(
        answers(&format!("{facts}?- N = count {{ C | m(T, C) }} + 1.")),
        vec!["answer(4)."]
    );
    // As a *filter* the aggregate binds only a fresh unnamed slot, so the atom
    // still accounts for every answer variable and the substituted form holds
    // (§17, 2026-08-17) — unlike the group-key case above, where `N` is named
    // and no atom carries it.
    assert_eq!(
        answers(&format!("{facts}?- thing(T), count {{ C | m(T, C) }} > 1.")),
        vec!["thing(\"a\")."]
    );
}

/// An aggregate's goal is a body and binds like one: a variable bound inside it
/// by an `=`-assignment or by a *nested* aggregate is available to the collected
/// expression. Lowering hoists a nested aggregate into the goal for exactly this
/// reason, so the safety check must use the same notion of "bound".
#[test]
fn an_aggregate_goal_binds_assignments_and_nested_aggregates() {
    let facts = "s(\"a\", 1). s(\"a\", 2). s(\"b\", 10).\n";

    // `T` bound by an `=`-assignment inside the goal.
    assert_eq!(
        answers(&format!(
            "{facts}doubled(K, M) :- s(K, _), M = max {{ T | s(K, V), T = V * 2 }}.\n\
             ?- doubled(K, M)."
        )),
        vec!["doubled(\"a\", 4).", "doubled(\"b\", 20)."]
    );
    // `T` bound by a nested aggregate: the largest per-key sum (a=3, b=10).
    assert_eq!(
        answers(&format!(
            "{facts}?- M = max {{ T | s(K, _), T = sum {{ V | s(K, V) }} }}."
        )),
        vec!["answer(10)."]
    );
}

/// §9's skip-but-report rule: `sum`/`avg`/`min`/`max` drop `absent` inputs, and
/// the drop is *reported* — read back out of the recorded provenance, one
/// warning per aggregate site. Until `?why` (§11) lands this is the only signal
/// that an answer covers less data than it appears to.
#[test]
fn skipped_absents_are_reported_as_warnings() {
    let result = datalog::run(
        "m(\"a\", 5). m(\"a\", absent). m(\"b\", 7).\n\
         thing(\"a\"). thing(\"b\").\n\
         total(T, S) :- thing(T), S = sum { A | m(T, A) }.\n\
         seen(T, N)  :- thing(T), N = count { A | m(T, A) }.\n\
         ?- total(T, S).",
    )
    .expect("runs");
    let warnings: Vec<String> = result.warnings.iter().map(|w| w.to_string()).collect();
    assert_eq!(warnings.len(), 1, "one site skipped: {warnings:?}");
    assert!(
        warnings[0].contains("`sum` in `total/2`")
            && warnings[0].contains("skipped 1 absent input(s)"),
        "unexpected warning: {}",
        warnings[0]
    );

    // No absent, no warning — and `count` never skips, so it never warns.
    let clean = datalog::run(
        "m(\"a\", 5).\nthing(\"a\").\n\
         total(T, S) :- thing(T), S = sum { A | m(T, A) }.\n?- total(T, S).",
    )
    .expect("runs");
    assert!(clean.warnings.is_empty(), "{:?}", clean.warnings);
}

#[test]
fn named_arguments_16_7() {
    assert_eq!(
        answers(&corpus("16_7_named_args.dl")),
        vec!["adult(\"alice\")."]
    );
}

/// §13 module imports: the corpus program splices `modules/family.dl` —
/// resolved relative to the program file, while this test runs with the crate
/// root as working directory.
#[test]
fn module_import_splices_a_library() {
    let path = std::path::Path::new("tests/programs/modules_import.dl");
    let source = fs::read_to_string(path).expect("corpus file exists");
    let result = datalog::run_at(&source, Some(path)).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(
        answers,
        vec![
            "ancestor(\"alice\", \"bob\").",
            "ancestor(\"alice\", \"carol\").",
        ]
    );
}

/// A pathless program (stdin/`-q` shape) resolves module imports against the
/// working directory — for tests, the crate root.
#[test]
fn pathless_module_import_resolves_against_the_working_directory() {
    let src = "import \"tests/programs/modules/family.dl\".\n?- parent(X, Y).\n";
    let result = datalog::run(src).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(answers.len(), 2);
}

/// §16.5 — a CSV import evaluates: its tuples are base facts feeding the
/// recursive ancestry rules. Resolved relative to the program file, so the
/// `data/parents.csv` path works from any working directory.
#[cfg(feature = "duckdb")]
#[test]
fn csv_import_evaluates_16_5() {
    let path = std::path::Path::new("tests/programs/16_5_import.dl");
    let source = fs::read_to_string(path).expect("corpus file exists");
    let result = datalog::run_at(&source, Some(path)).unwrap_or_else(|e| panic!("run: {e:?}"));
    let answers: Vec<String> = result.answers.into_iter().flatten().collect();
    assert_eq!(
        answers,
        vec![
            "ancestor(\"alice\", \"bob\").",
            "ancestor(\"alice\", \"carol\").",
            "ancestor(\"alice\", \"dave\").",
        ]
    );
}

/// Under `--no-default-features` there is no reader, so a data import is a
/// structured error naming the feature (not a silent empty result).
#[cfg(not(feature = "duckdb"))]
#[test]
fn csv_import_without_the_reader_feature_is_a_structured_error() {
    let path = std::path::Path::new("tests/programs/16_5_import.dl");
    let source = fs::read_to_string(path).expect("corpus file exists");
    let errors = datalog::run_at(&source, Some(path)).expect_err("no reader");
    assert!(
        errors.iter().any(|e| e.to_string().contains("duckdb")),
        "got {errors:?}"
    );
}

#[test]
fn a_malformed_program_reports_multiple_errors_in_one_run() {
    let errors = datalog::run(&corpus("broken_multi.dl")).expect_err("malformed");
    assert!(errors.len() >= 3, "expected several errors, got {errors:?}");
}

#[test]
fn features_program_output_shapes() {
    // One program exercising several end-to-end gaps at once: inline
    // arithmetic in a head arg, float + symbol value formatting, a filtered
    // query, and two queries in one program.
    let result = datalog::run(&corpus("features.dl")).expect("runs");
    assert_eq!(
        result.answers,
        vec![
            // Query 1 (single atom): substituted, floats keep their decimal
            // point, symbols print bare.
            vec!["scaled(a, 3.0).".to_string(), "scaled(b, 6.0).".to_string()],
            // Query 2 (atom + a non-binding filter): also substituted, under
            // the source relation's name (§17, 2026-08-17).
            vec!["measure(b, 3.0).".to_string()],
        ]
    );
}

#[test]
fn disjunction_end_to_end() {
    assert_eq!(
        answers(&corpus("disjunction.dl")),
        vec!["drinks(alice).", "drinks(bob)."]
    );
}

#[test]
fn a_type_error_is_reported() {
    let errors = datalog::run(&corpus("broken_types.dl")).expect_err("heterogeneous column");
    assert!(
        errors.iter().any(|e| e.to_string().contains("type error")),
        "got {errors:?}"
    );
}

#[test]
fn ordered_comparison_uses_every_types_natural_order() {
    // §8: ordered comparisons use the operand type's natural order — strings and
    // symbols lexicographically, `false < true` — the same order `min`/`max`
    // fold with. `bugs/006`: the typechecker required int or float, so `<` was
    // usable on two of the five primitives while `min` ordered all five.
    assert_eq!(
        answers("s(\"a\"). s(\"b\").\n?- s(X), s(Y), X < Y."),
        vec!["answer(\"a\", \"b\")."]
    );
    assert_eq!(
        answers("y(alpha). y(beta).\n?- y(X), y(Y), X < Y."),
        vec!["answer(alpha, beta)."]
    );
    assert_eq!(
        answers("b(true). b(false).\n?- b(X), b(Y), X < Y."),
        vec!["answer(false, true)."]
    );
    // The canonicalisation idiom the defect made unwritable over string keys:
    // an unordered pair counted once (`bugs/006`, "What it cost").
    assert_eq!(
        answers(
            "e(\"x\", \"y\"). e(\"y\", \"x\"). e(\"x\", \"z\").\n\
             pair(A, B) :- e(A, B), A < B.\n?- pair(A, B)."
        ),
        vec!["pair(\"x\", \"y\").", "pair(\"x\", \"z\")."]
    );
}

#[test]
fn a_cross_type_ordered_comparison_is_still_a_type_error() {
    // Widening `<` past int/float left the same-type rule alone: `union(l, r)`
    // is what rejects this, and it is independent of the numeric constraint
    // that `bugs/006` removed.
    let errors =
        datalog::run("s(\"a\").\n?- s(X), X < 1.").expect_err("string compared against int");
    assert!(
        errors.iter().any(|e| e.to_string().contains("type error")),
        "got {errors:?}"
    );
}

#[test]
fn a_runtime_arithmetic_error_is_reported() {
    let errors = datalog::run(&corpus("broken_arith.dl")).expect_err("division by zero");
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("division by zero")),
        "got {errors:?}"
    );
}

#[test]
fn output_is_valid_input_end_to_end() {
    // Materialize 16.1's query result (alice's ancestors), then query those
    // materialized facts again — the printed facts parse and load as input.
    let first = datalog::run(&corpus("16_1_ancestry.dl")).expect("runs");
    let materialized = first.output();
    let composed = format!("{materialized}?- ancestor(Who, \"dave\").");
    let second = datalog::run(&composed).expect("re-runs over its own output");
    // Only alice's ancestors were materialized, so alice is the only match.
    assert_eq!(
        second.answers.into_iter().flatten().collect::<Vec<_>>(),
        vec!["ancestor(\"alice\", \"dave\")."]
    );
}

/// An aggregate written in a **query** reports its skipped absents (§9). This
/// was the one shape the skip report could not see — a query records no
/// derivations, and the report was read out of them — so the count existed and
/// nothing read it. The site is the query's 1-based position, since an unnamed
/// query has no name to use (a named one lowers to a rule and reports as one).
#[test]
fn a_query_level_aggregate_reports_its_skipped_absents() {
    let result = datalog::run(
        "m(\"a\", 5). m(\"a\", absent).\n\
         ?- S = sum { A | m(\"a\", A) }.",
    )
    .expect("runs");
    let warnings: Vec<String> = result.warnings.iter().map(|w| w.to_string()).collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("`sum` in query 1") && warnings[0].contains("skipped 1 absent"),
        "{}",
        warnings[0]
    );
}

/// A conversion that fails on data reports **malformed, not missing** (§12).
/// The value model cannot carry the distinction — both are `absent` (§4) — so
/// without this a dirty column is indistinguishable from a sparse one.
#[test]
fn a_conversion_that_loses_a_value_reports_it_as_malformed() {
    let result = datalog::run(
        "raw(\"12\"). raw(\"abc\"). raw(\"30\").\n\
         num(V) :- raw(X), V = X as int.\n?- num(V).",
    )
    .expect("runs");
    let warnings: Vec<String> = result.warnings.iter().map(|w| w.to_string()).collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("`as int` in `num/1`")
            && warnings[0].contains("1 value(s)")
            && warnings[0].contains("malformed, not missing"),
        "{}",
        warnings[0]
    );
}

/// An `absent` that arrived as **data** is missing, not malformed: nothing was
/// lost, so the conversion report stays silent and only §9's skip speaks. This
/// is the assertion that keeps the two reports from collapsing into one.
#[test]
fn an_absent_that_was_never_a_value_is_not_reported_as_malformed() {
    let result = datalog::run(
        "raw(\"12\"). raw(absent).\n\
         num(V) :- raw(X), V = X as int.\n?- num(V).",
    )
    .expect("runs");
    assert!(
        result.warnings.is_empty(),
        "absent in, absent out is annihilation, not loss: {:?}",
        result.warnings
    );
}

/// A **guarded** conversion is silent: `V = X as int, V is absent` is §16.9's
/// recommended idiom for a mostly-numeric column, and the program asking the
/// question is precisely the case a warning must not fire on. A diagnostic that
/// fires on correct programs is the hazard `notes/tsdl-cross-project-review.md`
/// measured, and this test is what stops this one becoming it.
#[test]
fn a_guarded_conversion_is_not_warned_about() {
    let result = datalog::run(
        "raw(\"12\"). raw(\"abc\").\n\
         unparsed(X) :- raw(X), V = X as int, V is absent.\n\
         amount(V)   :- raw(X), V = X as int, V is not absent.\n\
         ?- unparsed(X).",
    )
    .expect("runs");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

// --- Temporal values (§4/§8, `notes/temporal-values.md`) ---

use datalog::temporal::{Date, Duration};
use proptest::prelude::*;
use proptest::strategy::ValueTree;

/// The one error a run reports, as text.
fn error_text(src: &str) -> String {
    let errors = datalog::run(src).expect_err("this program should not type-check");
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn arb_date() -> impl Strategy<Value = Date> {
    (1900i64..2200, 1i64..=12, 1i64..=28)
        .prop_map(|(y, m, d)| Date::from_ymd(y, m, d).expect("day 1-28 exists in every month"))
}

proptest! {
    /// **T2** — the affine laws, run through the whole pipeline: a point moved
    /// by a displacement and back is where it started, and the displacement
    /// between two points moves one onto the other (§8).
    #[test]
    fn t2_affine_laws_hold(start in arb_date(), days in -3000i64..3000, end in arb_date()) {
        let shift = Duration::from_micros(days * Date::DAY);
        let src = format!(
            "d(@{start}). k(@{shift}). e(@{end}).\n\
             there_and_back(X) :- d(D), k(K), X = (D + K) - D.\n\
             onto(X)           :- d(D), e(E), X = D + (E - D).\n\
             ?- there_and_back(X).\n?- onto(X)."
        );
        let out = answers(&src);
        prop_assert_eq!(
            out,
            vec![
                format!("there_and_back(@{shift})."),
                format!("onto(@{end})."),
            ]
        );
    }

    /// **T3** — the unit is the divisor. `(D2 - D1) / @1d` is the civil day
    /// difference, computed here from the day counts themselves rather than by
    /// a second traversal of the same arithmetic.
    ///
    /// This is the property that pins the sibling engine's `172800000` finding:
    /// there is no spelling of "as a number" that does not name a unit.
    #[test]
    fn t3_the_divisor_names_the_unit(a in arb_date(), b in arb_date()) {
        let gap = (b.days() - a.days()) as f64;
        // Two divisors, and the second is why the result type is `float`: a
        // day gap over 36 hours is fractional, so a truncating `int` here
        // would lose it silently — the very shape the divisor form exists to
        // prevent. Both oracles come from the day counts, not from a second
        // pass over the engine's own arithmetic.
        let src = format!(
            "a(@{a}). b(@{b}).\n\
             days(N)   :- a(A), b(B), N = (B - A) / @1d.\n\
             shifts(N) :- a(A), b(B), N = (B - A) / @36h.\n\
             ?- days(N).\n?- shifts(N)."
        );
        prop_assert_eq!(
            answers(&src),
            vec![
                format!("days({gap:?})."),
                format!("shifts({:?}).", gap * 24.0 / 36.0),
            ]
        );
    }
}

/// The non-vacuity half of T3: the generator must actually produce differences
/// of both signs and a nonzero one, or the property is a claim about `0.0`.
#[test]
fn t3_generator_reaches_both_signs() {
    let mut saw = (false, false, false);
    let mut runner = proptest::test_runner::TestRunner::default();
    for _ in 0..200 {
        let (a, b) = (arb_date(), arb_date())
            .new_tree(&mut runner)
            .expect("generates")
            .current();
        let gap = b.days() - a.days();
        saw.0 |= gap < 0;
        saw.1 |= gap == 0;
        saw.2 |= gap > 0;
    }
    assert!(
        saw.0 && saw.2,
        "the date generator never produced both signs"
    );
}

#[test]
fn a_duration_divided_by_a_duration_keeps_the_half_day() {
    // Float, not int: `@36h / @1d` is 1.5, and truncating it would be the
    // same silent loss the divisor form exists to prevent (§8).
    assert_eq!(
        answers("r(N) :- N = @36h / @1d.\n?- r(N)."),
        vec!["r(1.5).".to_string()]
    );
}

#[test]
fn adding_two_points_says_why_it_has_no_meaning() {
    let text = error_text("d(@2026-08-19).\nr(X) :- d(D), X = D + D.\n?- r(X).");
    assert!(text.contains("two points in time do not add"), "{text}");
    assert!(text.contains("subtract them"), "{text}");
}

#[test]
fn a_duration_and_a_number_do_not_add() {
    let text = error_text("r(X) :- X = @1d + 1.\n?- r(X).");
    assert!(text.contains("no unit"), "{text}");
    assert!(text.contains("/ @1d"), "{text}");
}

#[test]
fn a_duration_never_converts_to_a_number() {
    // The `172800000` bug, excluded by construction: the message names the
    // divisor form rather than explaining a unit.
    let text = error_text("r(N) :- N = @1d as int.\n?- r(N).");
    assert!(
        text.contains("no conversion between duration and int"),
        "{text}"
    );
    assert!(text.contains("name the unit"), "{text}");
}

#[test]
fn a_date_and_a_timestamp_do_not_mix() {
    let text = error_text(
        "d(@2026-08-19). t(@2026-08-19T10:30:00).\n\
         r(X) :- d(D), t(T), X = T - D.\n?- r(X).",
    );
    assert!(text.contains("mixes a date and a timestamp"), "{text}");
    assert!(text.contains("as timestamp"), "{text}");
}

#[test]
fn truncating_a_timestamp_names_the_construct_that_does_it() {
    // §8 resolves this tension in favour of the cast rule, so the error has to
    // carry the alternative — otherwise the refusal is a dead end.
    let text = error_text("t(@2026-08-19T10:30:00).\nr(D) :- t(T), D = T as date.\n?- r(D).");
    assert!(text.contains("does not truncate"), "{text}");
    assert!(text.contains("truncate(T, day, D)"), "{text}");
}

#[test]
fn a_date_shifts_only_by_whole_days() {
    // A date has day precision; rounding a sub-day shift would be the
    // truncation `as` refuses everywhere else.
    let text = error_text("d(@2026-08-19).\nr(X) :- d(D), X = D + @36h.\n?- r(X).");
    assert!(text.contains("not a whole number of days"), "{text}");
    assert!(text.contains("as timestamp"), "{text}");
    // A timestamp has room for the remainder, so the widened form works.
    assert_eq!(
        answers("d(@2026-08-19).\nr(X) :- d(D), X = (D as timestamp) + @36h.\n?- r(X)."),
        vec!["r(@2026-08-20T12:00:00).".to_string()]
    );
}

#[test]
fn durations_reduce_and_points_do_not() {
    // `avg` over durations is a duration, because dividing one by a count is
    // scaling — the vector rule's other half, not an exception to it (§9).
    assert_eq!(
        answers(
            "took(a, @1h). took(b, @2h).\n\
             stats(S, A) :- S = sum { D | took(_, D) }, A = avg { D | took(_, D) }.\n\
             ?- stats(S, A)."
        ),
        vec!["stats(@3h, @1h30m).".to_string()]
    );
    let text = error_text(
        "on(a, @2026-08-19). on(b, @2026-08-20).\n\
         r(S) :- S = sum { D | on(_, D) }.\n?- r(S).",
    );
    assert!(text.contains("two points"), "{text}");
    assert!(text.contains("min`/`max"), "{text}");
}

#[test]
fn temporal_values_order_and_compare_within_their_type() {
    assert_eq!(
        answers(
            "d(@2026-08-19). d(@2026-06-01). d(@2026-12-25).\n\
             early(D) :- d(D), D < @2026-08-01.\n\
             span(S)  :- S = max { D | d(D) } - min { D | d(D) }.\n\
             ?- early(D).\n?- span(S)."
        ),
        vec![
            "early(@2026-06-01).".to_string(),
            "span(@207d).".to_string()
        ]
    );
}

/// Absent annihilates temporal arithmetic exactly as it does numeric (§4/§8) —
/// the choke point is shared, and this is what pins that it stays shared.
#[test]
fn absent_annihilates_temporal_arithmetic_too() {
    assert_eq!(
        answers(
            "d(@2026-08-19). d(absent).\n\
             next(X) :- d(D), X = D + @1d.\n?- next(X)."
        ),
        // `absent` sorts first in the canonical output order (§4/§14).
        vec![
            "next(absent).".to_string(),
            "next(@2026-08-20).".to_string()
        ]
    );
}
