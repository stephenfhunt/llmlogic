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
    declare, fact, field_decl, named_arg, named_atom, positional_atom, positive_literal, rule,
    var_term,
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

/// One body atom: predicate selector, whether to write it in named form, args.
type BodyAtomSpec = (u8, bool, Vec<ArgSpec>);
/// A rule: body atoms, head predicate selector, named-head flag, head args.
type RuleSpec = (Vec<BodyAtomSpec>, u8, bool, Vec<HeadArgSpec>);
type FactSpec = (u8, Vec<Constant>);
/// One predicate: arity, and whether it gets a `declare` naming its fields
/// (only such predicates can be used with named arguments, §4).
type PredSpec = (u32, bool);
type ProgramSpec = (Vec<PredSpec>, Vec<FactSpec>, Vec<RuleSpec>);

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
    let arities = proptest::collection::vec(
        (1u32..=bounds.max_arity, proptest::bool::weighted(0.5)),
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
            proptest::collection::vec(arb_body_atom_spec(), bounds.body_len),
            any::<u8>(),
            proptest::bool::weighted(0.4),
            proptest::collection::vec(arb_head_arg_spec(), MAX_ARITY),
        ),
        bounds.rules,
    );
    (arities, facts, rules)
}

fn arb_body_atom_spec() -> impl Strategy<Value = BodyAtomSpec> {
    (
        any::<u8>(),
        proptest::bool::weighted(0.4),
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
            proptest::collection::vec(arb_body_atom_spec(), 1..=2),
            any::<u8>(),
            proptest::bool::weighted(0.4),
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

fn build_program((arities, facts, rules): ProgramSpec) -> Program {
    let pred_name = |sel: u8| format!("p{}", sel as usize % arities.len());
    let pred_arity = |sel: u8| arities[sel as usize % arities.len()].0 as usize;
    // Only `declare`d predicates may be used with named arguments (§4).
    let has_schema = |sel: u8| arities[sel as usize % arities.len()].1;

    let mut statements = Vec::new();

    // Declarations first, though lowering collects schemas program-wide and
    // does not require it.
    for (index, (arity, declared)) in arities.iter().enumerate() {
        if *declared {
            let fields = (0..*arity as usize)
                .map(|position| field_decl(&field_name(position), None))
                .collect();
            statements.push(declare(&format!("p{index}"), fields));
        }
    }

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

    for (body_specs, head_sel, named_head, head_specs) in rules {
        let mut body = Vec::new();
        // Body variables in first-occurrence order, deduplicated. Only
        // *retained* arguments count: a partially selected named atom does not
        // bind the variables of the fields it omits, so collecting them here
        // is what keeps head variables safe by construction.
        let mut body_vars: Vec<u8> = Vec::new();
        for (sel, named, arg_specs) in body_specs {
            let arity = pred_arity(sel);
            let named = named && has_schema(sel);
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
                    &pred_name(sel),
                    terms
                        .into_iter()
                        .map(|(position, term)| named_arg(&field_name(position), term))
                        .collect(),
                )
            } else {
                positional_atom(
                    &pred_name(sel),
                    terms.into_iter().map(|(_, term)| term).collect(),
                )
            };
            body.push(positive_literal(atom));
        }

        // A named head must supply every field (§4), so head arguments are
        // built at full arity either way.
        let head_terms: Vec<Term> = head_specs
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
        let head = if named_head && has_schema(head_sel) {
            named_atom(
                &pred_name(head_sel),
                head_terms
                    .into_iter()
                    .enumerate()
                    .map(|(position, term)| named_arg(&field_name(position), term))
                    .collect(),
            )
        } else {
            positional_atom(&pred_name(head_sel), head_terms)
        };
        statements.push(rule(head, body));
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

    let statements = program
        .statements
        .iter()
        .map(|statement| match &statement.kind {
            StatementKind::Clause(clause) => Statement {
                kind: StatementKind::Clause(Clause {
                    head: rewrite_atom(&clause.head),
                    body: clause
                        .body
                        .iter()
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
                        .collect(),
                    span: clause.span,
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
}
