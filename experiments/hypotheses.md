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
