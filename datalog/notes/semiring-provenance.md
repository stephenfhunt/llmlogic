# Semiring provenance under negation — parked research thread

*Recorded 2026-07-20, after the §7 stratified-negation sessions. Status:
**parked** — deliberately not on the roadmap (`ROADMAP.md`). This note exists so
the thread can be rebooted later without re-deriving it. It supplements — and
does not reopen — the §17 decision of 2026-07-20 that negation provenance is a
proof-tree-level why-not record rather than a semiring construction.*

## Background, compressed

**Positive programs.** Green–Karvounarakis–Tannen (PODS 2007, references.md
group 5) annotate base facts with elements of a commutative semiring and
propagate them through evaluation: joint use in one rule body multiplies,
alternative derivations add. The provenance of a derived fact is a polynomial
over the base-fact indeterminates; specializing the semiring turns the *same*
computation into counting, trust, access control, boolean lineage, or minimal
witness sets. Recursion needs infinite sums, hence ω-continuous semirings /
formal power series.

**Why it stops at negation.** Set difference is subtraction and semirings have
no additive inverses. Extending K-relations with a "monus" (m-semirings,
Geerts–Poggi) runs into genuine trouble: Amsterdamer–Deutch–Tannen showed the
candidate difference semantics conflict with identities one would want to
keep. There is no clean subtraction story.

**The dual-indeterminate move.** Grädel–Tannen (arXiv:1712.01980, 2017) avoid
subtraction entirely: put the query in negation normal form and annotate
*negated* atoms with their own dual indeterminates (`X̄` alongside `X`).
Provenance becomes a polynomial over both alphabets, quotiented by
`X·X̄ = 0` — no proof may use a fact and its absence simultaneously. Negation
never needs to be "subtracted"; an absence is just another token a proof can
consume. For fixed-point logics, Dannert–Grädel–Naaf–Tannen (CSL 2021) make
this finite with *absorptive* polynomials (`a + a·b = a`): only ≤-maximal
monomials survive, which tames the infinite unfoldings recursion produces.

(Verify all citations against the papers before relying on them —
references.md convention.)

## What this engine already has, without calling it that

The §17 "not adopted" framing undersells how much of the structure is already
present. Three observations, each anchored in code:

1. **The derivation store is a boolean provenance circuit in DAG form.**
   `Model.derivations` (`src/engine/mod.rs`) records *all* derivations per
   fact, deduplicated by rule instance. Alternatives per fact are the
   polynomial's `+`; a derivation's premises are a `×`; facts are shared
   nodes, so common subproofs are represented once. This is exactly the
   circuit representation of Deutch–Milo–Roy–Tannen (ICDT 2014, group 5) —
   compact where trees are exponential — instantiated at the boolean
   semiring.

2. **`Premise::Absent` is a *factored* dual token.** A satisfied negation is
   recorded as an `AbsentPattern` (`src/provenance.rs`): the negated atom
   under the rule's bindings, wildcard slots left open. Under the closed-world
   assumption over the active domain, `Absent(parent(_, "alice"))` denotes the
   product `∏_b p̄arent(b, "alice")` over every domain value `b` — the ∀
   hiding under a negated ∃. Where Grädel–Tannen expand that product over the
   (finite) structure, this engine keeps it symbolic as a pattern with open
   slots. The compressed form is arguably the more useful artifact — it is
   what renders as "because no `parent(_, "alice")` fact exists" — and the
   correspondence *compressed pattern ⇔ expanded dual product* appears to be
   a small original observation.

3. **Stratification is an iterated quotient; first-round stamping is a
   monomial selector.** Each stratum freezes which dual tokens hold before
   any reader consumes them — evaluating the quotient by `X·X̄ = 0` level by
   level. And `ProofTree::explain`'s first-round rule (fact premises must be
   strictly earlier; absences always qualify) is precisely a well-founded
   choice of one monomial from the polynomial.

Consequence: a semiring layer here would be an **interpretation folded over
the existing derivation DAG** — not a redesign, and not in tension with the
ratified proof-tree model, which is simply its boolean instance.

## Opportunities, ranked by fit to the project thesis

The project's pillars (`datalog/AGENTS.md`): LLM-agent usability, token economy,
explainability. Ranked accordingly:

### 1. `?whynot` with minimal repairs — practical, near-term

For a fact the model does *not* contain: enumerate the rules whose head
unifies with it; for each, run the body and report which literal failed and
what blocked it — the refuting fact for a failed negation (found by
`AbsentPattern::matches`-style scan), the missing fact for a failed positive —
then extract minimal repair sets ("delete `parent("alice","bob")`", "add
`person("dave")`"). This is instance-based why-not in the sense of
Chapman–Jagadish ("Why Not?", SIGMOD 2009), and the PUG framework
(Lee–Köhler–Ludäscher–Glavic) plus Ludäscher et al.'s game-theoretic
provenance show why/why-not can be treated uniformly — a natural fit, since
our `Premise` enum already made absence a first-class premise. For the LLM
debugging loop ("why isn't my query returning X?") this is plausibly *more*
valuable than `?why`. Buildable today on the existing join machinery; natural
landing spot is the §11/§14 provenance-surface work (roadmap step 6).

### 2. A `Semiring` trait folded over the derivation DAG — the on-brand novelty

One fold, many interpretations:

- **boolean** — what `explain` does now;
- **counting** — number of proofs (needs care under recursion: use
  first-round-founded unfolding, which is finite, as the practical cut;
  absorptive truncation as the principled one);
- **tropical (min-cost)** — cheapest proof. The application worth writing up:
  **token-economy proof selection**. When an agent asks `?why`, weight base
  facts and absences by rendered token cost and return the *cheapest*
  explanation for the context window. Framing context-window budget as a
  tropical semiring valuation appears to be a fresh application of standard
  theory, and it degrades gracefully: positive programs need nothing new;
  negation costs are dual-token valuations (what does it cost to *state* an
  absence?).
- **why-provenance** — minimal support sets, for "which base facts does this
  conclusion actually depend on".

### 3. The theory write-up — if ever worth a weekend

"Compressed dual-indeterminate provenance in a stratified Datalog engine":
the factored ∀-product observation, strata as iterated quotients, and the
first-round rule as monomial selection. Workshop-sized (TaPP — Theory and
Practice of Provenance — is the venue this literature lives at). Only worth
doing after (1) or (2) exists as evidence.

## Non-goals

- **Materializing absorptive polynomials for generality's sake.** Heavy, and
  the agent use case doesn't need arbitrary-semiring generality up front.
- **Replacing the proof-tree model.** The §17 decision stands. Everything
  above is an additional interpretation over `Model.derivations`; if a
  semiring layer ever lands, `ProofTree::explain` should fall out as its
  boolean/tropical instance, not be rewritten.
- **Well-founded or stable-model semantics.** Out of scope per §7; the dual
  machinery is interesting precisely because it works over our stratified
  fragment.

## Reading list before rebooting this thread

Group 5 of references.md holds Green–Karvounarakis–Tannen, Cheney–Chiticariu–
Tan (vocabulary), Deutch–Milo–Roy–Tannen (circuits), Köhler–Ludäscher–
Smaragdakis (debugging UX), Grädel–Tannen 2017, and Dannert–Grädel–Naaf–Tannen
CSL 2021. To chase beyond it:

- Amsterdamer, Deutch, Tannen — on the limitations of provenance for queries
  with difference (c. 2011) — why monus fails.
- Geerts, Poggi — m-semirings (the monus attempt itself).
- Chapman, Jagadish — "Why Not?", SIGMOD 2009 — instance-based why-not.
- Lee, Köhler, Ludäscher, Glavic — PUG: unified why/why-not provenance graphs.
- Ludäscher et al. — game-theoretic provenance (why/why-not as win/move
  positions; connects to well-founded semantics).
- Ramusat, Maniu, Senellart — practical algorithms for semiring provenance
  over Datalog/graphs (which semirings admit efficient computation).

All citations from memory — verify each against the actual paper before
building on it.
