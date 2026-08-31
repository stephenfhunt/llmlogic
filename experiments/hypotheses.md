# Hypotheses — written before the grid runs

**This document is a pre-registration, and its discipline is append-only.** It
says what will be measured, which single number decides it, and what would make
the run unreadable — *before* any of it is known. Amend it only by dated addendum
below; never revise a claim after seeing the data it is about. A pre-registration
edited in the light of its own results is a report of a decision already made.

The reason is in `decisions.md` 2026-08-25: the comparisons went from one to three
in a single session. Three comparisons × two strengths × two tracks is twelve
readings of one run, and without naming the endpoint in advance that is twelve
chances to find something.

## The claims

**S1** (`../datalog/spec.md` §1) — *an agent answering multi-hop, recursive or
constraint questions is measurably more accurate **with** the engine than
reasoning in prose, at two model strengths, first program recorded before any
feedback.*

Two further claims are in scope only once a subject weaker than haiku exists
(`notes/discriminating-instrument.md`):

- **W1** — the engine helps *more* at the weak end than at the strong end.
- **W2** — a small model with the engine matches a frontier model without it.

## Which subject, and what may be compared with what

A run measures **one subject**. The Agent SDK subject and the local subject are
not comparable to each other: different models, different drivers, and two
deliberate asymmetries recorded in `decisions.md` 2026-08-26 — the completion
reminder, and `thought` on structured actions.

What is comparable, and what every endpoint below is built from, is **arm against
arm within one subject and one strength**. That comparison holds because the arms
differ by the engine and nothing else.

W2 is the exception and is therefore **not** decided by a single run: it compares
two subjects, so it needs both runs and is reported as a comparison of two
separately-valid measurements, with the asymmetries restated at the point of use.

## The primary endpoint

> **The paired accuracy delta, `engine-forced` − `prose`, on the `in-context`
> measured slate, at the weaker strength, by McNemar's exact test.**

One number. Named in advance. Everything else in the report is secondary.

Three choices inside it, each already argued elsewhere:

- **`engine-forced`, not `engine`.** `engine` measures adoption and capability at
  once and cannot separate them; the mandate arm measures capability alone
  (`decisions.md` 2026-08-25). S1 asks whether the engine makes the agent right.
- **The weaker strength, not both pooled.** The strongest model routes around
  gaps instead of falling into them, and opus was 20/20 in prose on the live
  domains in the 2026-08-24 grid — a ceiling leaves no room for a positive delta.
  Pooling the two was removed from `report.py` once already, as a defect: it
  averages the two subjects whose difference is the reason both are in the grid.
- **`in-context`, not `at-scale`.** A win in the first is a claim about
  *reasoning*; a win in the second is a claim about *scale*. They are reported in
  separate tables and never averaged (`decisions.md` 2026-08-25).

**Direction and size.** The claim is one-directional — the engine helps — but the
test is two-sided, because a significant delta *against* the engine is a finding
and must not be discarded as a null. The effect the slate is powered for is
**+10 points**; `harness power --effect 0.10` is what sizes it.

## Secondary endpoints

Reported with intervals, and **not** used to claim S1:

| endpoint | question |
|---|---|
| `engine` − `prose`, weaker strength | does *supplying* the engine help? — adoption and capability together |
| `engine` − `engine-forced` | what does not reaching for it cost? |
| all three at the stronger strength | does the effect survive a model that can route around gaps? (**W1** is the difference between the strengths) |
| mean per-item F1, every arm | separates dropping one row of forty from returning nothing — both grade `wrong` |
| the `at-scale` table | a claim about scale, never pooled with the above |
| reach: `answered-from` on the `engine` arm | the adoption number, which is a result in its own right |

## Preconditions — when the run is unreadable rather than null

Checked **before** the primary endpoint is read. If any fails, the run is
reported as uninterpretable and the endpoint is not cited.

1. **The negative controls hold.** The `prose` arm must reach **≥ 75%** on
   `controls`, and the arms must not differ there beyond their interval. These
   are single-hop lookups and one-step arithmetic; a subject that cannot pass them
   is below the instrument's floor, and a null on the measured slate then says
   nothing about the engine. *This is the precondition the first local sweeps
   failed.*
2. **The mandate took.** On `engine-forced`, `answered-from` must be **≥ 80%**. A
   mandate arm that did not run the engine is not measuring the engine, and the
   comparison it feeds is void.
3. **Enough paired items.** At least as many as `harness power` requires for the
   effect being claimed. **A run below power is not run.**

   Two facts that are easy to get wrong and expensive to get wrong late:

   - **`--repeats` does not buy paired items.** Trials of one cell collapse to a
     single observation by thresholded mean, because McNemar wants one per pair
     (`report._paired_units`). Repeats buy *reliability* per item — they separate
     a lucky answer from a reliable one — and nothing else. Only more **tasks**
     move `n`.
   - **The arithmetic.** A 10-point effect at 80% power wants 155 paired items,
     so **78 tasks** across two strengths. The pinned slate is 28 tasks — 56
     paired items, short by 99. Generation yields 29 tasks per
     `(seed, difficulty)` combination, so a calibration pool of roughly six
     combinations, selecting near half, is what clears it.
4. **The slate is calibrated for *this* subject.** An item's difficulty is not a
   property of the item alone: the band `harness calibrate` selects is the band
   *the calibrating subject* scored in. A slate calibrated on haiku is not
   calibrated for an 8B local model, and running one against the other reproduces
   the ceiling that calibration exists to prevent — at the opposite end. Each
   subject needs its own calibration pass, and the manifest records which subject
   selected it.
5. **The instrument is the one we think.** `arms.require_engine` accepts the
   binary, `resume.moved` reports no drift, and for a local subject `preflight`
   passes — server, models, and a served context no smaller than assumed.

## How the result will be read

- **Positive** — the primary delta is positive, its interval excludes zero, and
  every precondition held. S1 is supported *at that strength, on that slate, for
  that subject*, and the wording of any claim says all three.
- **Null** — the interval includes zero **and** is tight enough to exclude the
  +10 points the run was powered for. That is evidence of no useful effect, and it
  is a real finding.
- **Uninterpretable** — the interval includes both zero and +10, or a precondition
  failed. This is what the 2026-08-24 grid actually was, and it was nearly cited
  as a null. *A delta with no interval is not a null* (`decisions.md` 2026-08-25).

## What would falsify the project's premise

A tight null at the **weak** end, with controls held and the mandate complied
with, on a slate calibrated to be neither trivial nor impossible. That is the
condition under which `llmlogic`'s founding hypothesis is wrong, and it is worth
naming here so that it can be recognised rather than explained away.

## Addenda

*(Dated entries only. Amend, never revise.)*

### 2026-08-30 — A Haiku difficulty ladder, pre-registered as a locating run

An 88-cell run of `prose` against `engine-briefed`, at **haiku-4.5 only**, over a
slate that is not calibrated and does not pretend to be: 8 questions × 5
generator rungs at seed 20260830 (`slates/ladder-20260830.json`), plus the pinned
`controls`. **It is exploratory, it is not S1, and no accuracy claim comes out of
it.** Written before it runs, because it will produce a `engine-briefed − prose`
delta per rung and those numbers will sit in the report whatever they say.

**Why a ladder and not a calibrated slate.** Calibration keeps the items a
subject scores in the *middle* on, and which items those are is itself a fact
about the rung — so a screened ladder measures the screen. The whole question
here is *where along difficulty does the engine start to pay*, and the band is
what erases that axis. The manifest says `selection: none` for this reason.

**Why haiku, and why now.** Every measurement since 2026-08-26 is of a subject
below the instrument's floor. Recomputed from `run-20260824T104501Z`, haiku scored
**prose 23/40 (57%) / engine 22/41 (54%)** — off the floor *and* off the ceiling —
on a slate that is difficulty-flat. The standing blocker, *a rung between
`controls` and the measured slate*, is a statement about a subject, and this run
asks whether the axis exists for a subject that is on it.

**Why the bound arm.** `engine-briefed − prose` is already recorded above as an
**upper bound** on what the engine buys a subject, not S1. For a locating run
that is the right instrument in both directions: a ceiling that never turns
positive across five rungs says no documentation or generator work rescues the
engine for this subject, and one that does turn names the rung a powered
`engine-forced` grid should sit on. **The primary endpoint does not move onto
`engine-briefed`** — restated here because a new run that may look better is
exactly when it would.

**What may be claimed from it**, none of it an accuracy result:

| what | why 88 cells buys it |
|---|---|
| the **ordering** of the per-rung delta | the one thing a ladder can say; a direction to aim a powered pass, not a result |
| median wall clock and USD **per rung** | what sizes the next grid. The 7.5h → 42h error was a rate measured on `controls` extrapolated to a multi-hop pool |
| `answered-from` on `engine-briefed` | the arm has never met the SDK subject; whether haiku fabricates engine syntax the way the 14B did is untested |
| cap-hit and truncation per arm | whether the instrument is readable at these rungs at all |

**Precondition 3 is knowingly suspended.** 40 paired items against the 155 a
+10-point effect needs; every per-rung interval will span zero and then some.
That is *uninterpretable* by the reading above, entered into deliberately, and
is why nothing rests on the delta. **1, 2 and 5 still bind**, and are read on a
`controls` gate before the ladder runs.

**Two confounds, named in advance rather than after.** Generator difficulty moves
fact-base **size and structure together** (57 → 876 facts, closure depth 2 → 6),
so a turn in the curve does not say which caused it — separating them is the
`at-scale` track and is not this run. And `prose` and `engine-briefed` do not
share a turn or wall-clock budget (`cell.ARM_BUDGET`), so the cap-hit rate has to
be low at every rung or the rung is measuring the cap.

### 2026-08-28 (later ii) — The local subject is deterministic, so `--repeats` buys nothing

`LocalSubject` runs at **temperature 0.0**. Measured rather than inferred: the
clean 32-cell run reproduced sitting 1 cell for cell — `who-can-read-r03` at
445s/5 turns/1 truncation in both, `engine-briefed` on the same task capped at
1,800s/35 turns in both, `resources-for-u04` at 137s and 136s.

**This sharpens precondition 3.** It already said `--repeats` does not buy paired
items, because trials of one cell collapse to a single observation. The stated
consolation was that repeats buy *reliability* — they separate a lucky answer
from a reliable one. **Against this subject they do not buy that either**, and
the sentence should be read as: repeats buy nothing here at all, and only more
**tasks** move anything.

Two consequences worth naming before they are convenient:

- **No variance estimate is available from this subject** by repeating cells. A
  per-cell interval would have to come from varying the seed, the temperature, or
  the item — each of which changes what is being measured, and none of which is
  the same thing as a repeat.
- **A re-run is a re-derivation, not a sample.** Re-running a slate after an
  instrument fix reproduces the old answers except where the fix bites, which is
  what makes such a re-run cheap in information and worth doing only for
  comparability — one run id, one instrument. It is not corroboration.

This says nothing about the SDK subject, which is not run at temperature 0 and is
a different subject in every other respect (2026-08-26).

### 2026-08-28 (later) — A behavioural run, pre-registered as not an endpoint

A 40-cell run of `prose` against `engine-briefed` over the pinned slate — four
packs plus `controls`, 20 tasks, one trial each. **It is exploratory, it is not
S1, and no accuracy claim comes out of it.** Written down before it runs, because
it will produce a `engine-briefed − prose` delta with an interval and that number
will be sitting in the report whatever it says.

**Why it is not an endpoint.** Two independent reasons, either sufficient:

- The comparison is already recorded above as an **upper bound**, not S1.
- At 20 tasks it is 20 paired items against the 155 a +10-point effect needs.
  A run below power is not run (precondition 3), and this is deliberately below
  it. The interval will include both zero and +10 — the definition of
  *uninterpretable* in **How the result will be read**, and it is being entered
  into knowingly rather than discovered afterwards.

**What it is for**, and what may be claimed from it — process signals, each a
rate the arms can be compared on without an accuracy claim:

| what | why it is worth 40 cells |
|---|---|
| cap-hit and truncation, per arm | whether the instrument is readable at this difficulty at all. `prose` and `engine-briefed` do not share a budget |
| `answered-from`, `engine-briefed` | the arm has never run outside `controls`; compliance there is untested |
| fabricated fact bases | the current blocker (`ROADMAP.md`), measured on 16 real items instead of one |
| wall clock per arm per cell | what sizes the next real pass. The 7.5h → 42h error was an unmeasured cost extrapolated from `controls` |

**The slate may be a floor, and that is anticipated here rather than after.** On
these exact items with thinking off, `prose` scored 1/24 and `engine-forced`
0/24. If both arms land near zero this run says nothing about the engine — which
is the same finding as *the missing rung*, and is why nothing above rests on the
accuracy number.

**Precondition 5 still binds.** The instrument is checked as for any run; what is
suspended is the *power* requirement, and only because no claim is drawn.

### 2026-08-28 — A fourth arm, and the primary endpoint does not move

`engine-briefed` is added: `engine-forced` plus the engine's reference
documentation in the prompt. It exists because the 2026-08-28 `controls` gate
found `engine-forced` reaching for the engine and then writing syntax it had
invented — four cells, four fabricated CSV loaders, no `Skill` call — so the arm
was measuring whether the subject can *find and read the manual* as much as
whether the engine helps.

**The primary endpoint is unchanged**: paired `engine-forced` − `prose`,
`in-context`, weaker strength, McNemar's exact, two-sided. This is said
explicitly because the temptation the pre-registration exists to resist is
exactly here — a new arm that may look better is not a licence to move the
endpoint onto it after seeing a gate.

Two new secondary endpoints, and what each may claim:

| endpoint | question |
|---|---|
| `engine-briefed` − `engine-forced` | what does *finding the manual* cost? A capability of the subject and the skill, not of the engine |
| `engine-briefed` − `prose` | S1 with the documentation handed over: an **upper bound** on what the engine buys this subject, not S1 itself |

**Why the second is a bound and not the claim.** S1 is about an agent with a
logic engine available, and an agent that is handed the manual unprompted is a
more equipped agent than the one S1 describes. Reported as a ceiling: if
`engine-briefed` − `prose` is null, no amount of documentation work rescues the
engine for this subject, and *that* is the informative reading.

The arms remain nested — `prose` ⊂ `engine-forced` ⊂ `engine-briefed` as strict
text suffixes, `engine` byte-identical to `prose` — so every pairwise delta has
one cause. `engine-briefed` carries the same 2.0 budget as `engine-forced`
(`cell.ARM_BUDGET`), so the pair differs by the briefing alone.

### 2026-08-26 — What a local-subject grid is powered for, and why it is not +10

The preconditions above size the slate for **+10 points** and quote 78 tasks.
That figure carries an assumption it did not state: `harness power`'s default
baseline of **0.85**. Paired items needed grow as the weaker arm approaches 50%,
because that is where discordant pairs are most numerous —
`stats.required_items` uses `psi = effect + 2·min(baseline, 1 − (baseline +
effect))`. At a 50% baseline, +10 points needs **705 paired items at one
strength**, not 155.

And a slate calibrated into the informative band [0.2, 0.8] has a baseline near
50% **by construction**. The band and the power arithmetic pull in opposite
directions: the items with headroom for the engine to show in are the noisiest
items to measure it on. Nothing above is wrong; the two facts had simply never
been put next to each other.

So, for a **local subject** — one model, so paired items are tasks:

- **The primary endpoint's effect is +20 points**, needing 155 paired items at a
  50% baseline. The endpoint itself is unchanged: paired `engine-forced` −
  `prose`, `in-context`, weaker strength, McNemar's exact, two-sided.
- **Why +20.** W1 claims the engine helps *more* at the weak end; +10 is a
  strong-subject scale. The alternative — ~1,400 pool items, ~4,200 pass cells
  and a 6,345-cell grid — buys a smaller effect for three or four nights.
- **What it costs, stated in advance.** A null now excludes +20, not +10. A true
  effect between 10 and 20 points reads as *uninterpretable*, not positive.
- **Unchanged for the API subject**, whose grid is still sized at +10 against its
  own baseline. The two subjects were never comparable to each other, and this
  makes them less so; W2 already carried that caveat.

### 2026-08-31 (later) — The provenance arm, and the zero it is aimed at

A fifth arm, `engine-briefed-provenance`: `engine-briefed` plus
`catalogue.PROVENANCE_MANDATE`, appended after the briefing so the briefed
prompt stays a strict prefix. Written before the run, and this section is the
pre-registration for it.

**The finding that motivates it, measured before any of this was built.** Across
every real run on disk — **1,812 transcripts, 23,430 tool calls** — there are
**zero `?why` / `?whynot` invocations**. Not one. That includes the 16
`engine-briefed` cells of `run-20260828T203413Z`, which had the whole of
`SKILL.md` in the prompt: worked examples of both sigils, the sigil-choice rule,
and the `repair: ask ?whynot …` chain. The count was taken twice, by `grep` over
the raw files and again by `engine_use.asks_provenance` over the parsed calls,
and both return 0.

**Why the failures are the right ones to aim at.** Of the 12 failing
`engine-briefed` cells in that run, triaged offline:

| shape | n | the sigil that fits |
|---|---|---|
| a program ran and derived **nothing** (`missing == truth_size`, `extra == 0`) | **9** | `?whynot` |
| a program ran and derived the wrong rows | 2 | `?why` |
| no program at all (0 turns) | 1 | neither |

Eleven of twelve got a program running. Nine stared at an empty result set and
rewrote the program rather than asking why it was empty — `who-can-read-r03` for
35 turns, `delete-without-read` for 28. This is the mechanism behind the
2026-08-28 entry's *"the briefed arm's worst cells burn out on the engine's
silence"*, now counted.

**What the triage does not license.** It says provenance was *applicable* to nine
cells. It does not say it would have worked, and it does not say the subject
would ask. Those are the two things the run measures, and they are separable —
see the secondary endpoint.

#### The primary endpoint

> **Paired `engine-briefed-provenance` − `engine-briefed`, on the 16 pinned
> items, `qwen3-14b-structured`, by McNemar's exact test, two-sided.**

**Directional prediction, recorded in advance:** positive, and concentrated in
the nine empty-answer cells. Predicted null on `busiest-month-per-region` (no
program was ever written, so there is nothing to interrogate), and unsigned on
the two `?why` cells.

**The primary endpoint of the grid does not move onto this arm.** S1's is still
`engine-forced` − `prose`. This one is a question about the *skill*, in the same
family as `engine-briefed` − `engine-forced`: it asks what instruction buys over
documentation. An arm told when to ask and what to type is more equipped than
the one S1 describes, and the temptation this document exists to resist is
exactly the one that would promote it.

#### The secondary endpoint, and why it is not optional

> **Provenance reach — `signals.ProvenanceUse`, three-valued: `none` / `asked` /
> `acted-on`.**

The primary is unreadable without it, because a null has two opposite readings
and only this column separates them:

| reach | delta | reading |
|---|---|---|
| ~zero | null | **the instruction did not take.** A finding about the block's wording, not about provenance |
| high | null | **provenance does not help here.** A finding about the feature |
| high | positive | the arm did what it was built to do |

So a gate that finds reach still at zero **stops the run**: 32 cells measuring an
instruction nobody followed is six hours spent re-deriving the number this
section opens with.

#### What is held, and what that costs

- **Both arms run fresh in one run.** The subject is deterministic (temperature
  0.0), so the archived 4/16 could serve as the control and halve the wall
  clock. It does not: cross-run control reuse is the error class this project
  keeps catching, and the re-run doubles as a **determinism check** — if
  `engine-briefed` does not reproduce 4/16 with the same per-task verdicts, that
  is a finding touching every prior conclusion and is reported as one.
- **`ARM_BUDGET` is 2.0 on both arms.** The provenance arm is instructed to take
  strictly more steps, so its cap may bind where the briefed one's does not.
  Raising it would put two causes under the endpoint — the instruction, or the
  turns — so it is held, and `report._budget_line`'s per-arm cap-hit rate is
  what makes the cost visible. **Stated in advance: if this arm ends at its cap
  materially more often than `engine-briefed`, the delta is confounded and is
  reported as such rather than claimed.**
- **The block is an instruction, not documentation**, and that is the whole
  hypothesis. Documentation is what `engine-briefed` already carries, and its
  result is the zero above.
