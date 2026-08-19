# Termination and value-creating recursion — the theorem and the rejected alternatives

*Recorded 2026-08-18, from the Termination design session. Status: **built** —
the classification ships as a warning in `lower::value_creating_recursion`, and
`spec.md` §10 is its normative statement. This note holds what would burst a §17
entry: the proof of the certified fragment, and the four designs that lost.*

## What the language guarantees

**A certified program terminates. An uncertified one is named before it runs.**
Certification is a syntactic property of the program alone, decided by lowering,
and every program still evaluates either way.

A rule is **uncertified** when a variable in its head is bound to an
arithmetic-computed value *and* its head predicate lies on a cycle of positive
dependency edges. A program is certified when none of its rules is.

The computed variables of a body are the least set closed under:

- an `=`-assignment whose source expression contains an arithmetic operator and
  mentions at least one variable — the seed, and the only construct in the
  language that can synthesise a value absent from the program and its inputs;
- an `=`-assignment whose source expression mentions an already-computed
  variable — so `K = M + 1, N = K` computes `N`;

with two constructs deliberately excluded. A **cast** propagates but never
creates: `X as int` maps a finite value set to a finite value set with no
accumulation (§8), so `N = M as int` over an uncomputed `M` is not value
creation, while `N = K as int` over a computed `K` still carries the growth. An
**aggregate** result is a function of a relation stratified strictly below (§9),
so its range is already finite.

## The theorem

> **Every certified program has a finite least model, reached in finitely many
> rounds.**

Condense the positive dependency graph into strongly-connected components.
Negated and aggregated edges already point strictly down — stratification (§7,
§9) rejects any program where they do not — so the condensation is a DAG. Induct
on its topological order, with the induction hypothesis that every lower SCC has
a finite extent over a finite active domain.

- **Trivial SCC** (a single predicate on no cycle). Its rules' bodies read only
  strictly-lower SCCs, finite by hypothesis, so each body has finitely many
  satisfying substitutions. Arithmetic is a total function on the values those
  substitutions supply, so the rule derives finitely many heads over finitely
  many values. Value creation is *permitted* here and costs nothing: it runs a
  bounded number of times.

- **Non-trivial SCC, or a self-loop.** By certification, no rule in it binds a
  head variable to a computed value. Every head variable is therefore bound by a
  positive atom, by a taint-free `=`-chain rooted in a constant or a positive
  atom, or by an aggregate whose range is finite by hypothesis. Positive atoms
  resolve either inside this SCC or strictly below it. So the SCC's active domain
  is contained in a **fixed finite set, fully determined before the SCC runs**:
  the constants of its rules, plus the active domain of the lower SCCs. Its
  Herbrand base is therefore finite, `T_P` is monotone over a finite lattice, and
  the least fixpoint is reached in finitely many steps. ∎

**Two consequences, stated in §6 since 2026-08-18.** The classical finite-universe
argument is available again, scoped to this fragment — which is what `bugs/004` found
missing. And with it the PTIME data-complexity result: for a fixed program the
number of SCC levels is fixed, and each level's active domain is bounded by a
polynomial in the input size whose degree depends only on the program's arities
and variable counts, so the whole model is polynomial in `|D|`. That is the
ordinary Datalog picture (data complexity PTIME, combined complexity higher), and
the certification is exactly what restores it.

**What the theorem does not claim.** Nothing about *speed*: a certified program
can be slow, and §17 2026-08-16 settled that a slow program stays slow. Nothing
about arithmetic errors: `i64` overflow is a structured error (§8), which is a
failure and not a divergence. And nothing about uncertified programs, which are
not thereby non-terminating — see below.

## Why the uncertified case warns rather than errors

The condition is sufficient for termination and **not necessary**, and the gap is
not an edge case. It is the shape people write:

```datalog
path_cost(X, Y, C) :- edge(X, Y, C).
path_cost(X, Z, C) :- path_cost(X, Y, C1), edge(Y, Z, C2), C = C1 + C2.
```

This terminates on every acyclic `edge` and diverges on every cyclic one. Whether
it is a good program is a property of the **data**, which no static rule can see.
Rejecting it would be a false positive on call graphs, build dependencies and
every other DAG — the source-analysis workloads the feature was motivated by.

Three further reasons, in the order they carried weight in the session:

1. **`bugs/004` never asked for rejection.** Its acceptance criteria read "the
   `nat` program above is rejected, **or** its non-termination is documented as
   in-scope behaviour. It must not be silently accepted while §6 claims it cannot
   happen." The defect is that §6 asserted something false.
2. **The denial-of-service argument was already discounted.** §17 2026-08-16
   ratified "a slow program stays slow, `^C` is the operator's" and parked the
   hosted surface. That was reasoning about a runtime budget, but it transfers to
   rejection without a change.
3. **There is no defensible line between `nat` and `path_cost`.** `nat` diverges
   with no data at all; `path_cost` only on cyclic data. The checkable version of
   that distinction — "the recursive rule reads an atom from outside its own
   SCC" — over-accepts `p(N) :- p(M), q(_), N = M+1.`, which diverges for any
   non-empty `q`. A heuristic line inside an *error* is worse than an honest
   warning, and the distinction is worth keeping only as what it is: a sentence
   in the message.

The warning therefore says which of the two it is. Bounded, it names the
relations read from outside the SCC and says termination holds while they have no
cycle reachable through this rule — a claim the user can check in this language:

```datalog
cyclic(X) :- reaches(X, X).
```

Unbounded, it says nothing consumes a step and the program cannot terminate on
any input.

**The timing is part of the design, not an implementation detail.** Answers print
after the fixpoint, so a warning carried out on the run's result reaches a
non-terminating program exactly never — `bugs/004`'s "no output at all" applied
to its own diagnostic. `api::run_at_reporting` exists for that reason.

## The four rejected alternatives

**Runtime fuel** — a budget over rounds, derived facts, wall clock. *Rejected
2026-07-25, reaffirmed 2026-08-16* against a sibling engine that ships one
(`notes/tsdl-cross-project-review.md`). A budget is not a guarantee, and the
forcing case for one is a hung browser tab; a CLI has `^C`. The half of that
engine's design we did take is the truncation contract, for the separate reason
that truncation costs soundness and not only completeness.

**A hard static rejection** — the direction this session inherited and reversed.
Its argument is above. What it would have bought is an *unconditional* theorem
rather than a scoped one; what it costs is `path_cost`, which is too much.

**A bounded-counter recognizer** — accept a computed head variable in a cycle
when the same body caps it: `N = M + c` with `c` a positive literal, together
with `N < k` or `N <= k` for a literal `k`. This covers the bounded-depth idiom
an LLM writes often, and it is genuinely sound for that shape. It was rejected
for two reasons. It cannot reach `path_cost`, whose increment comes from a
relation whose sign is not statically knowable — so it does not close the case
that motivated the escape hatch at all. And it makes the guarantee's statement
conditional on a second, subtler argument, at a moment when the first one is
being written for the first time. If the warning turns out to be noisy on counter
programs in practice, this is where to look first.

**Limit Datalog** — the principled version of the escape hatch, and the successor
this session recommends. Kaminski, Cuenca Grau, Kostylev, Motik and
Horrocks, *Foundations of Declarative Data Analysis Using Limit Datalog Programs*
(IJCAI 2017; references.md group 1) make numeric Datalog decidable by restricting
numeric predicates to **limit** predicates, which keep only the least (or
greatest) value per group of key arguments. `path_cost` under a `min` limit is
their canonical example: the relation stays finite because only the cheapest cost
per `(X, Z)` is retained, and the program computes shortest paths rather than
enumerating every path's cost. §4's `declare` is the obvious surface —

```datalog
declare path_cost(from, to, min cost).
```

— since it already exists, already carries a per-column vocabulary, and is
already the place a program says something extra about a relation. This is a
semantics change to the fixpoint and to §6 and §9, so it is a milestone rather
than a rider; it is recorded as a ROADMAP item and as a §17 open question.

## What this rests on, and what would falsify it

The theorem's load-bearing invariant is that **taint is transitive**: a check
reading only the literal that binds a head variable certifies
`nat(N) :- nat(M), K = M + 1, N = K.`, which diverges. The rule sketch this
session started from said exactly that, and it was wrong. Two properties fail if
the invariant does — `c10_a_certified_program_reaches_its_fixpoint`, on a round
cap, and `c8_the_taint_spellings_classify_alike` — and both were mutation-verified
against dropping the `=`-chain propagation and against a check that does not look
under a `Cast` node.

The second invariant is that **an aggregate result is not a computed value**. It
rests on aggregates being stratified strictly below their reader (§9), so if
recursive or monotonic aggregation ever lands, this classification has to be
revisited before it does.
