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
//!
//! [`positionalize`] rewrites a generated program's named literals into the
//! positional literals they denote — the other half of property A13, which
//! pins down that named arguments are surface syntax with no IR footprint.

use proptest::prelude::*;

use crate::ast::fixtures::{
    declare, fact, field_decl, named_arg, named_atom, negated_literal, positional_atom,
    positive_literal, query, rule, var_term, wildcard_term,
};
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

/// The field name for argument position `i` of a schema-carrying predicate.
fn field_name(position: usize) -> String {
    format!("f{position}")
}

/// Raw spec for one body-atom argument: a variable slot from a small pool, or
/// a constant. `selected` drives partial selection when the atom is written in
/// named form (§4); it is ignored for positional atoms.
#[derive(Debug, Clone)]
struct ArgSpec {
    var: Option<u8>,
    constant: Constant,
    selected: bool,
}

fn arb_arg_spec() -> impl Strategy<Value = ArgSpec> {
    (
        proptest::option::weighted(0.6, 0u8..6),
        arb_constant(),
        proptest::bool::weighted(0.7),
    )
        .prop_map(|(var, constant, selected)| ArgSpec {
            var,
            constant,
            selected,
        })
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

/// One body atom: predicate selector, named-form flag, negated flag, args.
type BodyAtomSpec = (u8, bool, bool, Vec<ArgSpec>);
/// A rule: body atoms, head predicate selector, named-head flag, head args.
type RuleSpec = (Vec<BodyAtomSpec>, u8, bool, Vec<HeadArgSpec>);
/// A query: just a body (`?- …`), built exactly like a rule body.
type QuerySpec = Vec<BodyAtomSpec>;
type FactSpec = (u8, Vec<Constant>);
/// One predicate: arity, whether it gets a `declare` naming its fields (only
/// such predicates can be used with named arguments, §4), and its *level*
/// (0..=2) — the stratifiability witness: `build_program` remaps positive
/// body selectors into predicates at ≤ the head's level and negated ones
/// strictly below it, so any cycle is level-constant and positive — generated
/// programs are stratifiable by construction (no rejection sampling). Levels
/// are coarse (three of them) so positive recursion within a level stays
/// common.
type PredSpec = (u32, bool, u8);
type ProgramSpec = (Vec<PredSpec>, Vec<FactSpec>, Vec<RuleSpec>, Vec<QuerySpec>);

/// Size bounds for [`arb_program_spec`]. Spec argument vectors are always
/// generated at [`MAX_ARITY`] length and truncated to the predicate's arity
/// during building, so `max_arity` only needs to stay ≤ `MAX_ARITY`.
struct SpecBounds {
    predicates: std::ops::RangeInclusive<usize>,
    max_arity: u32,
    facts: std::ops::RangeInclusive<usize>,
    rules: std::ops::RangeInclusive<usize>,
    body_len: std::ops::RangeInclusive<usize>,
    queries: std::ops::RangeInclusive<usize>,
}

fn arb_program_spec(bounds: SpecBounds) -> impl Strategy<Value = ProgramSpec> {
    let arities = proptest::collection::vec(
        (
            1u32..=bounds.max_arity,
            proptest::bool::weighted(0.5),
            0u8..=2,
        ),
        bounds.predicates,
    );
    let facts = proptest::collection::vec(
        (
            any::<u8>(),
            proptest::collection::vec(arb_constant(), MAX_ARITY),
        ),
        bounds.facts,
    );
    let rules = proptest::collection::vec(
        (
            proptest::collection::vec(arb_body_atom_spec(), bounds.body_len.clone()),
            any::<u8>(),
            proptest::bool::weighted(0.4),
            proptest::collection::vec(arb_head_arg_spec(), MAX_ARITY),
        ),
        bounds.rules,
    );
    let queries = proptest::collection::vec(
        proptest::collection::vec(arb_body_atom_spec(), bounds.body_len),
        bounds.queries,
    );
    (arities, facts, rules, queries)
}

fn arb_body_atom_spec() -> impl Strategy<Value = BodyAtomSpec> {
    (
        any::<u8>(),
        proptest::bool::weighted(0.4),
        proptest::bool::weighted(0.3),
        proptest::collection::vec(arb_arg_spec(), MAX_ARITY),
    )
}

/// Bounds used for lowering-focused properties (Phase A).
fn lowering_bounds() -> SpecBounds {
    SpecBounds {
        predicates: 2..=4,
        max_arity: MAX_ARITY as u32,
        facts: 0..=8,
        rules: 0..=5,
        body_len: 1..=3,
        queries: 0..=2,
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
        queries: 0..=2,
    }
}

/// A safe-by-construction program: consistent arities, ground facts, and every
/// head variable drawn from the rule's own body variables.
///
/// Mixes positional and named-argument forms (§4). Roughly half the predicates
/// get a `declare` naming their fields, and atoms over those may be written in
/// named form — partially selected in bodies, fully supplied in heads. Because
/// a partially selected atom does not bind the variables of the fields it
/// omits, `build_program` collects body variables *after* selection, which is
/// what keeps head variables safe by construction.
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

/// A pair `(base, extended)` of safe programs where `extended` is `base` plus
/// one extra fact and one extra rule (testing.md B4 monotonicity). The
/// extension lives on fresh `ext_*` predicates — nothing in the base depends
/// on them, so `extended ⊇ base` holds even when the base negates (testing.md
/// C3's B4 restriction) — while the extra rule still joins against an
/// existing relation. Compare outputs by predicate *name*: the extra
/// statements change predicate first-appearance order, so `PredId`s need not
/// line up between the two.
pub(crate) fn arb_extension_pair() -> impl Strategy<Value = (Program, Program)> {
    let extras = (any::<u8>(), arb_constant());
    (arb_program_spec(eval_bounds()), extras).prop_map(
        |((arities, facts, rules, queries), (join_sel, constant))| {
            let join_index = join_sel as usize % arities.len();
            let join_arity = arities[join_index].0 as usize;
            let base = build_program((arities, facts, rules, queries));
            let mut extended = base.clone();
            extended.statements.push(fact(
                "ext_seed",
                vec![Term {
                    kind: TermKind::Constant(constant),
                    span: Span::DUMMY,
                }],
            ));
            // ext_result(V0) :- ext_seed(V0), p<join>(_, …).
            extended.statements.push(rule(
                positional_atom("ext_result", vec![var_term("V0")]),
                vec![
                    positive_literal(positional_atom("ext_seed", vec![var_term("V0")])),
                    positive_literal(positional_atom(
                        &format!("p{join_index}"),
                        (0..join_arity).map(|_| wildcard_term()).collect(),
                    )),
                ],
            ));
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

/// Predicates no negated atom transitively depends on. Adding facts to these
/// cannot grow any negated relation, so evaluation stays monotone in them
/// even under negation (testing.md C3's B4 restriction). For positive
/// programs this is every predicate.
pub(crate) fn negation_independent_preds(program: &ir::Program) -> Vec<ir::PredId> {
    let n = program.predicates.len();
    let mut deps: Vec<Vec<ir::PredId>> = vec![Vec::new(); n];
    let mut tainted_roots: Vec<ir::PredId> = Vec::new();
    for rule in &program.rules {
        for literal in &rule.body {
            match &literal.kind {
                ir::BodyLiteralKind::Atom(atom) => {
                    deps[rule.head.pred.0 as usize].push(atom.pred);
                }
                ir::BodyLiteralKind::NegAtom(atom) => {
                    deps[rule.head.pred.0 as usize].push(atom.pred);
                    tainted_roots.push(atom.pred);
                }
                ir::BodyLiteralKind::Compare { .. } => {}
            }
        }
    }
    let mut tainted = vec![false; n];
    let mut worklist = tainted_roots;
    while let Some(pred) = worklist.pop() {
        if std::mem::replace(&mut tainted[pred.0 as usize], true) {
            continue;
        }
        worklist.extend(deps[pred.0 as usize].iter().copied());
    }
    (0..n as u32)
        .map(ir::PredId)
        .filter(|pred| !tainted[pred.0 as usize])
        .collect()
}

/// B4: appends one extra ground fact, at its predicate's declared arity, for
/// a predicate no negation transitively depends on (adding facts elsewhere is
/// not monotone under negation — testing.md C3). No-op when no such predicate
/// exists. `values` must supply at least [`MAX_ARITY`] values.
pub(crate) fn with_extra_fact(
    mut program: ir::Program,
    pred_sel: u8,
    values: Vec<ir::Value>,
) -> ir::Program {
    let safe = negation_independent_preds(&program);
    if !safe.is_empty() {
        let pred = safe[pred_sel as usize % safe.len()];
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

/// Builds one body from its atom specs: positive atoms first in source order
/// (collecting the positively-bound variable pool), then negated atoms whose
/// named variables draw only from that pool — §10 safety by construction;
/// everything else under a negation is a wildcard (existential, §7) or a
/// constant. `positive_pool` / `negative_pool` are the predicate indices
/// selectors remap into; a negated spec falls back to positive when
/// `negative_pool` is empty. Returns the literals and the positively-bound
/// variable pool (what heads may draw from).
fn build_body(
    body_specs: Vec<BodyAtomSpec>,
    positive_pool: &[usize],
    negative_pool: &[usize],
    arities: &[PredSpec],
) -> (Vec<crate::ast::Literal>, Vec<u8>) {
    let pred_name = |index: usize| format!("p{index}");
    let pred_arity = |index: usize| arities[index].0 as usize;
    let has_schema = |index: usize| arities[index].1;

    let mut body = Vec::new();
    let mut body_vars: Vec<u8> = Vec::new();
    let mut negated_specs: Vec<(usize, bool, Vec<ArgSpec>)> = Vec::new();
    for (sel, named, negated, arg_specs) in body_specs {
        if negated && !negative_pool.is_empty() {
            let index = negative_pool[sel as usize % negative_pool.len()];
            negated_specs.push((index, named, arg_specs));
            continue;
        }
        let index = positive_pool[sel as usize % positive_pool.len()];
        let arity = pred_arity(index);
        let named = named && has_schema(index);
        let specs: Vec<ArgSpec> = arg_specs.into_iter().take(arity).collect();

        // Partial selection keeps at least one field: `p()` is not
        // grammatical (§5, `named` needs one or more pairs).
        let keep = |position: usize, spec: &ArgSpec| {
            !named || spec.selected || specs.iter().all(|s| !s.selected) && position == 0
        };

        let mut terms = Vec::new();
        for (position, spec) in specs.iter().enumerate() {
            if !keep(position, spec) {
                continue;
            }
            let term = match spec.var {
                Some(v) => {
                    if !body_vars.contains(&v) {
                        body_vars.push(v);
                    }
                    var_term(&format!("V{v}"))
                }
                None => Term {
                    kind: TermKind::Constant(spec.constant.clone()),
                    span: Span::DUMMY,
                },
            };
            terms.push((position, term));
        }

        let atom = if named {
            named_atom(
                &pred_name(index),
                terms
                    .into_iter()
                    .map(|(position, term)| named_arg(&field_name(position), term))
                    .collect(),
            )
        } else {
            positional_atom(
                &pred_name(index),
                terms.into_iter().map(|(_, term)| term).collect(),
            )
        };
        body.push(positive_literal(atom));
    }

    // Negated atoms after the positives in source order (B5's IR-level body
    // swaps supply the other orderings). Named form still partially selects,
    // so omitted fields exercise the wildcard-under-negation lowering path
    // too.
    for (index, named, arg_specs) in negated_specs {
        let arity = pred_arity(index);
        let named = named && has_schema(index);
        let specs: Vec<ArgSpec> = arg_specs.into_iter().take(arity).collect();
        let keep = |position: usize, spec: &ArgSpec| {
            !named || spec.selected || specs.iter().all(|s| !s.selected) && position == 0
        };

        let mut terms = Vec::new();
        for (position, spec) in specs.iter().enumerate() {
            if !keep(position, spec) {
                continue;
            }
            let term = match spec.var {
                Some(v) if (named || spec.selected) && !body_vars.is_empty() => {
                    let bound = body_vars[v as usize % body_vars.len()];
                    var_term(&format!("V{bound}"))
                }
                Some(_) => wildcard_term(),
                None => Term {
                    kind: TermKind::Constant(spec.constant.clone()),
                    span: Span::DUMMY,
                },
            };
            terms.push((position, term));
        }

        let atom = if named {
            named_atom(
                &pred_name(index),
                terms
                    .into_iter()
                    .map(|(position, term)| named_arg(&field_name(position), term))
                    .collect(),
            )
        } else {
            positional_atom(
                &pred_name(index),
                terms.into_iter().map(|(_, term)| term).collect(),
            )
        };
        body.push(negated_literal(atom));
    }

    (body, body_vars)
}

fn build_program((arities, facts, rules, queries): ProgramSpec) -> Program {
    let pred_name = |index: usize| format!("p{index}");
    let pred_arity = |index: usize| arities[index].0 as usize;
    // Only `declare`d predicates may be used with named arguments (§4).
    let has_schema = |index: usize| arities[index].1;
    let level = |index: usize| arities[index].2;

    let mut statements = Vec::new();

    // Declarations first, though lowering collects schemas program-wide and
    // does not require it.
    for (index, (arity, declared, _level)) in arities.iter().enumerate() {
        if *declared {
            let fields = (0..*arity as usize)
                .map(|position| field_decl(&field_name(position), None))
                .collect();
            statements.push(declare(&format!("p{index}"), fields));
        }
    }

    // Facts create no dependency edges, so their selectors map over every
    // predicate regardless of level.
    for (sel, constants) in facts {
        let index = sel as usize % arities.len();
        let args = constants
            .into_iter()
            .take(pred_arity(index))
            .map(|c| Term {
                kind: TermKind::Constant(c),
                span: Span::DUMMY,
            })
            .collect();
        statements.push(fact(&pred_name(index), args));
    }

    for (body_specs, head_sel, named_head, head_specs) in rules {
        let head_index = head_sel as usize % arities.len();
        // The stratifiability witness (see `PredSpec`): positive body
        // selectors remap into predicates at ≤ the head's level (the head
        // itself is in the pool, so positive recursion survives), negated
        // ones strictly below; a negation with nothing below falls back to a
        // positive atom.
        let positive_pool: Vec<usize> = (0..arities.len())
            .filter(|&i| level(i) <= level(head_index))
            .collect();
        let negative_pool: Vec<usize> = (0..arities.len())
            .filter(|&i| level(i) < level(head_index))
            .collect();

        let (body, body_vars) = build_body(body_specs, &positive_pool, &negative_pool, &arities);

        // A named head must supply every field (§4), so head arguments are
        // built at full arity either way.
        let head_terms: Vec<Term> = head_specs
            .into_iter()
            .take(pred_arity(head_index))
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
        let head = if named_head && has_schema(head_index) {
            named_atom(
                &pred_name(head_index),
                head_terms
                    .into_iter()
                    .enumerate()
                    .map(|(position, term)| named_arg(&field_name(position), term))
                    .collect(),
            )
        } else {
            positional_atom(&pred_name(head_index), head_terms)
        };
        statements.push(rule(head, body));
    }

    // Queries run over the finished model, where every relation is complete
    // (§7) — so both pools cover every predicate; no level discipline needed.
    // A query binding no named variable is skipped (the negation-fallback
    // pattern): B8's query≡rule equivalence projects named variables, and the
    // nullary corner isn't worth generating.
    let all_preds: Vec<usize> = (0..arities.len()).collect();
    for body_specs in queries {
        let (body, body_vars) = build_body(body_specs, &all_preds, &all_preds, &arities);
        if body_vars.is_empty() {
            continue;
        }
        statements.push(query(body));
    }

    Program { statements }
}

/// Rewrites every named literal into the positional literal it denotes:
/// each field lands at its schema position and omitted fields become `_`.
///
/// Lowering must be insensitive to the difference (testing.md A13). This walks
/// the AST and recovers positions from the field-name convention rather than
/// reusing the builder's bookkeeping, so it is an independent check.
pub(crate) fn positionalize(program: &Program) -> Program {
    use crate::ast::{Args, Atom, Clause, LiteralKind, StatementKind};

    // Predicate -> declared field names, from the program's own declarations.
    let mut schemas: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for statement in &program.statements {
        if let StatementKind::Declare(declaration) = &statement.kind {
            schemas.insert(
                declaration.relation.name.clone(),
                declaration
                    .fields
                    .iter()
                    .map(|f| f.name.name.clone())
                    .collect(),
            );
        }
    }

    let rewrite_atom = |atom: &Atom| -> Atom {
        let Args::Named(named) = &atom.args else {
            return atom.clone();
        };
        let fields = schemas
            .get(&atom.predicate.name)
            .expect("generated named atoms always have a declared schema");
        let args = fields
            .iter()
            .map(|field| {
                named
                    .iter()
                    .find(|arg| &arg.field.name == field)
                    .map(|arg| arg.value.clone())
                    .unwrap_or(Term {
                        kind: TermKind::Wildcard,
                        span: Span::DUMMY,
                    })
            })
            .collect();
        positional_atom(&atom.predicate.name, args)
    };

    let rewrite_body = |body: &[crate::ast::Literal]| -> Vec<crate::ast::Literal> {
        body.iter()
            .map(|literal| match &literal.kind {
                LiteralKind::Atom { negated, atom } => crate::ast::Literal {
                    kind: LiteralKind::Atom {
                        negated: *negated,
                        atom: rewrite_atom(atom),
                    },
                    span: literal.span,
                },
                LiteralKind::Comparison(_) => literal.clone(),
            })
            .collect()
    };

    let statements = program
        .statements
        .iter()
        .map(|statement| match &statement.kind {
            StatementKind::Clause(clause) => Statement {
                kind: StatementKind::Clause(Clause {
                    head: rewrite_atom(&clause.head),
                    body: rewrite_body(&clause.body),
                    span: clause.span,
                }),
                span: statement.span,
            },
            StatementKind::Query(query) => Statement {
                kind: StatementKind::Query(crate::ast::Query {
                    body: rewrite_body(&query.body),
                    span: query.span,
                }),
                span: statement.span,
            },
            _ => statement.clone(),
        })
        .collect();

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
    /// A named argument whose field is not in the predicate's schema.
    UnknownField,
    /// A named head that leaves a field unbound (§4 forbids it).
    PartialSelectionInHead,
    /// Named arguments on a predicate with no `declare` and no import schema.
    NamedWithoutSchema,
    /// Recursion through negation: a predicate negating itself.
    NegativeCycle,
    /// A named variable occurring only in a negated atom (§10).
    UnsafeNegatedVar,
}

impl DefectKind {
    /// Substring the reported `Error::Semantic` must contain.
    pub(crate) fn expected_error(self) -> &'static str {
        match self {
            DefectKind::UnsafeHeadVar => "head variable",
            DefectKind::NonGroundFact => "not ground",
            DefectKind::ArityClash => "arity",
            DefectKind::UnknownField => "unknown field",
            DefectKind::PartialSelectionInHead => "must supply every field",
            DefectKind::NamedWithoutSchema => "require known field names",
            DefectKind::NegativeCycle => "not stratifiable",
            DefectKind::UnsafeNegatedVar => "negated atom",
        }
    }
}

pub(crate) fn arb_defect() -> impl Strategy<Value = DefectKind> {
    prop_oneof![
        Just(DefectKind::UnsafeHeadVar),
        Just(DefectKind::NonGroundFact),
        Just(DefectKind::ArityClash),
        Just(DefectKind::UnknownField),
        Just(DefectKind::PartialSelectionInHead),
        Just(DefectKind::NamedWithoutSchema),
        Just(DefectKind::NegativeCycle),
        Just(DefectKind::UnsafeNegatedVar),
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
        DefectKind::UnknownField => vec![
            declare("defect_named", vec![field_decl("f0", None)]),
            rule(
                positional_atom("defect_unknown", vec![var_term("Zz")]),
                vec![positive_literal(named_atom(
                    "defect_named",
                    vec![named_arg("nope", var_term("Zz"))],
                ))],
            ),
        ],
        DefectKind::PartialSelectionInHead => vec![
            declare(
                "defect_partial",
                vec![field_decl("f0", None), field_decl("f1", None)],
            ),
            rule(
                named_atom("defect_partial", vec![named_arg("f0", var_term("Zz"))]),
                vec![positive_literal(positional_atom(
                    "defect_source",
                    vec![var_term("Zz")],
                ))],
            ),
        ],
        DefectKind::NamedWithoutSchema => vec![rule(
            positional_atom("defect_undeclared", vec![var_term("Zz")]),
            vec![positive_literal(named_atom(
                "defect_no_schema",
                vec![named_arg("f0", var_term("Zz"))],
            ))],
        )],
        DefectKind::NegativeCycle => vec![rule(
            positional_atom("defect_cycle", vec![var_term("Zz")]),
            vec![
                positive_literal(positional_atom("defect_seed", vec![var_term("Zz")])),
                negated_literal(positional_atom("defect_cycle", vec![var_term("Zz")])),
            ],
        )],
        DefectKind::UnsafeNegatedVar => vec![rule(
            positional_atom("defect_unsafe", vec![var_term("Zz")]),
            vec![
                positive_literal(positional_atom("defect_pos", vec![var_term("Zz")])),
                negated_literal(positional_atom("defect_other", vec![var_term("Yy")])),
            ],
        )],
    };
    program.statements.extend(statements);
    program
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Args, LiteralKind, StatementKind};
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    /// Counts (named, positional) body atoms in a program.
    fn count_forms(program: &Program) -> (usize, usize) {
        let mut named = 0;
        let mut positional = 0;
        for statement in &program.statements {
            if let StatementKind::Clause(clause) = &statement.kind {
                for atom in
                    std::iter::once(&clause.head).chain(clause.body.iter().filter_map(|literal| {
                        match &literal.kind {
                            LiteralKind::Atom { atom, .. } => Some(atom),
                            LiteralKind::Comparison(_) => None,
                        }
                    }))
                {
                    match atom.args {
                        Args::Named(_) => named += 1,
                        Args::Positional(_) => positional += 1,
                    }
                }
            }
        }
        (named, positional)
    }

    /// Coverage guard: the generator must actually emit both argument forms.
    /// Without this, property A13 (named/positional agreement) could pass
    /// vacuously if named generation silently regressed.
    #[test]
    fn generator_emits_both_argument_forms() {
        let mut runner = TestRunner::deterministic();
        let strategy = arb_safe_program();
        let (mut named, mut positional) = (0, 0);
        for _ in 0..200 {
            let program = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            let (n, p) = count_forms(&program);
            named += n;
            positional += p;
        }
        assert!(named > 0, "generator never produced a named atom");
        assert!(positional > 0, "generator never produced a positional atom");
    }

    /// Partial selection must actually occur, or the omitted-field path (fresh
    /// slots) goes untested by the properties.
    #[test]
    fn generator_emits_partial_selection() {
        let mut runner = TestRunner::deterministic();
        let strategy = arb_safe_program();
        let mut partial = 0;
        for _ in 0..200 {
            let program = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            // A named body atom supplying fewer pairs than its predicate's
            // declared arity is a partial selection.
            let mut arities = std::collections::HashMap::new();
            for statement in &program.statements {
                if let StatementKind::Declare(d) = &statement.kind {
                    arities.insert(d.relation.name.clone(), d.fields.len());
                }
            }
            for statement in &program.statements {
                if let StatementKind::Clause(clause) = &statement.kind {
                    for literal in &clause.body {
                        if let LiteralKind::Atom { atom, .. } = &literal.kind
                            && let Args::Named(named) = &atom.args
                            && let Some(&arity) = arities.get(&atom.predicate.name)
                            && named.len() < arity
                        {
                            partial += 1;
                        }
                    }
                }
            }
        }
        assert!(partial > 0, "generator never produced a partial selection");
    }

    /// Coverage guard for the negation paths (the A13 precedent): sampling
    /// must produce negated atoms, wildcards under negation, and programs
    /// that lower to more than one stratum — otherwise C1/C3 could pass
    /// vacuously if negation generation silently regressed.
    #[test]
    fn generator_emits_negation_shapes() {
        let mut runner = TestRunner::deterministic();
        let strategy = arb_safe_program();
        let (mut negated, mut wildcard_under_negation, mut multi_strata) = (0, 0, 0);
        for _ in 0..200 {
            let program = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            for statement in &program.statements {
                if let StatementKind::Clause(clause) = &statement.kind {
                    for literal in &clause.body {
                        if let LiteralKind::Atom {
                            negated: true,
                            atom,
                        } = &literal.kind
                        {
                            negated += 1;
                            // Count only explicit positional wildcards — the
                            // conservative signal (named partial selection
                            // also produces fresh slots, but is guarded by
                            // `generator_emits_partial_selection`).
                            if let Args::Positional(terms) = &atom.args
                                && terms
                                    .iter()
                                    .any(|t| matches!(t.kind, crate::ast::TermKind::Wildcard))
                            {
                                wildcard_under_negation += 1;
                            }
                        }
                    }
                }
            }
            let lowered =
                crate::lower::lower(&program).expect("safe-by-construction programs lower");
            if lowered.strata.len() > 1 {
                multi_strata += 1;
            }
        }
        assert!(negated > 0, "generator never produced a negated atom");
        assert!(
            wildcard_under_negation > 0,
            "generator never produced a wildcard under negation"
        );
        assert!(
            multi_strata > 0,
            "generator never produced a multi-stratum program"
        );
    }

    /// Coverage guard for the query paths: sampling must produce queries and
    /// queries containing a negated literal — otherwise B8 (query≡rule) could
    /// pass vacuously, and query negation would fall back to hand tests only.
    #[test]
    fn generator_emits_queries() {
        let mut runner = TestRunner::deterministic();
        let strategy = arb_safe_program();
        let (mut queries, mut negated_queries) = (0, 0);
        for _ in 0..200 {
            let program = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            for statement in &program.statements {
                if let StatementKind::Query(q) = &statement.kind {
                    queries += 1;
                    if q.body
                        .iter()
                        .any(|l| matches!(l.kind, LiteralKind::Atom { negated: true, .. }))
                    {
                        negated_queries += 1;
                    }
                }
            }
        }
        assert!(queries > 0, "generator never produced a query");
        assert!(
            negated_queries > 0,
            "generator never produced a negated query"
        );
    }
}
