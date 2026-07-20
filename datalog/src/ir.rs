//! Core intermediate representation — the evaluator's input.
//!
//! This is the *lowered* form of a program (spec §17, 2026-07-19: surface AST /
//! core IR split), produced by [`crate::lower`] from the surface AST
//! ([`crate::ast`]) and consumed by the engine ([`crate::engine`]), provenance
//! layer ([`crate::provenance`]), fact sources ([`crate::sources`]), and the
//! canonical printer ([`crate::api`]). Everything is resolved and positional:
//!
//! - Predicates are interned to dense [`PredId`]s with a [`PredicateInfo`]
//!   table on [`Program`]; atoms always carry their full declared arity.
//! - Variables are per-rule numbered slots ([`Var`]) with a `var_names` side
//!   table recovering original names (`None` marks lowering-generated fresh
//!   variables from wildcards and partial selection).
//! - Named arguments do not exist here — lowering consumes them entirely.
//! - [`Value`] has total `Eq`/`Hash`/`Ord` (via [`F64`]), so relations are
//!   genuine sets of [`Fact`]s (set semantics, §17) and the derived cross-type
//!   `Ord` (symbol < string < int < float < bool) fixes the deterministic
//!   canonical output order (§14).
//! - `strata` is non-optional: evaluation order is part of the contract, not a
//!   filled-in-later annotation. Until negation lands it is a single stratum.
//!
//! ## Provenance guarantees (§11)
//!
//! Derivations reference IR coordinates, so these are stable by contract:
//! [`RuleId`] is the index into [`Program::rules`] in source order and is never
//! renumbered by later transforms; [`BodyIdx`] indexes into [`Rule::body`],
//! which preserves source literal order — lowering never reorders bodies (an
//! evaluator wanting a different join order maps back internally). Fact
//! identity is [`Fact`]'s `Eq`/`Hash`; original names for proof rendering come
//! from [`PredicateInfo::name`], [`Rule::var_names`], and the retained spans.
//!
//! ## Not in this IR (evaluator-internal)
//!
//! Delta/semi-naive rule rewrites, adornment/magic-sets annotations, join
//! plans, indexes and relation storage, and EDB/IDB classification (derivable:
//! a predicate is intensional iff it heads a rule).
//!
//! Field *names* are retained on [`PredicateInfo`] (§17, 2026-07-20) even
//! though named arguments themselves are gone: they carry no evaluation
//! meaning, but type inference and provenance rendering both need to name
//! columns, and those passes run over the IR with no access to the AST.

use std::hash::{Hash, Hasher};

use crate::ast::{ArithOp, CmpOp, Span};
use crate::error::Error;

/// An interned predicate identity: index into [`Program::predicates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PredId(pub u32);

/// A rule identity: index into [`Program::rules`], in source order.
///
/// Stable by contract — later transforms (magic sets, deltas) produce internal
/// rules that *reference* the originating `RuleId`, never renumber it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub u32);

/// A rule-scoped variable slot: index into [`Rule::var_names`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Var(pub u32);

/// A stable index into [`Rule::body`] — the provenance handle for "which body
/// literal matched this premise".
pub type BodyIdx = usize;

/// Name, arity, and (when known) field names of an interned predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateInfo {
    pub name: String,
    pub arity: u32,
    /// Field names in positional order, from a `declare` or an explicit import
    /// schema; `None` when the predicate has no schema. A schema is known in
    /// full or not at all — the surface syntax cannot name only some fields.
    ///
    /// Invariant: `Some(fields)` implies `fields.len() == arity as usize`.
    ///
    /// Retained for the same reason as [`PredicateInfo::name`] and
    /// [`Rule::var_names`]: rendering and error messages. Named arguments are
    /// still fully resolved by lowering — these names give type inference (§4)
    /// a way to say *which column* conflicts, and let proof trees (§11) print
    /// `employee(name: "alice", …)` instead of eight positional columns.
    pub fields: Option<Vec<String>>,
}

/// A never-NaN `f64` with total `Eq`/`Ord`/`Hash`.
///
/// Invariants enforced by [`F64::new`]: the value is not NaN, and `-0.0` is
/// normalized to `+0.0`. Under both invariants `total_cmp` coincides exactly
/// with `==`/`<`, and bit-hashing agrees with `==` — so facts containing
/// floats are well-behaved set members and sort deterministically (§14).
///
/// NaN is unrepresentable in source (§3 has no NaN token); rejecting it here
/// guards the arithmetic-builtin path, where NaN-producing operations (e.g.
/// `0.0 / 0.0`) become structured runtime errors (§8, open question there).
#[derive(Debug, Clone, Copy)]
pub struct F64(f64);

impl F64 {
    /// Wraps a float, rejecting NaN and normalizing `-0.0` to `+0.0`.
    pub fn new(value: f64) -> crate::Result<F64> {
        if value.is_nan() {
            return Err(Error::Semantic(
                "NaN is not a representable float value".to_string(),
            ));
        }
        // Normalize -0.0 so bit-based hashing agrees with ==.
        Ok(F64(if value == 0.0 { 0.0 } else { value }))
    }

    /// Returns the wrapped float.
    pub fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for F64 {
    fn eq(&self, other: &Self) -> bool {
        // Total given the no-NaN invariant.
        self.0 == other.0
    }
}

impl Eq for F64 {}

impl Hash for F64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Consistent with == given the -0.0 normalization.
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for F64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for F64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Agrees with == and < under the two constructor invariants.
        self.0.total_cmp(&other.0)
    }
}

/// A ground value of one of the five primitive types (§4).
///
/// The derived `Ord` fixes the canonical cross-type sort order — symbol <
/// string < int < float < bool, then within-type — used for deterministic
/// output (§14). Symbols are plain `String`s in v1; interning is a deferred
/// drop-in behind this single choke point (§17).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Value {
    Symbol(String),
    String(String),
    Int(i64),
    Float(F64),
    Bool(bool),
}

/// A ground tuple: one row of a relation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tuple(pub Vec<Value>);

/// A ground fact — the set member of set semantics (§17): a fact derived
/// multiple ways is one fact with multiple derivations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fact {
    pub pred: PredId,
    pub tuple: Tuple,
}

/// A term: variable slot or constant. Flat per §4 — no compound terms in v1.
#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Var(Var),
    Const(Value),
}

/// A positional atom, always at the predicate's full arity.
#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub pred: PredId,
    pub args: Vec<Term>,
}

/// A body literal with the span of its surface form (retained so passes that
/// run after lowering — type inference, stratification — report against
/// source).
#[derive(Debug, Clone, PartialEq)]
pub struct BodyLiteral {
    pub kind: BodyLiteralKind,
    pub span: Span,
}

/// Body literal forms.
#[derive(Debug, Clone, PartialEq)]
pub enum BodyLiteralKind {
    /// A positive atom.
    Atom(Atom),
    /// A negated atom. Representable from day one; stratification rejects it
    /// with "negation not yet supported" until roadmap step 3 lands.
    NegAtom(Atom),
    /// A comparison between arithmetic expressions.
    Compare { op: CmpOp, lhs: Expr, rhs: Expr },
}

/// An arithmetic expression over resolved terms.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Term(Term),
    Binary {
        op: ArithOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

/// A lowered rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub head: Atom,
    /// Body literals in source order — never reordered (provenance contract).
    pub body: Vec<BodyLiteral>,
    /// `Var(i)` → the variable's source name; `None` for lowering-generated
    /// fresh variables (wildcards, partial selection).
    pub var_names: Vec<Option<String>>,
    /// Span of the whole source clause.
    pub span: Span,
}

/// A lowered query. Entries of `var_names` that are `Some` are the projected
/// answer variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub body: Vec<BodyLiteral>,
    pub var_names: Vec<Option<String>>,
    pub span: Span,
}

/// An import binding carried through lowering; inert until the source layer
/// ([`crate::sources`]) lands.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportSpec {
    pub pred: PredId,
    pub path: String,
    pub span: Span,
}

/// A lowered program — the complete evaluator input.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    /// Interning table: `PredId(i)` → `predicates[i]`.
    pub predicates: Vec<PredicateInfo>,
    /// Ground facts from empty-body clauses (and later, imports).
    pub facts: Vec<Fact>,
    /// Rules in source order: `RuleId(i)` → `rules[i]`.
    pub rules: Vec<Rule>,
    pub queries: Vec<Query>,
    pub imports: Vec<ImportSpec>,
    /// Evaluation order: each inner vec is one stratum, evaluated to fixpoint
    /// before the next. A single stratum until stratified negation lands.
    pub strata: Vec<Vec<RuleId>>,
}

impl Program {
    /// Returns the `PredId` for `name`/`arity`, interning it if new.
    ///
    /// Looking up an existing name with a *different* arity is the caller's
    /// error to detect (lowering reports it as a semantic error); this method
    /// matches on name alone and returns the existing id so the caller can
    /// compare arities.
    pub fn intern_pred(&mut self, name: &str, arity: u32) -> PredId {
        if let Some(i) = self.predicates.iter().position(|p| p.name == name) {
            return PredId(i as u32);
        }
        self.predicates.push(PredicateInfo {
            name: name.to_string(),
            arity,
            // Only lowering knows about schemas; callers that have field names
            // set them afterwards.
            fields: None,
        });
        PredId((self.predicates.len() - 1) as u32)
    }

    /// Returns the interned info for `pred`.
    pub fn pred_info(&self, pred: PredId) -> &PredicateInfo {
        &self.predicates[pred.0 as usize]
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Hand-constructed IR fixtures from the spec §16 corpus. The 16.1 fixture
    //! is the exact input the core evaluator's first test will consume.

    use super::*;
    use crate::ast::Span;

    pub(crate) fn string_value(s: &str) -> Value {
        Value::String(s.to_string())
    }

    pub(crate) fn fact2(pred: PredId, a: &str, b: &str) -> Fact {
        Fact {
            pred,
            tuple: Tuple(vec![string_value(a), string_value(b)]),
        }
    }

    /// Spec §16.1 in lowered form, exactly as `lower()` must produce it from
    /// the surface fixture `ast::fixtures::example_16_1()`:
    /// predicates interned in first-appearance order (parent = 0, ancestor =
    /// 1), variables numbered in first-occurrence order per rule, one stratum.
    pub(crate) fn example_16_1() -> Program {
        let parent = PredId(0);
        let ancestor = PredId(1);
        Program {
            predicates: vec![
                PredicateInfo {
                    name: "parent".to_string(),
                    arity: 2,
                    fields: None,
                },
                PredicateInfo {
                    name: "ancestor".to_string(),
                    arity: 2,
                    fields: None,
                },
            ],
            facts: vec![
                fact2(parent, "alice", "bob"),
                fact2(parent, "bob", "carol"),
                fact2(parent, "carol", "dave"),
            ],
            rules: vec![
                // ancestor(X, Y) :- parent(X, Y).
                Rule {
                    head: Atom {
                        pred: ancestor,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    },
                    body: vec![BodyLiteral {
                        kind: BodyLiteralKind::Atom(Atom {
                            pred: parent,
                            args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                        }),
                        span: Span::DUMMY,
                    }],
                    var_names: vec![Some("X".to_string()), Some("Y".to_string())],
                    span: Span::DUMMY,
                },
                // ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
                Rule {
                    head: Atom {
                        pred: ancestor,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    },
                    body: vec![
                        BodyLiteral {
                            kind: BodyLiteralKind::Atom(Atom {
                                pred: parent,
                                args: vec![Term::Var(Var(0)), Term::Var(Var(2))],
                            }),
                            span: Span::DUMMY,
                        },
                        BodyLiteral {
                            kind: BodyLiteralKind::Atom(Atom {
                                pred: ancestor,
                                args: vec![Term::Var(Var(2)), Term::Var(Var(1))],
                            }),
                            span: Span::DUMMY,
                        },
                    ],
                    var_names: vec![
                        Some("X".to_string()),
                        Some("Y".to_string()),
                        Some("Z".to_string()),
                    ],
                    span: Span::DUMMY,
                },
            ],
            queries: vec![
                // ?- ancestor("alice", Who).
                Query {
                    body: vec![BodyLiteral {
                        kind: BodyLiteralKind::Atom(Atom {
                            pred: ancestor,
                            args: vec![Term::Const(string_value("alice")), Term::Var(Var(0))],
                        }),
                        span: Span::DUMMY,
                    }],
                    var_names: vec![Some("Who".to_string())],
                    span: Span::DUMMY,
                },
            ],
            imports: Vec::new(),
            strata: vec![vec![RuleId(0), RuleId(1)]],
        }
    }

    /// Spec §16.7 in lowered form, exactly as `lower()` must produce it from
    /// `ast::fixtures::example_16_7()`.
    ///
    /// This is where named arguments disappear: `employee(name: N, title:
    /// "manager")` becomes a full-arity eight-argument atom whose six omitted
    /// fields are fresh anonymous slots, and `person(name: N, age: A)` becomes
    /// the plain positional `person(N, A)`. Predicates intern in
    /// first-appearance order (employee = 0, manager_name = 1, person = 2,
    /// adult = 3).
    pub(crate) fn example_16_7() -> Program {
        let employee = PredId(0);
        let manager_name = PredId(1);
        let person = PredId(2);
        let adult = PredId(3);
        Program {
            predicates: vec![
                // Field names survive lowering: `employee` from the explicit
                // import schema, `person` from its `declare`. The two rule
                // heads were never declared, so they carry none.
                PredicateInfo {
                    name: "employee".to_string(),
                    arity: 8,
                    fields: Some(
                        [
                            "id",
                            "name",
                            "age",
                            "dept",
                            "title",
                            "salary",
                            "city",
                            "start_date",
                        ]
                        .iter()
                        .map(|f| f.to_string())
                        .collect(),
                    ),
                },
                PredicateInfo {
                    name: "manager_name".to_string(),
                    arity: 1,
                    fields: None,
                },
                PredicateInfo {
                    name: "person".to_string(),
                    arity: 2,
                    fields: Some(vec!["name".to_string(), "age".to_string()]),
                },
                PredicateInfo {
                    name: "adult".to_string(),
                    arity: 1,
                    fields: None,
                },
            ],
            facts: vec![Fact {
                pred: person,
                tuple: Tuple(vec![string_value("alice"), Value::Int(30)]),
            }],
            rules: vec![
                // manager_name(N) :- employee(name: N, title: "manager").
                // Slot 0 is N (numbered from the head); the six omitted fields
                // take fresh slots in schema order, skipping the two supplied.
                Rule {
                    head: Atom {
                        pred: manager_name,
                        args: vec![Term::Var(Var(0))],
                    },
                    body: vec![BodyLiteral {
                        kind: BodyLiteralKind::Atom(Atom {
                            pred: employee,
                            args: vec![
                                Term::Var(Var(1)),                    // id
                                Term::Var(Var(0)),                    // name = N
                                Term::Var(Var(2)),                    // age
                                Term::Var(Var(3)),                    // dept
                                Term::Const(string_value("manager")), // title
                                Term::Var(Var(4)),                    // salary
                                Term::Var(Var(5)),                    // city
                                Term::Var(Var(6)),                    // start_date
                            ],
                        }),
                        span: Span::DUMMY,
                    }],
                    var_names: vec![Some("N".to_string()), None, None, None, None, None, None],
                    span: Span::DUMMY,
                },
                // adult(N) :- person(name: N, age: A), A >= 18.
                Rule {
                    head: Atom {
                        pred: adult,
                        args: vec![Term::Var(Var(0))],
                    },
                    body: vec![
                        BodyLiteral {
                            kind: BodyLiteralKind::Atom(Atom {
                                pred: person,
                                args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                            }),
                            span: Span::DUMMY,
                        },
                        BodyLiteral {
                            kind: BodyLiteralKind::Compare {
                                op: CmpOp::Ge,
                                lhs: Expr::Term(Term::Var(Var(1))),
                                rhs: Expr::Term(Term::Const(Value::Int(18))),
                            },
                            span: Span::DUMMY,
                        },
                    ],
                    var_names: vec![Some("N".to_string()), Some("A".to_string())],
                    span: Span::DUMMY,
                },
            ],
            queries: Vec::new(),
            imports: vec![ImportSpec {
                pred: employee,
                path: "data/employees.csv".to_string(),
                span: Span::DUMMY,
            }],
            strata: vec![vec![RuleId(0), RuleId(1)]],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::fixtures::*;
    use super::*;

    #[test]
    fn f64_rejects_nan_and_normalizes_negative_zero() {
        assert!(F64::new(f64::NAN).is_err());
        assert!(F64::new(f64::INFINITY).is_ok());

        let pos = F64::new(0.0).unwrap();
        let neg = F64::new(-0.0).unwrap();
        assert_eq!(pos, neg);
        assert_eq!(pos.get().to_bits(), neg.get().to_bits());

        let mut set = HashSet::new();
        set.insert(pos);
        assert!(set.contains(&neg));
    }

    #[test]
    fn f64_ordering_is_sane() {
        let mut values = [
            F64::new(1.5).unwrap(),
            F64::new(f64::NEG_INFINITY).unwrap(),
            F64::new(0.0).unwrap(),
            F64::new(f64::INFINITY).unwrap(),
            F64::new(-2.0).unwrap(),
        ];
        values.sort();
        let raw: Vec<f64> = values.iter().map(|v| v.get()).collect();
        assert_eq!(raw, vec![f64::NEG_INFINITY, -2.0, 0.0, 1.5, f64::INFINITY]);
    }

    #[test]
    fn value_canonical_order_is_symbol_string_int_float_bool() {
        let mut values = [
            Value::Bool(false),
            Value::Float(F64::new(1.0).unwrap()),
            Value::Int(5),
            Value::String("a".to_string()),
            Value::Symbol("z".to_string()),
        ];
        values.sort();
        assert!(matches!(values[0], Value::Symbol(_)));
        assert!(matches!(values[1], Value::String(_)));
        assert!(matches!(values[2], Value::Int(_)));
        assert!(matches!(values[3], Value::Float(_)));
        assert!(matches!(values[4], Value::Bool(_)));
    }

    #[test]
    fn symbols_and_strings_never_compare_equal() {
        assert_ne!(
            Value::Symbol("alice".to_string()),
            Value::String("alice".to_string())
        );
    }

    #[test]
    fn facts_collapse_as_set_members() {
        let program = example_16_1();
        let mut set: HashSet<Fact> = HashSet::new();
        for fact in &program.facts {
            set.insert(fact.clone());
            set.insert(fact.clone()); // duplicate insertion is a no-op
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn intern_pred_is_idempotent_and_dense() {
        let mut program = Program::default();
        let p = program.intern_pred("parent", 2);
        let a = program.intern_pred("ancestor", 2);
        assert_eq!(p, PredId(0));
        assert_eq!(a, PredId(1));
        assert_eq!(program.intern_pred("parent", 2), p);
        assert_eq!(program.predicates.len(), 2);
        assert_eq!(program.pred_info(a).name, "ancestor");
    }

    // --- Phase A properties A1–A5 (testing.md) ---

    mod properties {
        use std::collections::HashSet;
        use std::hash::{DefaultHasher, Hash, Hasher};

        use proptest::prelude::*;

        use super::super::*;
        use crate::testgen::arb_value;

        fn hash_of<T: Hash>(value: &T) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        /// Rank of a value's type in the canonical cross-type order (§14).
        fn type_rank(value: &Value) -> u8 {
            match value {
                Value::Symbol(_) => 0,
                Value::String(_) => 1,
                Value::Int(_) => 2,
                Value::Float(_) => 3,
                Value::Bool(_) => 4,
            }
        }

        proptest! {
            /// A1 — F64 order is total and sorting is deterministic.
            #[test]
            fn a1_f64_total_order(bits in proptest::collection::vec(any::<u64>(), 0..20)) {
                let mut values: Vec<F64> = bits
                    .iter()
                    .filter_map(|b| F64::new(f64::from_bits(*b)).ok())
                    .collect();
                let mut again = values.clone();
                values.sort();
                again.sort();
                prop_assert_eq!(&values, &again);
                for pair in values.windows(2) {
                    prop_assert!(pair[0] <= pair[1]);
                    // Trichotomy: cmp and == always agree.
                    prop_assert_eq!(
                        pair[0].cmp(&pair[1]) == std::cmp::Ordering::Equal,
                        pair[0] == pair[1]
                    );
                }
            }

            /// A2 — equality implies hash equality.
            #[test]
            fn a2_f64_eq_implies_hash_eq(b1 in any::<u64>(), b2 in any::<u64>()) {
                if let (Ok(x), Ok(y)) =
                    (F64::new(f64::from_bits(b1)), F64::new(f64::from_bits(b2)))
                    && x == y
                {
                    prop_assert_eq!(hash_of(&x), hash_of(&y));
                }
            }

            /// A3 — every NaN bit pattern is rejected; everything else
            /// round-trips (modulo -0.0 normalization).
            #[test]
            fn a3_f64_constructor_invariants(bits in any::<u64>()) {
                let raw = f64::from_bits(bits);
                match F64::new(raw) {
                    Err(_) => prop_assert!(raw.is_nan()),
                    Ok(v) => {
                        prop_assert!(!raw.is_nan());
                        let expected = if raw == 0.0 { 0.0f64 } else { raw };
                        prop_assert_eq!(v.get().to_bits(), expected.to_bits());
                    }
                }
            }

            /// A4 — Value's Ord is lawful for sorting and respects the
            /// canonical cross-type order.
            #[test]
            fn a4_value_canonical_order(values in proptest::collection::vec(arb_value(), 0..20)) {
                let mut sorted = values.clone();
                let mut again = values.clone();
                sorted.sort();
                again.sort();
                prop_assert_eq!(&sorted, &again);
                for pair in sorted.windows(2) {
                    prop_assert!(pair[0] <= pair[1]);
                    prop_assert!(type_rank(&pair[0]) <= type_rank(&pair[1]));
                }
                // Sorting is a permutation.
                prop_assert_eq!(sorted.len(), values.len());
                for value in &values {
                    prop_assert!(sorted.contains(value));
                }
            }

            /// A5 — facts behave as set members: HashSet size equals the
            /// Ord-deduplicated size, however many duplicates are inserted.
            #[test]
            fn a5_fact_set_semantics(
                tuples in proptest::collection::vec(
                    (0u32..3, proptest::collection::vec(arb_value(), 0..3)),
                    0..20,
                ),
            ) {
                let facts: Vec<Fact> = tuples
                    .into_iter()
                    .map(|(pred, values)| Fact {
                        pred: PredId(pred),
                        tuple: Tuple(values),
                    })
                    .collect();
                let hashed: HashSet<Fact> = facts.iter().cloned().collect();
                let mut deduped = facts.clone();
                deduped.sort();
                deduped.dedup();
                prop_assert_eq!(hashed.len(), deduped.len());
                for fact in &deduped {
                    prop_assert!(hashed.contains(fact));
                }
            }
        }
    }

    #[test]
    fn example_16_1_fixture_shape() {
        let program = example_16_1();
        assert_eq!(program.predicates.len(), 2);
        assert_eq!(program.facts.len(), 3);
        assert_eq!(program.rules.len(), 2);
        assert_eq!(program.queries.len(), 1);
        assert_eq!(program.strata, vec![vec![RuleId(0), RuleId(1)]]);

        // Recursive rule: three variable slots, all named.
        let recursive = &program.rules[1];
        assert_eq!(recursive.var_names.len(), 3);
        assert!(recursive.var_names.iter().all(|n| n.is_some()));

        // Every atom is at full arity and references an interned predicate.
        for rule in &program.rules {
            let info = program.pred_info(rule.head.pred);
            assert_eq!(rule.head.args.len(), info.arity as usize);
        }
    }
}
