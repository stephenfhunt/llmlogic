# The asking form, and why provisioning is the sigil's real job

Long-form backing for the §17 decision of **2026-08-21** (the provenance asking
form and demand-proportional provisioning). §17 holds the four rulings; this file
holds the flows they were derived from, the alternatives that were rejected, and
the one finding that reordered the whole question.

**This is the three-way session** `ROADMAP.md` and
[`taking-stock-2026-08-18.md`](taking-stock-2026-08-18.md) finding 4 said had to
be held as one: the asking form, "does the derivation store earn its cost", and
row-level import provenance. The third was ruled 2026-08-21 and stays a trade; the
first two are here, and they turned out to be **one decision**, not two that
constrain each other.

## The finding that reordered it

The 2026-08-16 surface decision, adopted from tsdl, says *the sigil is a cost hint,
not a selector*. Re-derived here, that is nearly an argument for having **one**
form: after the fixpoint the engine already knows whether the fact holds, so it
needs no hint to dispatch, and a hint legible only to a human is decoration.

It is decoration at exactly one moment less than it looks. **Before** the fixpoint
the sigil is the only thing that says whether the run needs a derivation store:
`?why` needs one, `?whynot` needs the model and a re-solve and nothing else. So the
sigil buys the provisioning decision — the first argument for two forms this
project generated rather than adopted, and the reason the two ROADMAP items are one
item.

## Why a query cannot stand in for the forms

The question put was whether issuing a query is already a request for its
derivation, and whether why-not could be had that way.

**For `why`, the objection is only volume, and it is a real one.** §11 rules *nothing
elides and nothing is shared*, so an answer set of forty recursive facts is forty
unshared trees. The closure property survives — proofs ride in `%` comments — so
this is a cost objection, not a correctness one. A narrow version (explain
implicitly when the answer is one fact or `holds(true)`) was considered and
rejected: it keys the output shape on the answer's *cardinality*, which is §14's
projection hazard reached from a third side, and it silently re-prices every
program already written.

**For `whynot`, the objection is structural.** The dominant shape of the question is
a query that **succeeded**: *twelve rows came back and I expected `alice` among
them*. The query did not fail, so there is nothing to trigger on, and the
expectation — the fact the user thought would be there — exists nowhere in the
program. Only a form that **names the missing fact** can carry it. The empty-result
sub-case is the easy one, and even there a non-ground goal degrades the near-miss
to *"rule 2 got as far as literal 3, for some binding"*: a fact about the rule, not
an answer about the row.

**And §16.13 forecloses the implicit version outright.** `datalog roster.dl -q 'not
double_booked(_, _)' && deploy` makes silence-plus-exit-1 the *designed* answer for
no. Implicit explanation would put a failure trace on the fact stream and pay a
near-miss re-solve on the caller contract's hot path. Making it opt-in by flag
instead is strictly worse than a goal, because a flag cannot name which fact.

## The flows

Written against `EXPERIMENTS.md` task 6 (the crate analysed as itself, 22k facts),
because that is the only place this engine's failure modes have been observed on a
real workload rather than a fixture.

**A — the answer is wrong.** The measured gap, 2026-07-27: *"On well-formed
programs asking the wrong question, no [self-correction] … `dead(F)` returning six
plainly-used functions read exactly like `dead(F)` returning nothing."*

```sh
./datalog crate.dl -q 'dead(F)'                    # six rows, one obviously wrong
./datalog crate.dl -q '?why dead("Model::new")'
% why dead("Model::new")
% 0  dead("Model::new")  by dead(F) :- defined(F), not called(F)
% 1    defined("Model::new")  [fact from "defs.csv"]
% 1    no called("Model::new")
```

The last line is the finding, and it points at the **fact base**. That matters
because all four wrong conclusions in that session came from the fact base and none
from evaluation, and because the leaves carry `[fact from …]`: the proof lands the
reader on "audit your extractor", which was the engine's real job there and took
three fix-and-re-extract rounds, each triggered only by eyeballing an answer.

**B — the answer is missing.** The same session's *"one join across two id-spaces
that derived nothing at all"* — a correct empty answer and a broken one are the same
output.

```sh
./datalog crate.dl -q 'calls("a","b")'             # nothing, exit 1
./datalog crate.dl -q '?whynot calls("a","b")'
% whynot calls("a","b")
% rule 3: calls(A, B) :- defined(A, Id), callsite(Id, N), defined(B, N)
%   satisfied: defined("a", 42), callsite(42, "b_impl")
%   blocked at: defined(B, "b_impl")  — no fact matches
%   repair: add defined(_, "b_impl")
```

**C — the loop.** Once the fact is known, both goals ride in one invocation:
`-q 'dead(F)' -q '?why dead("Model::new")'`, re-run after each extractor fix.

## Will an agent distinguish them

**The selector is observable, not inferential**: *did the fact come back or not*,
and the agent is holding that output when it chooses. Set beside the choices the
skill already asks for — name the query or not, `count` versus `count { … is not
absent }` — this is the easy one.

**And a wrong pick cannot lose the question**, because both forms return the same
union: `?why` over a fact that does not hold answers with the trace. That property
is worth more here than the sigil's precision, given the same experiment's finding
that *"a skill that cannot say something loses the question silently, and the model
does not announce the switch"*.

Two risks outrank sigil confusion:

- **The ground-goal rule.** `?why dead(F)` is the likely error, and it is a hard
  one. The diagnostic has to teach the division rather than refuse — *a goal names
  one fact; run `?- dead(F)` to see which hold, then ask about one* — which is the
  usage model in a sentence: **`?-` enumerates, `?why`/`?whynot` interrogate one
  row.**
- **Never asking at all.** By the symmetry of the finding above, a model that does
  not announce when a skill cannot say something will not announce when it could
  have asked. That is a doc-placement problem (the forms belong in the debugging
  loop, beside the "reading `person` facts back does not mean you have all of them"
  warning) and it is what S1's rebuilt harness should measure.

## Two runs, and what that buys

The flow is inherently **two runs**: nothing knows it wants an explanation until it
has read an answer. So run 1 is a plain query needing no provenance and run 2 is
the explanation — **the agent's own workflow is the demand signal**, which is what
makes gating nearly free rather than a trade.

It also settles provisioning against the union. `notes/semiring-provenance.md` §1
rates `?whynot` *"plausibly more valuable than `?why`"* for the LLM debugging loop,
and a `?whynot` run needs no recorder; gating on the union of both sigils would
therefore burn 70–78% of peak RSS on the commonest explanation run there is, for
nothing. So provisioning is **per-sigil**, and the cross case — `?whynot` over a
fact that turns out to hold — re-runs the **fixpoint only**, reusing the lowered
`ir::Program` so there is no re-parse and no re-import, and says so in one comment
line. The union contract is preserved exactly: any form may still return any arm.

The re-run is affordable for the reason the session named: an engine run is in the
ballpark of an LLM call or faster, and an agent runs on its own timeline, so a
second fixpoint on the case where the user guessed wrong is a wash. That reasoning
has a scale limit worth stating — §13's pitch is 170k-row imports, and `sparse_800`
is 46 s — but the limit falls on *re-running at all*, which is
[`taking-stock-2026-08-18.md`](taking-stock-2026-08-18.md) finding 2 (nothing is
reusable across runs), not on this decision.

## What it does to "does the derivation store earn its cost?"

It answers the question by **proportioning** it rather than by replacing the store.
Today the recorder costs 70–78% of peak RSS and a projected 50–60% of the post-seek
run ([`profile-2026-08-20.md`](profile-2026-08-20.md)) on **every** run and earns it
on **none**, since nothing can ask. Gated, it costs only on the run that asks for a
proof, and "does it earn its cost" becomes a much easier yes.

Backwards extraction — tsdl's answer, no recorder in the fixpoint, one proof
extracted from the retained model — therefore demotes from a v1 architecture
decision to a **post-v1 optimisation of the explaining path alone**. It is also the
riskier of the two: it gives up the all-derivations contract, needs its own cycle
guard in place of the round stamp, and could change *which* proof prints, which
§16.6 and E7/E8 pin byte for byte. Gating captures the whole win on the dominant
run for a flag and a branch, and leaves almost no lock-in — if extraction ever
lands, the flag becomes vestigial rather than wrong.

Two consequences worth recording:

- **Cut A and cut B never touched `first_round`** — a second `HashMap<Fact, u32>`
  keyed by a cloned fact, and provenance-only. Gating it as well may beat the
  measured −78%.
- **The profile's scratch build stops being necessary.** Once the flag exists the
  A/B is two ordinary invocations, so the post-seek number the ROADMAP has been
  waiting for becomes a measurement anyone can repeat instead of a throwaway
  worktree.

## Rejected

- **Implicit explanation on every query.** Volume for `why`; structurally unable to
  carry `whynot` (the succeeded-query case); and incompatible with §16.13. Above.
- **Implicit explanation for single-fact answers.** Output shape keyed on answer
  cardinality; silently re-prices existing programs.
- **One sigil, dispatching on whether the fact holds.** Genuinely tempting while
  the sigil is only a cost hint — and defeated by provisioning, which needs the
  distinction *before* the fixpoint, where holds-ness is not yet known.
- **Union provisioning** (any explanation goal records). Simplest to explain, and
  it overpays on the commonest explanation run. Above.
- **Per-sigil with no re-run** — `?whynot` over a holding fact answering *"it
  holds; ask `?why`"*. Cheapest and fully predictable, and it breaks the 2026-08-16
  ruling: one form could then only answer one way, which is the thing that ruling
  exists to prevent.
- **A dedicated CLI flag** (`--why`). `program_with_queries` already classifies its
  `-q` argument by parsing it, so a sigil inside `-q` is one more arm and no new
  flag — and it composes with multiple `-q` in order, which flow C needs.
- **An explanation counting as an answer row** for the exit code. It would make
  `?why` change the branch of any check it is appended to. Explanations are
  exit-code-**neutral** instead, which is the exit-code twin of E5's
  comment-stripping guard: adding an explanation to a run changes neither its fact
  stream nor its code.

## Out of scope, deliberately

The proof/trace **JSON encoding** (§14 parks JSON as a machine-readable *edge*
feature); **semiring provenance** and tropical cheapest-proof selection (parked
research, and the place a token-economy answer to deep proofs would come from);
**row-level import anchors** (ruled 2026-08-21, a memory trade).

**Magic sets** deserve their own line, because "skip the work nobody asked for" is
the general form of this decision and demand-driven evaluation is its full version.
It conflicts with §11: a proof is rendered *against* a program, and a magic-set
rewrite's derivations cite rules the user never wrote. Anyone taking it up owns
that conflict.
