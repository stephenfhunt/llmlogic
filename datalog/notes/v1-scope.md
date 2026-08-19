# The v1 line — ruling every open backlog item against §1's success criteria

*Recorded 2026-08-18, in the session that wrote §1 and ratified §2. §1 defines v1
as **S2–S6 hold and S1 has been measured at least once**; this note applies that
definition to `ROADMAP.md`'s open set, item by item. The tag on each ROADMAP item
is the ruling; the argument is here.*

**The test used throughout:** an item is **v1** when a stated success criterion is
not met without it, or when a criterion cannot be *checked* without it. Everything
else is **post-v1** — which means *not blocking v1*, not *unwanted*. Where an item
could be argued either way, the tie-break is §1's target-user ordering: the agent
driving the CLI wins over the embedding Rust program, which wins over the human
reader.

Two shapes recur and are worth naming, because they are why the v1 count is larger
than the session count:

- **Merged items.** Several rulings are v1 only as part of a session that was
  already decided to be joint — the caller's contract, and the provenance surface.
  Splitting them would produce two vocabularies for one mechanism, which is the
  argument §17 2026-08-18 already made.
- **Decision, not implementation.** A few items are v1 *as a decision*: the session
  must rule on them because a sibling item's design is constrained by the answer.
  Row-level import provenance is the clear case — the ruling may well be "no".

## v1

| item | section | why a criterion needs it |
|---|---|---|
| **`EXPERIMENTS.md` as an instrument** | skill | **S1** outright. This is the only item S1 names, and until it exists v1 is undefined rather than unfinished. |
| **Provenance query surface (`?why`/`?whynot`)** | §11 | **S5**: "explained *through the surface the caller used*". The engine records everything and the CLI exposes none of it. |
| **Does the derivation store earn its cost** | §11/engine | Decided in the same session as the row above, per the stock-take: a profile pressures the recorder toward optional, and pillar 1 is the only argument against. Whichever settles first constrains the other. |
| **Imported facts anchored by relation, not row** | §11/§13 | Third input to that same decision. **v1 as a ruling**, not necessarily as a feature — retaining rows costs memory on the 1.2 GB shapes, so "no, and §11 says so plainly" is a legitimate outcome. |
| **E3 replay does not cover §8 builtins (E6)** | §11/§15 | If S5 ships a proof surface, the strongest provenance property must have seen an `=`-assignment. It never has: its generator emits no comparisons. |
| **Profile the engine** | engine | Not required by **S6** directly (S6 is met), but it is the input the recorder decision above is sequenced behind. v1 by that dependency, not on its own merits. |
| **Aggregation does not scale with the aggregated relation** | §9/engine | **S6**: 2.5× the rows at fixed group count costs 9.6×, and it is the one shape a sibling engine wins outright. The suspect is a rescan, so the fix is expected to be small once profiled. |
| **Integrity constraints + an exit code that carries an answer** | §12/§14/§15 | **S1** and pillar 3: the measured question class includes constraint/consistency checking, and today the caller must parse stdout to learn the answer. `datalog check.dl && deploy` cannot mean what it looks like. |
| **The truncation contract's open half** | §9/§13/§15 | Merged with the row above by §17 2026-08-16/2026-08-18 — one exit code, one stdout discipline, one vocabulary. |
| **Make the malformed/missing reclassification visible** | §8/§9/§11 | Rides the same session: `"abc" as int` is `absent`, so a dirty column reads as a sparse one and nothing says otherwise. It is the truncation question asked of `=`. |
| **Machine-readable error taxonomy** (codes + semantic spans) | §12 | **S3**, and the reason §2's structured-errors principle ratified *scoped*: all 46 `Error::semantic` and 33 `Error::source` sites carry no span, and there is no code to branch on. |
| **A type-clash diagnostic that names the conversion** | §12 | **S3**. `as` now exists, so there is a concrete fix to suggest — which there was not when this was first noted. |
| **`declare` does not count as defining a predicate** | §10/§12 | **S3**, inverted: a warning that fires on a *correct* program, with no way to silence it. The sibling engine measured this exact hazard leading a model to a destructive fix. |
| **Temporal types** | §4/§8/§13 | **S4**, which is why S4's status reads "met **except dates**". A date column out of §13 lands as a string or an int with no arithmetic over it, so the preprocessing step S4 denies is exactly what a date question needs. |
| **Count-distinct, and the invisible wildcard** | §9/§13 | **S1**. Not ergonomics: measured on a 7-column table it returns 36 where the question wanted 20, silently. A trap that yields wrong answers is a direct threat to the accuracy S1 measures. |
| **Name a test per §16 example** | §16 | v1 claims S2–S6 hold, and §16 is where those claims are demonstrated. Eight of eleven examples carry prose in comments and no named test — a block nobody wired up cannot fail. |
| **Normalize §17's chronology** | §17 | Trivial, and §17 is cited by every ruling in this note; a reader currently cannot tell which end is current. |

## post-v1

Grouped by the reason, since the reasons repeat.

**Awaiting a consumer** — each already carries "deferred until something needs it",
and nothing in S1–S6 does: **database loading**, **TSV**, **filter pushdown**,
**module namespacing** (all §13), **`serde` for the API**, **`--format json`**
(parked), and the **big external fact base demo**.

**Additive to a surface that already works** — no criterion is unmet without them:
**statistical reducers** (§9), **collection-valued reducers** (blocked on a
first-class collection value anyway), and **parallelism** (scope follows the
profile, and S6 is about exponents, which parallelism does not change).

**Research** — **recursive/monotonic aggregation** and **semiring provenance under
negation**. Both are parked with that label already.

**A milestone the v1 answer already covers** — **limit predicates**. The
2026-08-18 warning *is* v1's answer to value-creating recursion: the program runs,
and is named before it does. Limit predicates would make the accumulating shape
finite rather than merely warned about, and they move §6's `T_P`, §9 and §11 —
which is a v2 theme if anything is.

**Ergonomic, with the workaround in-language** — **an unnameable query as an
error** (naming stays optional; recovery is "prepend a word"), **a synthesized
answer that does not say which query it answers** (narrowed 2026-08-17 to the
*unnamed* case, so naming both queries already resolves it), and **three body
grammars** (the asymmetry is now *stated* in §5's footer; whether `;` extends is
the open part, and no criterion turns on it).

**Deliberately gated on S1** — **other agent-exposure forms** (MCP, an API
harness). §17 already says the skill experiment is what should decide these, and
S1 is now that experiment. A hosted surface is also the one place the no-budget
and warn-don't-reject calls do not cover, so it should not be built before the
measurement it is waiting on.

**Post-v1 with a named trigger** — **nothing is reusable across runs**. S1 measures
accuracy, not latency, so no criterion is unmet without it. The trigger to promote
it is S1's own harness: if the measurement shows the per-iteration reload changing
what an agent *does* — abandoning a refinement rather than paying for it — this
becomes a v1 item retroactively. That is a finding the current benchmark cannot
produce, since it times whole processes by construction.

**Deferred on its own terms** — **restructuring the §17 decisions log**. 53% of
`spec.md` and growing faster than the body, but no criterion turns on it and it is
flagged a user call needing its own session. Chronology normalisation above is the
cheap half, taken now.

## What this ruling does not do

It does not sequence the v1 items among themselves — that stays `ROADMAP.md`'s job,
and the stock-take's recommended order (the caller's contract, then temporal types,
then the profile) survives this note unchanged, with S1's harness now sitting
alongside them rather than near the bottom.

It also does not claim the criteria are complete. The check run while writing this
was: *if an item's ruling cannot be derived from a stated criterion, the criterion
is missing rather than the ruling being wrong.* That check fired once — S4 was
written "met" and had to be corrected to "met except dates", which is what makes
temporal types v1 rather than a preference.
