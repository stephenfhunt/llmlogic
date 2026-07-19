//! Test-only generators for property-based testing (proptest strategies).
//!
//! This module is coverage machinery, not a test DSL: strategies construct
//! plain [`crate::ast`] / [`crate::ir`] values and return them. Hand-written
//! example tests must keep constructing literal structs verbatim (AGENTS.md;
//! `testing.md`); nothing here is for human ergonomics.
//!
//! Design rules (see `testing.md`):
//! - **Valid by construction, no rejection sampling** — rules are generated
//!   body-first and head variables are drawn from the body's variables by
//!   index, so every program from [`arb_safe_program`] is safe and
//!   arity-consistent by construction.
//! - **Small-biased, collision-rich pools** — few symbols, tiny int range —
//!   so generated joins actually join instead of every relation being empty.
//! - **Index-vector selection + `prop_map`** (not `prop_flat_map` over
//!   materialized sets) keeps proptest's integrated shrinking effective:
//!   specs shrink toward fewer/smaller parts and the mapping stays total.
//!
//! Defect injection ([`arb_defect`], [`inject_defect`]) appends exactly one
//! self-contained defective statement over fresh `defect_*` predicates, so the
//! expected error is unambiguous regardless of the surrounding program.

use proptest::prelude::*;

use crate::ast::fixtures::{fact, positional_atom, positive_literal, rule, var_term};
use crate::ast::{Constant, Program, Span, Statement, Term, TermKind};
use crate::ir;

/// Maximum predicate arity the generators produce.
const MAX_ARITY: usize = 3;

/// A constant from small, collision-rich pools (never NaN).
pub(crate) fn arb_constant() -> impl Strategy<Value = Constant> {
    prop_oneof![
        prop_oneof![Just("a"), Just("b"), Just("c")].prop_map(|s| Constant::Symbol(s.to_string())),
        prop_oneof![Just("x"), Just("y")].prop_map(|s| Constant::String(s.to_string())),
        (-3i64..=3).prop_map(Constant::Int),
        prop_oneof![Just(0.0f64), Just(1.5), Just(-2.0)].prop_map(Constant::Float),
        any::<bool>().prop_map(Constant::Bool),
    ]
}

/// A ground IR value, via the same constant pools.
pub(crate) fn arb_value() -> impl Strategy<Value = ir::Value> {
    arb_constant().prop_map(|c| match c {
        Constant::Symbol(s) => ir::Value::Symbol(s),
        Constant::String(s) => ir::Value::String(s),
        Constant::Int(i) => ir::Value::Int(i),
        Constant::Float(f) => ir::Value::Float(ir::F64::new(f).expect("pool floats are not NaN")),
        Constant::Bool(b) => ir::Value::Bool(b),
    })
}

/// Raw spec for one body-atom argument: a variable slot from a small pool, or
/// a constant.
#[derive(Debug, Clone)]
struct ArgSpec {
    var: Option<u8>,
    constant: Constant,
}

fn arb_arg_spec() -> impl Strategy<Value = ArgSpec> {
    (proptest::option::weighted(0.6, 0u8..6), arb_constant())
        .prop_map(|(var, constant)| ArgSpec { var, constant })
}

/// Raw spec for one head argument: an index into the body's variable list
/// (applied modulo its length), or a constant fallback.
#[derive(Debug, Clone)]
struct HeadArgSpec {
    body_var_index: Option<u8>,
    constant: Constant,
}

fn arb_head_arg_spec() -> impl Strategy<Value = HeadArgSpec> {
    (proptest::option::weighted(0.7, 0u8..8), arb_constant()).prop_map(
        |(body_var_index, constant)| HeadArgSpec {
            body_var_index,
            constant,
        },
    )
}

type BodyAtomSpec = (u8, Vec<ArgSpec>);
type RuleSpec = (Vec<BodyAtomSpec>, u8, Vec<HeadArgSpec>);
type FactSpec = (u8, Vec<Constant>);
type ProgramSpec = (Vec<u32>, Vec<FactSpec>, Vec<RuleSpec>);

/// Size bounds for [`arb_program_spec`]. Spec argument vectors are always
/// generated at [`MAX_ARITY`] length and truncated to the predicate's arity
/// during building, so `max_arity` only needs to stay ≤ `MAX_ARITY`.
struct SpecBounds {
    predicates: std::ops::RangeInclusive<usize>,
    max_arity: u32,
    facts: std::ops::RangeInclusive<usize>,
    rules: std::ops::RangeInclusive<usize>,
    body_len: std::ops::RangeInclusive<usize>,
}

fn arb_program_spec(bounds: SpecBounds) -> impl Strategy<Value = ProgramSpec> {
    let arities = proptest::collection::vec(1u32..=bounds.max_arity, bounds.predicates);
    let facts = proptest::collection::vec(
        (
            any::<u8>(),
            proptest::collection::vec(arb_constant(), MAX_ARITY),
        ),
        bounds.facts,
    );
    let rules = proptest::collection::vec(
        (
            proptest::collection::vec(
                (
                    any::<u8>(),
                    proptest::collection::vec(arb_arg_spec(), MAX_ARITY),
                ),
                bounds.body_len,
            ),
            any::<u8>(),
            proptest::collection::vec(arb_head_arg_spec(), MAX_ARITY),
        ),
        bounds.rules,
    );
    (arities, facts, rules)
}

/// Bounds used for lowering-focused properties (Phase A).
fn lowering_bounds() -> SpecBounds {
    SpecBounds {
        predicates: 2..=4,
        max_arity: MAX_ARITY as u32,
        facts: 0..=8,
        rules: 0..=5,
        body_len: 1..=3,
    }
}

/// Tighter bounds for evaluation-focused properties (Phases B/E): evaluation
/// cost grows much faster than lowering cost (joins, fixpoints), so keep
/// arities and bodies small while staying collision-rich.
fn eval_bounds() -> SpecBounds {
    SpecBounds {
        predicates: 2..=3,
        max_arity: 2,
        facts: 0..=6,
        rules: 0..=4,
        body_len: 1..=2,
    }
}

/// A safe-by-construction positional program: consistent arities, ground
/// facts, and every head variable drawn from the rule's own body variables.
///
/// Bounded small: 2–4 predicates of arity 1–3, up to 8 facts and 5 rules.
pub(crate) fn arb_safe_program() -> impl Strategy<Value = Program> {
    arb_program_spec(lowering_bounds()).prop_map(build_program)
}

/// A lowered, evaluation-ready program with its EDB (testing.md Phases B/E):
/// [`arb_program_spec`] under [`eval_bounds`], built and lowered. Lowering is
/// total on safe-by-construction programs (property A11), so the `expect`
/// never fires.
pub(crate) fn arb_program_with_edb() -> impl Strategy<Value = ir::Program> {
    arb_program_spec(eval_bounds())
        .prop_map(build_program)
        .prop_map(|ast| crate::lower::lower(&ast).expect("safe-by-construction programs lower"))
}

/// A pair `(base, extended)` of safe programs over the same predicate
/// universe where `extended` is `base` plus one extra fact and one extra rule
/// (testing.md B4 monotonicity). Compare outputs by predicate *name*: the
/// extra statements can change predicate first-appearance order, so `PredId`s
/// need not line up between the two.
pub(crate) fn arb_extension_pair() -> impl Strategy<Value = (Program, Program)> {
    let extras = (
        (
            any::<u8>(),
            proptest::collection::vec(arb_constant(), MAX_ARITY),
        ),
        (
            proptest::collection::vec(
                (
                    any::<u8>(),
                    proptest::collection::vec(arb_arg_spec(), MAX_ARITY),
                ),
                1..=2,
            ),
            any::<u8>(),
            proptest::collection::vec(arb_head_arg_spec(), MAX_ARITY),
        ),
    );
    (arb_program_spec(eval_bounds()), extras).prop_map(
        |((arities, facts, rules), (extra_fact, extra_rule))| {
            let base = build_program((arities.clone(), facts.clone(), rules.clone()));
            let mut facts = facts;
            let mut rules = rules;
            facts.push(extra_fact);
            rules.push(extra_rule);
            let extended = build_program((arities, facts, rules));
            (base, extended)
        },
    )
}

/// Random edge sets over a small node pool — the `arb_edb` shape for the
/// §16.1 ancestor program (testing.md B7).
pub(crate) fn arb_parent_edges() -> impl Strategy<Value = Vec<(String, String)>> {
    proptest::collection::vec((0u8..6, 0u8..6), 0..=15).prop_map(|pairs| {
        pairs
            .into_iter()
            .map(|(a, b)| (format!("n{a}"), format!("n{b}")))
            .collect()
    })
}

// --- IR-level metamorphic mutators (testing.md B3–B6) ---
//
// Plain functions over `ir::Program`, selector-driven so strategies stay on
// index vectors (shrinking-friendly). Each preserves the IR contract (arity
// consistency, safety, strata coverage), so the mutant is always evaluable.

/// B3: appends duplicates of existing facts (selectors index modulo the fact
/// count). No-op on fact-free programs.
pub(crate) fn with_duplicated_facts(mut program: ir::Program, selectors: &[u8]) -> ir::Program {
    if !program.facts.is_empty() {
        for &sel in selectors {
            let fact = program.facts[sel as usize % program.facts.len()].clone();
            program.facts.push(fact);
        }
    }
    program
}

/// B4: appends one extra ground fact for an existing predicate, at its
/// declared arity. No-op on predicate-free programs. `values` must supply at
/// least [`MAX_ARITY`] values.
pub(crate) fn with_extra_fact(
    mut program: ir::Program,
    pred_sel: u8,
    values: Vec<ir::Value>,
) -> ir::Program {
    if !program.predicates.is_empty() {
        let pred = ir::PredId(pred_sel as u32 % program.predicates.len() as u32);
        let arity = program.pred_info(pred).arity as usize;
        program.facts.push(ir::Fact {
            pred,
            tuple: ir::Tuple(values.into_iter().take(arity).collect()),
        });
    }
    program
}

/// B5: swaps two body literals of one rule (selectors modulo the respective
/// lengths). Variable slots are rule-scoped, so the swap needs no renaming;
/// safety is occurrence-based, so the rule stays safe.
pub(crate) fn with_swapped_body(
    mut program: ir::Program,
    rule_sel: u8,
    i: u8,
    j: u8,
) -> ir::Program {
    if !program.rules.is_empty() {
        let rule_idx = rule_sel as usize % program.rules.len();
        let rule = &mut program.rules[rule_idx];
        let len = rule.body.len();
        if len > 1 {
            rule.body.swap(i as usize % len, j as usize % len);
        }
    }
    program
}

/// B6: swaps two rule ids within one stratum, changing rule application order
/// but not membership.
pub(crate) fn with_swapped_stratum_rules(
    mut program: ir::Program,
    stratum_sel: u8,
    i: u8,
    j: u8,
) -> ir::Program {
    if !program.strata.is_empty() {
        let stratum_idx = stratum_sel as usize % program.strata.len();
        let stratum = &mut program.strata[stratum_idx];
        let len = stratum.len();
        if len > 1 {
            stratum.swap(i as usize % len, j as usize % len);
        }
    }
    program
}

fn build_program((arities, facts, rules): (Vec<u32>, Vec<FactSpec>, Vec<RuleSpec>)) -> Program {
    let pred_name = |sel: u8| format!("p{}", sel as usize % arities.len());
    let pred_arity = |sel: u8| arities[sel as usize % arities.len()] as usize;

    let mut statements = Vec::new();

    for (sel, constants) in facts {
        let args = constants
            .into_iter()
            .take(pred_arity(sel))
            .map(|c| Term {
                kind: TermKind::Constant(c),
                span: Span::DUMMY,
            })
            .collect();
        statements.push(fact(&pred_name(sel), args));
    }

    for (body_specs, head_sel, head_specs) in rules {
        let mut body = Vec::new();
        // Body variables in first-occurrence order, deduplicated.
        let mut body_vars: Vec<u8> = Vec::new();
        for (sel, arg_specs) in body_specs {
            let args = arg_specs
                .into_iter()
                .take(pred_arity(sel))
                .map(|spec| match spec.var {
                    Some(v) => {
                        if !body_vars.contains(&v) {
                            body_vars.push(v);
                        }
                        var_term(&format!("V{v}"))
                    }
                    None => Term {
                        kind: TermKind::Constant(spec.constant),
                        span: Span::DUMMY,
                    },
                })
                .collect();
            body.push(positive_literal(positional_atom(&pred_name(sel), args)));
        }
        let head_args = head_specs
            .into_iter()
            .take(pred_arity(head_sel))
            .map(|spec| match spec.body_var_index {
                Some(i) if !body_vars.is_empty() => {
                    let v = body_vars[i as usize % body_vars.len()];
                    var_term(&format!("V{v}"))
                }
                _ => Term {
                    kind: TermKind::Constant(spec.constant),
                    span: Span::DUMMY,
                },
            })
            .collect();
        statements.push(rule(positional_atom(&pred_name(head_sel), head_args), body));
    }

    Program { statements }
}

/// The single-defect mutations [`inject_defect`] can apply, with the error
/// substring lowering must report for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefectKind {
    /// A rule whose head variable occurs in no positive body atom.
    UnsafeHeadVar,
    /// A fact containing a variable.
    NonGroundFact,
    /// The same predicate used at two arities.
    ArityClash,
}

impl DefectKind {
    /// Substring the reported `Error::Semantic` must contain.
    pub(crate) fn expected_error(self) -> &'static str {
        match self {
            DefectKind::UnsafeHeadVar => "head variable",
            DefectKind::NonGroundFact => "not ground",
            DefectKind::ArityClash => "arity",
        }
    }
}

pub(crate) fn arb_defect() -> impl Strategy<Value = DefectKind> {
    prop_oneof![
        Just(DefectKind::UnsafeHeadVar),
        Just(DefectKind::NonGroundFact),
        Just(DefectKind::ArityClash),
    ]
}

/// Appends exactly one defective statement (over fresh `defect_*` predicates,
/// so it cannot interact with the surrounding program) and returns the
/// now-invalid program.
pub(crate) fn inject_defect(mut program: Program, kind: DefectKind) -> Program {
    let symbol = |s: &str| Term {
        kind: TermKind::Constant(Constant::Symbol(s.to_string())),
        span: Span::DUMMY,
    };
    let statements: Vec<Statement> = match kind {
        DefectKind::UnsafeHeadVar => vec![rule(
            positional_atom("defect_head", vec![var_term("Zz")]),
            vec![positive_literal(positional_atom(
                "defect_body",
                vec![symbol("a")],
            ))],
        )],
        DefectKind::NonGroundFact => vec![fact("defect_fact", vec![var_term("Zz")])],
        DefectKind::ArityClash => vec![
            fact("defect_clash", vec![symbol("a")]),
            fact("defect_clash", vec![symbol("a"), symbol("b")]),
        ],
    };
    program.statements.extend(statements);
    program
}
