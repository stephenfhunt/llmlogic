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
//!   filled-in-later annotation. Lowering's stratification (§7) populates it;
//!   positive programs form a single stratum.
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

use crate::ast::{AggOp, ArithOp, CmpOp, Span, TypeName};
use crate::error::{Error, ErrorCode};

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
    /// Declared column types in positional order, from the type annotations of a
    /// `declare` or explicit import schema (§4). A position is `None` when the
    /// field was named without a type; the whole field is `None` when the
    /// predicate has no schema at all.
    ///
    /// Invariant: `field_types` is `Some` **iff** [`fields`](Self::fields) is
    /// `Some`, with the same length. (The grammar forbids a type without a name,
    /// so types never appear without a schema.) These are the *asserted* types
    /// type inference (§4) verifies against the *inferred* ones; imported
    /// *inferred* column types (§13) are a separate, later channel.
    pub field_types: Option<Vec<Option<TypeName>>>,
    /// Where the schema was written, when a `declare` or an import schema wrote
    /// one. Retained for the same reason as the two above: it is what lets a
    /// diagnostic about a *declared* column point at the declaration rather
    /// than at whichever rule happened to contradict it (§12).
    pub decl_span: Option<Span>,
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
            return Err(Error::new(
                ErrorCode::ArithmeticError,
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

/// `n` as an `f64`, or `None` when no `f64` represents it exactly.
///
/// The single implementation of §8's **numeric conversion is exact or it is an
/// error** rule, shared by the `as` cast and §13's import coercion so the two
/// cannot disagree about which values survive. Above 2⁵³ an `i64` generally has
/// no exact `f64`, and silently rounding an identifier corrupts every join built
/// on it — which is why this is a refusal rather than a rounding.
///
/// The test round-trips through `i128`, which — unlike `as i64` — cannot
/// saturate and so cannot report a lossy conversion as exact.
pub(crate) fn i64_as_exact_f64(n: i64) -> Option<f64> {
    let widened = n as f64;
    (widened as i128 == i128::from(n)).then_some(widened)
}

/// `f` as an `i64`, or `None` when it has a fractional part or falls outside
/// the `i64` range. The narrowing direction of the same rule.
pub(crate) fn f64_as_exact_i64(f: f64) -> Option<i64> {
    if !f.is_finite() || f.fract() != 0.0 {
        return None;
    }
    // `as i128` cannot saturate over any finite `f64` (i128's range dwarfs the
    // exactly-representable integers), so the bounds check below is real.
    let n = f as i128;
    (n >= i128::from(i64::MIN) && n <= i128::from(i64::MAX)).then_some(n as i64)
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

/// A symbol's or string's text, held once for the process (§17 2026-09-14,
/// `notes/interning.md`). Equal texts are one reference, so equality and hashing
/// compare pointers; order compares the texts, which is §14's order. The field is
/// private: every `Sym` comes from the interner, and pointer equality is content
/// equality only because of that. The interner never frees a text, which is the
/// design's recorded cost for a long-lived process.
#[derive(Clone, Copy)]
pub struct Sym(&'static str);

impl Sym {
    /// The text, interned: the one `Sym` held for it, made on first sight.
    pub fn intern(text: &str) -> Sym {
        use std::collections::HashSet;
        use std::sync::{Mutex, OnceLock};
        static INTERNER: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
        let mut held = INTERNER
            .get_or_init(Default::default)
            .lock()
            .expect("the interner's lock is not poisoned");
        if let Some(&existing) = held.get(text) {
            return Sym(existing);
        }
        let text: &'static str = Box::leak(text.to_owned().into_boxed_str());
        held.insert(text);
        Sym(text)
    }

    /// The text.
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl PartialEq for Sym {
    fn eq(&self, other: &Sym) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for Sym {}

impl Hash for Sym {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0.as_ptr() as usize).hash(state);
    }
}

impl Ord for Sym {
    fn cmp(&self, other: &Sym) -> std::cmp::Ordering {
        if std::ptr::eq(self.0, other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(other.0)
        }
    }
}

impl PartialOrd for Sym {
    fn partial_cmp(&self, other: &Sym) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Debug for Sym {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.0, f)
    }
}

/// A ground value: the missing-data value [`absent`](Value::Absent), or one of
/// the five primitive types (§4).
///
/// The derived `Ord` fixes the canonical cross-type sort order — absent <
/// symbol < string < int < float < bool, then within-type — used for
/// deterministic output (§14). A symbol's or string's text is a [`Sym`], held once
/// for the process (§17 2026-09-14, `notes/interning.md`).
///
/// # Two notions of "same" (§4)
///
/// The derived `Eq`/`Ord`/`Hash` are the **structural** notion: `Absent ==
/// Absent`, so a relation holds a single copy of `p(absent)` and `absent` has a
/// fixed sort position. That is what set storage, deduplication, canonical
/// output order (§14) and test assertions all want, which is why it is the
/// derive.
///
/// The **semantic** notion — `absent` matches *nothing*, including another
/// `absent`, so a missing foreign key never joins another into a cartesian
/// blowup — is [`Value::unifies_with`]. Every join and unification site must
/// use it; `==` at such a site is silently wrong.
///
/// **The anti-join is the one exception** (§4/§7): a negated atom binds
/// nothing, so refutation is a *membership* test rather than a join and
/// compares structurally — `not p(X)` with `X` bound to `absent` asks whether
/// `p(absent)` is in the relation, and it is. The blowup the semantic notion
/// prevents is a property of joins bringing in new bindings, which membership
/// never does. The single site is [`crate::provenance::NoMatchPattern::matches`].
///
/// **If you are matching two values, you almost certainly want
/// `unifies_with`, not `==`.** The rule lives here, as a method, precisely so
/// it is discoverable from the type: it was previously only a free function in
/// `engine`, and `engine::naive` — written without it in view — used plain `==`
/// at four match sites and diverged from the engine on every absent value for
/// as long as no generator emitted one (§17, 2026-07-25). What catches that
/// class of mistake is the differential (`b1_absent_programs_agree`); what makes
/// it less likely is having one obvious place to reach for.
///
/// Testing for absence itself *is* structural — use [`Value::is_absent`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Value {
    /// The missing-data value (§4): type-neutral, inhabits any column,
    /// two-valued (annihilates in arithmetic, false in comparisons). First
    /// variant so the derived `Ord` sorts it before every typed value.
    Absent,
    Symbol(Sym),
    String(Sym),
    Int(i64),
    Float(F64),
    Bool(bool),
    /// A civil day (§4). After `Bool` because the variant order *is* the
    /// cross-type output order (§14), and the temporal types were added to it.
    Date(crate::temporal::Date),
    /// A civil date and time, microsecond precision (§4).
    Timestamp(crate::temporal::Timestamp),
    /// An exact elapsed quantity, in microseconds (§4).
    Duration(crate::temporal::Duration),
}

impl Value {
    /// The symbol `text`, interned.
    pub fn symbol(text: &str) -> Value {
        Value::Symbol(Sym::intern(text))
    }

    /// The string `text`, interned.
    pub fn string(text: &str) -> Value {
        Value::String(Sym::intern(text))
    }

    /// Is this the missing-data value? A **structural** test — `absent` is
    /// perfectly identifiable, it just does not *match* anything (§4).
    pub fn is_absent(&self) -> bool {
        matches!(self, Value::Absent)
    }

    /// The **semantic** sameness of two ground values (§4): structural equality,
    /// except that `absent` unifies with nothing — not with a value, and not
    /// with another `absent`.
    ///
    /// This is the notion every join and unification uses
    /// ([`crate::engine`]'s `try_match`), but **not** the anti-join, which is a
    /// structural membership test — see the type-level note above.
    /// It is deliberately *not* `PartialEq`: the derive is the structural
    /// notion that set storage and canonical output order need, and both are
    /// load-bearing.
    pub fn unifies_with(&self, other: &Value) -> bool {
        !self.is_absent() && self == other
    }
}

/// A ground tuple: one row of a relation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tuple(pub Vec<Value>);

/// A tuple is looked up by its row. `Vec`'s `Eq`, `Ord` and `Hash` are its
/// slice's, so a set of tuples orders and finds a row exactly as it does the
/// tuple holding it.
impl std::borrow::Borrow<[Value]> for Tuple {
    fn borrow(&self) -> &[Value] {
        &self.0
    }
}

/// A ground fact — the set member of set semantics (§17): a fact derived
/// multiple ways is one fact with multiple derivations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fact {
    pub pred: PredId,
    pub tuple: Tuple,
}

/// One data import's rows (§13): `rows` rows of `arity` values laid end to end,
/// sorted and deduplicated, so no row is its own allocation
/// (`notes/fact-store.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRows {
    pub pred: PredId,
    pub arity: usize,
    pub rows: usize,
    pub values: Vec<Value>,
    /// How many facts written in the program text precede the import statement:
    /// [`Program::base_facts`] yields the block at this position among them.
    pub at: usize,
}

/// Sorts and deduplicates `rows` rows of `arity` values laid end to end. A row's
/// values move to their place, never copied, and no row becomes its own
/// allocation. Returns the rows kept and their count.
pub(crate) fn sort_dedup_rows(
    mut values: Vec<Value>,
    rows: usize,
    arity: usize,
) -> (Vec<Value>, usize) {
    debug_assert_eq!(values.len(), rows * arity, "rows are rectangular");
    let mut order: Vec<usize> = (0..rows).collect();
    {
        let row = |index: usize| &values[index * arity..(index + 1) * arity];
        order.sort_unstable_by(|&a, &b| row(a).cmp(row(b)));
        order.dedup_by(|a, b| row(*a) == row(*b));
    }
    let mut sorted = Vec::with_capacity(order.len() * arity);
    for &index in &order {
        for value in &mut values[index * arity..(index + 1) * arity] {
            sorted.push(std::mem::replace(value, Value::Absent));
        }
    }
    (sorted, order.len())
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
    /// A negated atom (§7): holds when no fact of the predicate matches, with
    /// `None`-named slots existential under the negation. Stratification
    /// places its predicate's defining rules in a strictly lower stratum.
    NegAtom(Atom),
    /// A comparison between arithmetic expressions.
    Compare { op: CmpOp, lhs: Expr, rhs: Expr },
    /// A presence test `expr is [not] absent` (§4/§8): a filter that holds iff
    /// `expr` evaluates to [`Value::Absent`] (`negated` flips it). Like a
    /// comparison it binds nothing and adds no stratum; its operand must be
    /// bound by a positive atom (§10). Distinct from [`Self::Compare`] because
    /// it has no [`CmpOp`] and partitions rows (both `=`/`!=` are false on
    /// absent), and distinct from [`Self::NegAtom`] because its `not` is the
    /// operator's, not atom-negation.
    Presence { expr: Expr, negated: bool },
    /// A set-builder aggregate (§9), the lowered form of `result = op { expr |
    /// goal }`. Evaluated once per binding of the group keys — the enclosing
    /// rule's variables that occur in `goal` and are already bound when this
    /// literal is reached — it folds `op` over the multiset of `expr` across the
    /// distinct witness tuples of `goal`, honouring the absent rules (§9: skip
    /// for sum/avg/min/max, count includes absent bindings, empty → absent), and
    /// binds `result` like an `=`-assignment. Its `goal` predicates sit in a
    /// strictly lower stratum (like negation), so the inputs are complete when it
    /// runs. `params` is empty for the v1 five (reserved for `percentile(p)`).
    Aggregate {
        result: Var,
        op: AggOp,
        params: Vec<Expr>,
        expr: Expr,
        goal: Vec<BodyLiteral>,
    },
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
    /// An explicit conversion `expr as type` (§8, ratified 2026-07-25). Unlike
    /// a surface aggregate this survives lowering: the conversion is per-row, so
    /// there is nothing to hoist and the evaluator applies it in place.
    Cast {
        expr: Box<Expr>,
        ty: TypeName,
    },
    /// A `std` module relation applied to its inputs (§13). The surface form is
    /// an atom — `year(D, Y)` — which lowering turns into the `=`-assignment
    /// `Y = year(D)`, so scheduling, safety and the assignment-vs-filter rule
    /// all apply to it unchanged, and the output position may equally be a
    /// constant (`day(D, 15)` is a filter).
    Builtin {
        op: crate::ast::BuiltinOp,
        args: Vec<Expr>,
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

/// A lowered query.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub body: Vec<BodyLiteral>,
    /// `Var(i)` → the variable's source name; `None` for lowering-generated
    /// fresh variables (wildcards, partial selection, aggregate results).
    pub var_names: Vec<Option<String>>,
    /// The answer variables, as slots in ascending order: the *named* variables
    /// the body binds at the top level. Computed by lowering rather than
    /// re-derived from `var_names`, because being named is not the same as being
    /// bound: an aggregate's goal-local variables (§9) are named and are bound
    /// only inside the aggregate's sub-join, so `?- N = count { C | p(P, C) }.`
    /// names `C` and `P` but answers over `N` alone.
    pub projection: Vec<u32>,
    pub span: Span,
}

/// A lowered explanation goal (§11): one ground fact, and the sigil that asked.
///
/// The goal is a [`Fact`] rather than an atom because groundness is checked in
/// lowering — a variable in it is rejected there, pointing at `?-` — so by the
/// time the engine sees it there is nothing left to bind.
#[derive(Debug, Clone, PartialEq)]
pub struct Explanation {
    pub sigil: crate::ast::Sigil,
    pub goal: Fact,
    pub span: Span,
}

impl Program {
    /// What this program's goals require the run to record (§17, 2026-08-21).
    ///
    /// A `?why` goal needs a derivation store; `?whynot` re-solves and needs
    /// none. The cross case — `?whynot` over a fact that turns out to hold — is
    /// *not* provisioned for here, because whether it holds is not known until
    /// the fixpoint has run: that case re-evaluates instead.
    ///
    /// A program that only *reports* through provenance gets
    /// [`Provenance::Reports`](crate::engine::Provenance::Reports), not the full
    /// store: the §9/§12 counts are read off the premises that skipped, and
    /// nothing asked for a proof (§17, 2026-09-11).
    pub fn provenance(&self) -> crate::engine::Provenance {
        self.provenance_pruned(None)
    }

    /// [`Program::provenance`] for a run that evaluates only the `live` rules
    /// ([`crate::lower::live_predicates`]; `None` is every rule). A rule pruned
    /// away produces no premise, so an aggregate in it is no reason to keep a
    /// store.
    pub fn provenance_pruned(&self, live: Option<&[bool]>) -> crate::engine::Provenance {
        let asked = self
            .explanations
            .iter()
            .any(|explanation| explanation.sigil == crate::ast::Sigil::Why);
        if asked {
            crate::engine::Provenance::Recorded
        } else if self.reports_through_provenance_pruned(live) {
            crate::engine::Provenance::Reports
        } else {
            crate::engine::Provenance::Unrecorded
        }
    }

    /// Does any **rule** carry a §9 aggregate or an §8 conversion?
    ///
    /// These are the second reason a run needs the store, and they are not a
    /// concession: §9's skip count and §12's *malformed* count are read back out
    /// of the recorded premises, deduplicated by rule instance, because a
    /// counter beside the fixpoint would count a rediscovered instance twice
    /// (§17, 2026-07-24 and 2026-08-16). "Skip but **report**" is a provenance
    /// surface that predates the asking form, so a program with an aggregate in
    /// a rule provisions a recorder whether or not it asks anything — the
    /// reporting one ([`Provenance::Reports`](crate::engine::Provenance::Reports)),
    /// which keeps only the derivations those counts are read from.
    ///
    /// A *query*'s aggregate needs nothing here: `Model::answer_reporting`
    /// hands its premises straight to the caller and the model never stores
    /// them (§14).
    pub fn reports_through_provenance(&self) -> bool {
        self.reports_through_provenance_pruned(None)
    }

    /// [`Program::reports_through_provenance`] over only the rules a pruned run
    /// evaluates — those whose head is `live` (`None` is every rule).
    pub fn reports_through_provenance_pruned(&self, live: Option<&[bool]>) -> bool {
        self.rules
            .iter()
            .filter(|rule| live.is_none_or(|live| live[rule.head.pred.0 as usize]))
            .any(|rule| rule.body.iter().any(Self::literal_reports))
    }

    /// Can this body literal produce a premise a §9 or §12 warning reads?
    fn literal_reports(literal: &BodyLiteral) -> bool {
        fn casts(expr: &Expr) -> bool {
            match expr {
                Expr::Cast { .. } => true,
                Expr::Binary { lhs, rhs, .. } => casts(lhs) || casts(rhs),
                Expr::Builtin { args, .. } => args.iter().any(casts),
                Expr::Term(_) => false,
            }
        }
        match &literal.kind {
            BodyLiteralKind::Aggregate { .. } => true,
            BodyLiteralKind::Compare { lhs, rhs, .. } => casts(lhs) || casts(rhs),
            _ => false,
        }
    }
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
    /// Ground facts written in the program text: empty-body clauses. Imported
    /// rows are [`Program::imported`]; [`Program::base_facts`] reads both.
    pub facts: Vec<Fact>,
    /// Where a fact written in the program text was written, for a diagnostic
    /// that has to point at it (`bugs/009`). A side table and not a field of
    /// [`Fact`], whose identity is its value — a fact derived two ways is one
    /// fact (§17). Imported rows have no entry: their place is a source row,
    /// not a span of the program. A repeated fact keeps its first place.
    pub fact_spans: std::collections::HashMap<Fact, Span>,
    /// Each data import's rows, one flat block per import, in source order.
    pub imported: Vec<ImportedRows>,
    /// Rules in source order: `RuleId(i)` → `rules[i]`.
    pub rules: Vec<Rule>,
    pub queries: Vec<Query>,
    /// Explanation goals in source order (§11): `?why` / `?whynot`.
    pub explanations: Vec<Explanation>,
    pub imports: Vec<ImportSpec>,
    /// Evaluation order: each inner vec is one stratum, evaluated to fixpoint
    /// before the next (§7). Rules keep source order within a stratum; a
    /// positive program is a single stratum.
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
            field_types: None,
            decl_span: None,
        });
        PredId((self.predicates.len() - 1) as u32)
    }

    /// Every base fact, written or imported, in source order: each import's rows
    /// at its statement's place among the written facts. A written fact carries
    /// itself, for the tables keyed by one ([`Program::fact_spans`]).
    pub fn base_facts(&self) -> impl Iterator<Item = (PredId, &[Value], Option<&Fact>)> + '_ {
        let (mut next_block, mut next_fact) = (0, 0);
        let mut block: Option<(&ImportedRows, usize)> = None;
        std::iter::from_fn(move || {
            loop {
                if let Some((rows, row)) = block {
                    if row < rows.rows {
                        block = Some((rows, row + 1));
                        let start = row * rows.arity;
                        return Some((rows.pred, &rows.values[start..start + rows.arity], None));
                    }
                    block = None;
                }
                if let Some(rows) = self.imported.get(next_block)
                    && rows.at <= next_fact
                {
                    block = Some((rows, 0));
                    next_block += 1;
                    continue;
                }
                let fact = self.facts.get(next_fact)?;
                next_fact += 1;
                return Some((fact.pred, fact.tuple.0.as_slice(), Some(fact)));
            }
        })
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
        Value::string(s)
    }

    pub(crate) fn fact2(pred: PredId, a: &str, b: &str) -> Fact {
        Fact {
            pred,
            tuple: Tuple(vec![string_value(a), string_value(b)]),
        }
    }

    pub(crate) fn fact1(pred: PredId, a: &str) -> Fact {
        Fact {
            pred,
            tuple: Tuple(vec![string_value(a)]),
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
            fact_spans: Default::default(),
            imported: Vec::new(),
            predicates: vec![
                PredicateInfo {
                    name: "parent".to_string(),
                    arity: 2,
                    fields: None,
                    field_types: None,
                    decl_span: None,
                },
                PredicateInfo {
                    name: "ancestor".to_string(),
                    arity: 2,
                    fields: None,
                    field_types: None,
                    decl_span: None,
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
                    projection: vec![0],
                    span: Span::DUMMY,
                },
            ],
            explanations: Vec::new(),
            imports: Vec::new(),
            strata: vec![vec![RuleId(0), RuleId(1)]],
        }
    }

    /// Spec §16.2 in lowered form, exactly as `lower()` must produce it from
    /// `ast::fixtures::example_16_2()`: predicates interned in
    /// first-appearance order (person = 0, parent = 1, root = 2), the wildcard
    /// under negation a fresh `None`-named slot, and a single stratum — `root`
    /// negates only the EDB predicate `parent`, so no rule needs to wait on
    /// another (its stratum *number* is 1, but empty levels are dropped).
    pub(crate) fn example_16_2() -> Program {
        let person = PredId(0);
        let parent = PredId(1);
        let root = PredId(2);
        Program {
            fact_spans: Default::default(),
            imported: Vec::new(),
            predicates: vec![
                PredicateInfo {
                    name: "person".to_string(),
                    arity: 1,
                    fields: None,
                    field_types: None,
                    decl_span: None,
                },
                PredicateInfo {
                    name: "parent".to_string(),
                    arity: 2,
                    fields: None,
                    field_types: None,
                    decl_span: None,
                },
                PredicateInfo {
                    name: "root".to_string(),
                    arity: 1,
                    fields: None,
                    field_types: None,
                    decl_span: None,
                },
            ],
            facts: vec![
                fact1(person, "alice"),
                fact1(person, "bob"),
                fact1(person, "carol"),
                fact2(parent, "alice", "bob"),
                fact2(parent, "bob", "carol"),
            ],
            rules: vec![
                // root(X) :- person(X), not parent(_, X).
                Rule {
                    head: Atom {
                        pred: root,
                        args: vec![Term::Var(Var(0))],
                    },
                    body: vec![
                        BodyLiteral {
                            kind: BodyLiteralKind::Atom(Atom {
                                pred: person,
                                args: vec![Term::Var(Var(0))],
                            }),
                            span: Span::DUMMY,
                        },
                        BodyLiteral {
                            kind: BodyLiteralKind::NegAtom(Atom {
                                pred: parent,
                                args: vec![Term::Var(Var(1)), Term::Var(Var(0))],
                            }),
                            span: Span::DUMMY,
                        },
                    ],
                    var_names: vec![Some("X".to_string()), None],
                    span: Span::DUMMY,
                },
            ],
            queries: Vec::new(),
            explanations: Vec::new(),
            imports: Vec::new(),
            strata: vec![vec![RuleId(0)]],
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
            fact_spans: Default::default(),
            imported: Vec::new(),
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
                    // The explicit import schema types every column (§4).
                    field_types: Some(vec![
                        Some(TypeName::Int),
                        Some(TypeName::String),
                        Some(TypeName::Int),
                        Some(TypeName::String),
                        Some(TypeName::String),
                        Some(TypeName::Int),
                        Some(TypeName::String),
                        Some(TypeName::String),
                    ]),
                    decl_span: Some(Span::DUMMY),
                },
                PredicateInfo {
                    name: "manager_name".to_string(),
                    arity: 1,
                    fields: None,
                    field_types: None,
                    decl_span: None,
                },
                PredicateInfo {
                    name: "person".to_string(),
                    arity: 2,
                    fields: Some(vec!["name".to_string(), "age".to_string()]),
                    field_types: Some(vec![Some(TypeName::String), Some(TypeName::Int)]),
                    decl_span: Some(Span::DUMMY),
                },
                PredicateInfo {
                    name: "adult".to_string(),
                    arity: 1,
                    fields: None,
                    field_types: None,
                    decl_span: None,
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
            explanations: Vec::new(),
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
            Value::string("a"),
            Value::symbol("z"),
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
        assert_ne!(Value::symbol("alice"), Value::string("alice"));
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

    // --- Phase A properties A1–A5 and A17 (testing.md) ---

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

        /// The characters texts are drawn from: one-byte and multi-byte.
        const TEXT_CHARS: [char; 4] = ['a', 'b', 'é', '日'];

        /// Two texts: the first text rebuilt in a fresh allocation, the first text
        /// with one character replaced, or an unrelated draw. Either may be empty.
        fn arb_text_pair() -> impl Strategy<Value = (String, String)> {
            let text = || {
                proptest::collection::vec(prop::sample::select(TEXT_CHARS.to_vec()), 0..4)
                    .prop_map(|chars| chars.into_iter().collect::<String>())
            };
            (
                text(),
                text(),
                0usize..3,
                any::<prop::sample::Index>(),
                prop::sample::select(TEXT_CHARS.to_vec()),
            )
                .prop_map(|(first, unrelated, shape, at, replacement)| {
                    let mut chars: Vec<char> = first.chars().collect();
                    let second = match shape {
                        0 => chars.into_iter().collect(),
                        1 if !chars.is_empty() => {
                            let position = at.index(chars.len());
                            chars[position] = replacement;
                            chars.into_iter().collect()
                        }
                        _ => unrelated,
                    };
                    (first, second)
                })
        }

        /// Rank of a value's type in the canonical cross-type order (§14):
        /// absent sorts before every typed value.
        fn type_rank(value: &Value) -> u8 {
            match value {
                Value::Absent => 0,
                Value::Symbol(_) => 1,
                Value::String(_) => 2,
                Value::Int(_) => 3,
                Value::Float(_) => 4,
                Value::Bool(_) => 5,
                Value::Date(_) => 6,
                Value::Timestamp(_) => 7,
                Value::Duration(_) => 8,
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

            /// **A17** — interned equality is content equality. Two interned texts
            /// are equal exactly when the texts are, hash alike when equal, and
            /// order as the texts do; so do the symbol and string values made from
            /// them, and a symbol never equals the string of the same text.
            ///
            /// The oracle is `str`'s own `Eq` and `Ord`.
            ///
            /// *Mutations (killed):* recorded on `testing.md`'s A17 line.
            #[test]
            fn a17_interned_equality_is_content_equality((first, second) in arb_text_pair()) {
                let (x, y) = (Sym::intern(&first), Sym::intern(&second));
                prop_assert_eq!(x.as_str(), first.as_str());
                prop_assert_eq!(y.as_str(), second.as_str());
                prop_assert_eq!(x == y, first == second);
                if x == y {
                    prop_assert_eq!(hash_of(&x), hash_of(&y));
                }
                prop_assert_eq!(x.cmp(&y), first.cmp(&second));
                prop_assert_eq!(Value::symbol(&first) == Value::symbol(&second), first == second);
                prop_assert_eq!(
                    Value::string(&first).cmp(&Value::string(&second)),
                    first.cmp(&second)
                );
                prop_assert_ne!(Value::symbol(&first), Value::string(&first));
            }
        }

        /// **A17's non-vacuity guard**, read against its sentence: pairs of equal
        /// texts in separate allocations, pairs of the same length differing in a
        /// character, both orders, empty texts and multi-byte texts are all drawn.
        /// Floors at about two thirds of what was measured, of 400: equal 137, near 108,
        /// less 120, greater 92, empty 115, multi-byte 284.
        #[test]
        fn a17_generator_reaches_equal_near_and_ordered_texts() {
            use proptest::strategy::ValueTree;
            use proptest::test_runner::TestRunner;

            let mut runner = TestRunner::deterministic();
            let strategy = arb_text_pair();
            let (mut equal, mut near, mut less, mut greater, mut empty, mut multibyte) =
                (0, 0, 0, 0, 0, 0);
            for _ in 0..400 {
                let (first, second) = strategy
                    .new_tree(&mut runner)
                    .expect("strategy produces a value")
                    .current();
                match first.cmp(&second) {
                    std::cmp::Ordering::Equal if !first.is_empty() => equal += 1,
                    std::cmp::Ordering::Less => less += 1,
                    std::cmp::Ordering::Greater => greater += 1,
                    _ => {}
                }
                if first != second && first.chars().count() == second.chars().count() {
                    near += 1;
                }
                if first.is_empty() || second.is_empty() {
                    empty += 1;
                }
                if !first.is_ascii() || !second.is_ascii() {
                    multibyte += 1;
                }
            }
            eprintln!(
                "equal {equal}, near {near}, less {less}, greater {greater}, empty {empty}, multibyte {multibyte}"
            );
            for (name, count, floor) in [
                ("equal non-empty", equal, 91),
                ("same length, different", near, 72),
                ("less", less, 80),
                ("greater", greater, 61),
                ("with an empty text", empty, 77),
                ("with a multi-byte text", multibyte, 189),
            ] {
                assert!(count >= floor, "{name}: {count} pairs (floor {floor})");
            }
        }
    }

    #[test]
    fn a_value_is_a_tag_and_a_string_reference_wide() {
        // A `Sym` is a pointer and a length, where a `String` added a capacity.
        assert_eq!(std::mem::size_of::<Sym>(), 16);
        assert_eq!(std::mem::size_of::<Value>(), 24);
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
