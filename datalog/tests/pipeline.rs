//! Integration tests: source text → query answers through the full public
//! pipeline (`datalog::run`), over the spec §16 corpus. These exercise the
//! library surface from outside the crate; the system tests
//! (`tests/system.rs`) run the compiled binary over the same programs.

use datalog::ErrorCode;
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
        errors
            .iter()
            .any(|e| e.code == ErrorCode::TypeClash || e.code == ErrorCode::TypeMismatch),
        "got {errors:?}"
    );
}

/// `bugs/009` — a column-level type clash must name **both** occurrences.
///
/// The variable form already does (`variable Q ... but variable W ...`); the
/// column form names one label and a span belonging to whichever occurrence
/// arrived second, so neither side of the conflict is locatable. §12: "the
/// position says where to look for it, which is what stops the message
/// degrading with program length."
///
/// `#[ignore]`d and failing until `set_type` carries the site that fixed a
/// class's type, in the style of `bugs/002`'s criterion (`src/api.rs:1182`).
#[test]
#[ignore = "bugs/009: set_type records the type without the site that fixed it"]
fn a_column_type_clash_names_both_occurrences() {
    let errors = datalog::run("p(alice).\n?- p(\"alice\").\n").expect_err("symbol vs string");
    let clash = errors
        .iter()
        .find(|e| e.code == ErrorCode::TypeClash)
        .expect("a type clash");
    let rendered = clash.to_string();
    // Deliberately not pinning a layout — the fix chooses one. Both sides have
    // to be *findable*: the earlier occurrence's line, and the term as written,
    // which is what says how the token was read.
    assert!(
        rendered.contains("1:"),
        "the fact that made column 0 a symbol is on line 1, unnamed in: {rendered}"
    );
    assert!(
        rendered.contains("alice"),
        "neither term appears, so nothing says how each was read: {rendered}"
    );
}

/// `bugs/009`'s **general property**: every `type-clash` diagnostic locates both
/// sides of the conflict, whichever form produced it.
///
/// Filed with the bug rather than after it, per `bugs/README.md` — `001` was
/// still reachable by a second spelling because its property was named and never
/// written. The variable form satisfies this today only because `union` happens
/// to hold two labelled slots, and nothing pins that; the column form fails, and
/// the two-fact case below carries no span at all.
#[test]
#[ignore = "bugs/009: the column form locates at most one side"]
fn every_type_clash_locates_both_sides() {
    let programs = [
        // column form: symbol against string
        "p(alice).\n?- p(\"alice\").\n",
        // column form, two facts: today this renders with no position whatsoever
        "p(1).\np(\"x\").\n?- p(X).\n",
        // variable form: passes today, and is here so it stays passing
        "declare item(name: string, qty: int, weight: float).\n\
         item(\"bolt\", 4, 1.5).\n\
         total(N) :- item(qty: Q, weight: W), N = Q + W.\n\
         ?- total(N).\n",
    ];
    for program in programs {
        let errors = datalog::run(program).expect_err("a type clash");
        let clash = errors
            .iter()
            .find(|e| e.code == ErrorCode::TypeClash)
            .unwrap_or_else(|| panic!("no type clash for:\n{program}"));
        let rendered = clash.to_string();
        let located = rendered.matches(" at ").count() + rendered.matches("(at ").count();
        assert!(
            located >= 2,
            "only {located} side(s) located for:\n{program}rendered: {rendered}"
        );
    }
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
        errors
            .iter()
            .any(|e| e.code == ErrorCode::TypeClash || e.code == ErrorCode::TypeMismatch),
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

proptest! {
    /// **T4** — ordering agreement, the migration-safety oracle. ISO-8601 text
    /// sorts lexicographically, which is why date *filtering* already worked
    /// before this feature existed (`bugs/006` widened `<` to the types §8
    /// orders). So the old behaviour is a free reference implementation: for
    /// any two dates, comparing them as dates must give what comparing their
    /// ISO text gives.
    ///
    /// It is the property that says this feature took nothing away.
    #[test]
    fn t4_date_order_agrees_with_the_text_order_it_replaces(a in arb_date(), b in arb_date()) {
        let src = format!(
            "d(@{a}, @{b}). t(\"{a}\", \"{b}\").\n\
             as_date(true) :- d(A, B), A < B.\n\
             as_text(true) :- t(A, B), A < B.\n\
             ?- as_date(true).\n?- as_text(true)."
        );
        let out = answers(&src);
        prop_assert_eq!(
            out.iter().filter(|line| line.starts_with("as_date")).count(),
            out.iter().filter(|line| line.starts_with("as_text")).count(),
            "date and text order disagreed on {} < {}", a, b
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

// --- `std/time` (§13) ---

proptest! {
    /// **T5** — truncation is **idempotent** (truncating a period start leaves
    /// it alone) and **monotone** (it never reorders two points), which is what
    /// makes a truncated value usable as a group key at all: a key that moved
    /// under re-truncation would double-count, and one that reordered would
    /// sort wrong.
    #[test]
    fn t5_truncation_is_idempotent_and_monotone(
        a in arb_date(),
        b in arb_date(),
        unit in proptest::sample::select(vec!["year", "quarter", "month", "week", "day"]),
    ) {
        let src = format!(
            "import \"std/time\".\n\
             a(@{a}). b(@{b}).\n\
             once(K)  :- a(D), truncate(D, {unit}, K).\n\
             twice(K) :- once(P), truncate(P, {unit}, K).\n\
             ordered(true) :- a(X), b(Y), X <= Y,\n\
                              truncate(X, {unit}, KX), truncate(Y, {unit}, KY), KX <= KY.\n\
             ordered(true) :- a(X), b(Y), Y < X.\n\
             ?- once(K).\n?- twice(K).\n?- ordered(true)."
        );
        let out = answers(&src);
        prop_assert_eq!(&out[0].replace("once", "twice"), &out[1], "not idempotent");
        prop_assert_eq!(out.len(), 3, "monotonicity found no witness: {:?}", out);
    }

    /// **T5**, the extraction half: `year`/`month`/`day` agree with the
    /// components of the value's own canonical text (§14), which is the only
    /// independent statement of what those components are.
    #[test]
    fn t5_extraction_agrees_with_the_printed_form(date in arb_date()) {
        let text = date.to_string();
        let src = format!(
            "import \"std/time\".\n\
             d(@{date}).\n\
             parts(Y, M, N) :- d(D), year(D, Y), month(D, M), day(D, N).\n?- parts(Y, M, N)."
        );
        let (y, rest) = text.split_at(4);
        prop_assert_eq!(
            answers(&src),
            vec![format!(
                "parts({}, {}, {}).",
                y.parse::<i64>().expect("year digits"),
                rest[1..3].parse::<i64>().expect("month digits"),
                rest[4..6].parse::<i64>().expect("day digits"),
            )]
        );
    }
}

#[test]
fn truncation_keeps_the_type_it_was_given() {
    // The result type follows the input's, so a group key never changes type
    // with the unit (§13) — a date truncates to a date, a timestamp to a
    // timestamp at midnight.
    assert_eq!(
        answers(
            "import \"std/time\".\n\
             d(@2026-08-19). t(@2026-08-19T10:30:00).\n\
             dk(K) :- d(D), truncate(D, month, K).\n\
             tk(K) :- t(T), truncate(T, day, K).\n\
             hk(K) :- t(T), truncate(T, hour, K).\n\
             ?- dk(K).\n?- tk(K).\n?- hk(K)."
        ),
        vec![
            "dk(@2026-08-01).".to_string(),
            "tk(@2026-08-19T00:00:00).".to_string(),
            "hk(@2026-08-19T10:00:00).".to_string(),
        ]
    );
}

/// The week is ISO's — Monday-based. T5 deliberately does not pin this: a
/// Sunday-based week is just as idempotent and just as monotone, so the
/// property cannot see the difference and an example has to.
#[test]
fn a_week_starts_on_monday() {
    assert_eq!(
        answers(
            "import \"std/time\".\n\
             d(@2026-08-19). d(@2026-08-17). d(@2026-08-16).\n\
             w(D, K) :- d(D), truncate(D, week, K).\n?- w(D, K)."
        ),
        vec![
            // Sunday the 16th belongs to the week that began on the 10th.
            "w(@2026-08-16, @2026-08-10).".to_string(),
            "w(@2026-08-17, @2026-08-17).".to_string(),
            "w(@2026-08-19, @2026-08-17).".to_string(),
        ]
    );
}

#[test]
fn a_std_relation_is_a_filter_when_its_output_is_a_constant() {
    // `day(D, 15)` needs no separate form: the `=` rule already decides
    // assignment-vs-filter, which is why lowering can spell a builtin as one.
    assert_eq!(
        answers(
            "import \"std/time\".\n\
             d(@2026-08-15). d(@2026-08-19).\n\
             mid(D) :- d(D), day(D, 15).\n?- mid(D)."
        ),
        vec!["mid(@2026-08-15).".to_string()]
    );
}

#[test]
fn a_truncation_unit_is_checked_once_with_the_list_attached() {
    let text = error_text(
        "import \"std/time\".\nd(@2026-08-19).\nr(K) :- d(D), truncate(D, fortnight, K).\n?- r(K).",
    );
    assert!(
        text.contains("`fortnight` is not a truncation unit"),
        "{text}"
    );
    assert!(text.contains("quarter"), "{text}");
    // A computed unit is refused too: the check is static on purpose.
    let text = error_text(
        "import \"std/time\".\nd(@2026-08-19). u(month).\n\
         r(K) :- d(D), u(U), truncate(D, U, K).\n?- r(K).",
    );
    assert!(text.contains("must be written as a symbol"), "{text}");
}

#[test]
fn asking_a_date_for_a_time_of_day_is_a_type_error() {
    // §4 gives a date no time of day, so a silent zero would read as midnight
    // — a plausible wrong answer, which is the class this engine refuses.
    let text =
        error_text("import \"std/time\".\nd(@2026-08-19).\nr(H) :- d(D), hour(D, H).\n?- r(H).");
    assert!(text.contains("a date has no time of day"), "{text}");
    assert!(text.contains("as timestamp"), "{text}");
}

#[test]
fn a_std_relation_annihilates_on_absent_like_every_other_computation() {
    assert_eq!(
        answers(
            "import \"std/time\".\n\
             d(@2026-08-19). d(absent).\n\
             y(Y) :- d(D), year(D, Y).\n?- y(Y)."
        ),
        vec!["y(absent).".to_string(), "y(2026).".to_string()]
    );
}

#[test]
fn negating_a_std_relation_points_at_the_comparison_form() {
    let text = error_text(
        "import \"std/time\".\nd(@2026-08-19).\nr(D) :- d(D), not day(D, 15).\n?- r(D).",
    );
    assert!(text.contains("`not` applies to relations"), "{text}");
    assert!(text.contains("X != "), "{text}");
}

/// `bugs/resolved/008`: a rule-level type clash between two **declared**
/// columns used to manufacture a *second*, false diagnostic asserting the fact
/// table held values of a type it does not hold. It named the opposite column
/// from the one at fault, so an agent following it edited correct data.
///
/// Only the rule is wrong here: `item.weight` is declared float and its one
/// value is `1.5`.
#[test]
fn a_rule_type_clash_accuses_only_the_rule() {
    let src = "declare item(name: string, qty: int, weight: float).\n\
               item(\"bolt\", 4, 1.5).\n\
               total(N) :- item(qty: Q, weight: W), N = Q + W.\n";
    let errors = datalog::run(src).expect_err("int plus float is a clash");
    let rendered: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    assert_eq!(
        rendered.len(),
        1,
        "expected one diagnostic, got {rendered:?}"
    );
    assert!(
        rendered[0].contains("`Q`") && rendered[0].contains("`W`"),
        "{rendered:?}"
    );
    // Neither column is accused: the declarations and the data agree.
    for message in &rendered {
        assert!(!message.contains("item.weight"), "{message}");
        assert!(!message.contains("item.qty"), "{message}");
    }
}

/// The same clash written the other way round. The false accusation used to
/// follow operand order — swapping the addition flipped which column was
/// blamed, though neither column's values had changed. That tell is what
/// identified the merged union-find class as the source.
#[test]
fn a_rule_type_clash_reads_the_same_in_either_operand_order() {
    let one = "declare item(name: string, qty: int, weight: float).\n\
               item(\"bolt\", 4, 1.5).\n\
               total(N) :- item(qty: Q, weight: W), N = Q + W.\n";
    let other = "declare item(name: string, qty: int, weight: float).\n\
                 item(\"bolt\", 4, 1.5).\n\
                 total(N) :- item(qty: Q, weight: W), N = W + Q.\n";
    let count = |src| datalog::run(src).expect_err("a clash either way").len();
    assert_eq!(count(one), 1);
    assert_eq!(count(other), 1);
}

/// The genuine case still reports, and still says **values**: `item.weight` is
/// declared float, the fact table holds an int, and nothing else is wrong. This
/// is the diagnostic the suppression above must not cost.
#[test]
fn a_declared_type_the_facts_contradict_still_names_the_values() {
    let errors = datalog::run("declare item(name: string, weight: float).\nitem(\"bolt\", 2).\n")
        .expect_err("a float column holding an int");
    let rendered: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    assert!(
        rendered
            .iter()
            .any(|message| message
                .contains("`item.weight` is declared as float but its values are int")),
        "{rendered:?}"
    );
}

/// The second half of `bugs/resolved/008`, which its own acceptance criteria
/// did not cover: inference reaches a column from rules as well as from facts,
/// and only the latter is a claim about data. `p` holds **no facts at all**, so
/// "its values are" would be a statement about a table nothing scanned.
#[test]
fn a_contradiction_inference_derived_from_a_rule_does_not_claim_values() {
    let errors = datalog::run("declare p(x: int).\ns(\"a\").\nr(S) :- p(x: S), s(S).\n")
        .expect_err("a rule using a declared int column as a string");
    let rendered: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    assert!(
        rendered
            .iter()
            .any(|message| message.contains("`p.x` is declared as int but is used as string")),
        "{rendered:?}"
    );
    for message in &rendered {
        assert!(!message.contains("its values are"), "{message}");
    }
}
