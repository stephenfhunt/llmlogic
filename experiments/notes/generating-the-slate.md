# Generating the slate

Long-form for the 2026-08-26 generator work. `decisions.md` holds the rulings;
this file holds the per-pack knobs and — the part worth keeping — **what
measuring the generators found that reading them did not**. `ROADMAP.md` holds
the work items.

## Why generate at all

The 2026-08-24 grid could not be read, and one of its three independent causes
was that the slate is at a ceiling: opus is 20/20 in prose on the live domains,
so even at perfect adoption there is no room for a positive delta
([`discriminating-instrument.md`](discriminating-instrument.md)). The fix is not
to hand-tune harder questions — guessing which knob is hard is how the ceiling
got built. It is to generate a pool and **select** the slate by calibration.

The 28 pinned tasks are the comparable slate and do not move.
`tests/test_pinned_slate.py` hashes all of them, and was written before any
fixture was touched.

## The shape every pack takes

1. A frozen **value type** in `fixture.py` — hashable tuple fields, so a
   generated fixture can be handed to the oracle.
2. `PINNED = _build()`, with the existing module-level row names re-bound to its
   fields, so `tasks.py` and the reference corpus are untouched.
3. **`truth.py` takes the value**, defaulting to `PINNED`. This is the whole
   point: an oracle that only runs against the fixture it was written for can
   only ever be checked against that fixture's own answers.
4. `generate(seed, difficulty, track)` + `to_fixture(value)` + `generated(…)`.
5. `check(task)` — this pack's invariants. The universal ones live once, in
   `harness/generate.py`.

## What each pack's difficulty actually turns

Structure, not size — the knob list in `discriminating-instrument.md`, one row
per pack.

| pack | difficulty is | `at-scale` grows | the fifth question |
|---|---|---|---|
| `access_control` | closure depth, cross-links, near-miss revocations | users and resources | *whose access needs the full closure* |
| `eligibility` | how many columns can be **blank** | applicants | *households with more undecided than qualifying* |
| `ontology` | levels, diamonds, override depth | instances (not classes) | *declared values no instance ends up with* |
| `scheduling` | how much the day **overlaps itself**, rest-window tightness | people (not shifts) | *shifts nobody is on that somebody could work* |
| `imports` | how much is absent or unresolved, months spanned | orders (not customers) | *customers who ordered in every month* |
| `controls` | nothing — flat by design | no track | none: the rate is the finding |

The fifth question exists for one reason, and it is the same reason in five
packs: **every wrong answer to the four pinned questions is a subset of the right
one**, so their shape cannot say which mistake was made. Nine of the 2026-08-24
answers were subsets. Each fifth question is false in both directions — a wrong
answer gains rows as well as losing them — so the shape of a miss says what the
subject did.

## Thresholds are calibrated to the draw

`eligibility`'s income limit, `imports`' region threshold and `controls`' order
threshold are set from the population that was drawn, not fixed. Fixed at 32,000
against a draw topping out at 20,000, *income* stops being one of four criteria
and the item quietly becomes a three-criterion question. The same argument
applies to any question with a number in it.

`imports` goes one step further. The pinned fixture bought its strict busiest
month with a **seed search** — run the generator, keep the draw only if no region
tied — which does not survive being asked for a thousand fixtures. `_break_ties`
moves one order from the runner-up month into the leader's until every region is
strict, and `truth.busiest_month_per_region` still raises on a tie. That guard is
what makes it a repair rather than a hope.

## Fixed handfuls, not shares

`access_control` recorded this after a *proportional* isolated set put 1,920
users in one answer at `at-scale`. Every pack now has the same rule under a
different name — `RARE_INSTANCES`, `MAX_QUIET_CUSTOMERS`, `MAX_UNDECIDED`,
`AT_SCALE_SHIFTS_EACH` — because at 400x–2000x, *a share of the population* is an
answer nobody can transcribe, and an item like that measures transcription.

The combination the `at-scale` track is actually about: **a fact base nobody can
hold, and an answer somebody can write.** Measured on the finished generators —
200k to 1.2M tokens of fixture, answers of 2 to 41 rows.

## What measuring found that reading did not

Every one of these passed a reading of the code. All of them were found by
running the generator over 60 fixtures and printing the answer sizes.

- **`eligibility` d1 had one eligible applicant.** Drawn at random, almost nobody
  clears four criteria at once. The positives had to be engineered exactly the
  way the near-misses were.
- **`eligibility`'s fifth question was empty or single-row on most seeds**, until
  households were assigned *by the part each applicant plays* rather than
  sprinkled: `pending` households hold no qualifier at all, and a third of the
  undecided are placed elsewhere so the `at-scale` narrowing still narrows.
- **`ontology` crafted conflicts fell back into classes reserved as empty**,
  which silently populated three of the five empty classes and put strangers in
  the rare class. The fallback now skips the conflict instead.
- **`ontology` disjoint pairs all shared one left-hand class** — one constraint
  wearing three hats, so an arm that finds it finds all of them.
- **Every declared value was somebody's effective value on every seed**, so
  `dead-declarations` asked nothing. One spine class is now barred from holding
  any instance that is not also under the next.
- **`scheduling` planted five double bookings and no short turnaround** at the
  small end, because the crafted pairs were concatenated rather than interleaved.
- **`scheduling` answered 3,534 rows at `at-scale`.** Two or three shifts apiece
  over 9,000 people; one apiece leaves the answer the size of what was planted.
  This is why `scheduling` asks the *same* questions on both tracks.
- **`imports`' universal quantifier was satisfied by 864 of 864 customers.**
  Twenty-five orders over five months covers them all; ordinary customers now
  have a season.
- **A frozen value rehashed its rows on every cached lookup.** Eleven seconds for
  one `at-scale` test, against 1.8s for the whole file once `__hash__` was
  memoized.

The pattern is worth naming: **a generator's defects are in its output
distribution, not in its control flow.** Reading the code tells you what it
intends; only running it over many seeds tells you what it produces. That is the
same lesson `decisions.md` 2026-08-24 records about hand-verifying `scheduling` —
the check proved the oracle matched the author's reading, and could not prove
there was only one.
