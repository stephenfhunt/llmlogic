# The caller's contract — what a run that completed tells its caller

*Recorded 2026-08-18, from the caller's-contract design session (the merge §17
2026-08-18 required: integrity constraints and the truncation contract's open half
are one exit code, so they are one design). Status: **built**, except withholding,
which is specified and deliberately unnumbered — see axis D. `spec.md` §14 is the
normative statement of the vocabulary; this note holds the evidence per axis and
the alternatives that lost.*

## The problem, as measured

`src/main.rs` returned `ExitCode::SUCCESS` for every run that completed, and an
empty answer set prints nothing (§14: silence means no). So `0` meant *ran*, not
*answered*, and a caller had to parse stdout to learn anything at all —
`datalog check.dl && deploy` could not mean what it looks like it means.

Five axes were taken one at a time, which is the method the 2026-08-17 answer-shape
session recorded as load-bearing rather than procedural. Two of the five were
decided by measurement rather than by argument, and both measurements contradicted
a document.

## A. A constraint is not a language construct

**Ruled: convention, no grammar.** The language already expresses a constraint; only
the exit code was missing. Measured on the binary before anything was written:

```sh
$ datalog roster.dl -q 'not double_booked(_, _, _)'   # clean
holds(true).
$ datalog bad.dl    -q 'not double_booked(_, _, _)'   # violated
$                                                     # (nothing)
```

A negation-only body answers `holds(true).` when it holds (§14, 2026-08-17), and
§10 exempts wildcard-fresh variables under negation from range restriction, so the
check is writable today. With axis B's polarity it composes as the stock-take
wanted, and **the affirmative phrasing is safe on errors for free** — every error
code is nonzero, so `&&` cannot fire on a program that failed to compile. (The
inverted phrasing, `datalog … -q 'conflict(X)' || deploy`, is the unsafe one: `||`
fires on `2` as readily as on `1`. §14 documents the affirmative form for this
reason.)

**Rejected: a `constraint <name> :- body.` statement.** It desugars cleanly — §14's
query naming is already "a definition, not a label", exact sugar for a rule whose
head is the projection — so §10 safety, §7 stratification and §11 provenance would
all have been inherited rather than designed. It was rejected as a construct that
buys no expressiveness: every program it could write is one the query form already
writes, and it would cost a reserved word, a printer arm, and a D2 round-trip risk.

**Rejected: the headless denial `:- body.`** (ASP; references group 3,
Gelfond–Lifschitz). The spelling is standard and the parse is unambiguous — `:-` is
already `TokenKind::ColonDash` and nothing else can begin a statement with it — but
the *meaning* is not ours. In ASP a denial eliminates candidate stable models; we
compute one perfect model, so ours could only ever be a post-fixpoint emptiness
check. Borrowing the syntax would import a semantics we do not implement. It also
has no name, so a program checking three things could only report that one failed.

**Rejected: a query-level form** (`?! name: body.`, reachable from `-q`), which would
have let a caller impose a constraint over a file it did not write. Real, and still
available later; it blurs §14's line between asking and asserting, and axis B makes
the plain `-q` form do the same job.

Neither neighbour applies pressure here: Soufflé has no constraint construct
(references group 6), and the sibling engine's five-way `verdict` covers truncation,
not consistency (`notes/tsdl-cross-project-review.md`).

## B. The exit-code vocabulary is grep's

**Ruled: `0` rows found · `1` no rows · `2` did not answer**, and the contract is
stated as a **range** — `0`/`1` are answers, `≥2` means the run did not answer — so
a later refinement (a §13 source-failure code is the standing candidate) is additive
rather than a reinterpretation of `2`.

`grep`'s 0/1/2 is the precedent, and the one every shell reader already knows.
Program errors moved **1 → 2**, joining usage errors, which is affordable because
the repo has no callers outside its own tests.

**Why errors did not get a finer numbering.** §12 already carries the split, at a
finer grain than a code could: every diagnostic has a `category` (`lex`, `parse`,
`semantic`, `source`) plus location and suggestion, and §12's premise is that a
consumer never has to parse English. A numeric error taxonomy would be a second,
coarser vocabulary for the axis §12 owns — the failure §17 2026-08-18 predicted when
it merged this session in the first place. The one distinction a code could carry
that stderr does not is *what to do next*, and only a source failure differs there
(retry, not rewrite); no consumer has asked, and the range invariant keeps it cheap
to add.

**Scope of "found": any query, and a query-less run exits `0`.** Nothing was asked,
so "no rows" is not an answer to anything, and `datalog p.dl` stays usable as a
plain validity check. *Rejected: the last query decides* — it would make the code
depend on argument order, which nothing else in the CLI does.

Two codes are not ours to set and are documented as such: `101` (a Rust panic) and
`130` (`^C`).

## C. stdout stays a pure fact stream

**Ruled: nothing on stdout.** A run that has something to say and no rows to say it
with says it on stderr, and the exit code carries the verdict. stdout means exactly
one thing, S2's byte-for-byte claim stays trivially true, and §12's existing rule
("warnings go to stderr, never stdout") needs no exception.

**Rejected: a `%` comment marker on stdout.** §17 2026-08-16 did ratify `%` comments
as the device for prose on stdout — it rejected provenance-as-facts in their favour
— but the precedent does not transfer: there a comment *accompanies* the facts it
explains, and withholding has nothing to accompany. More decisively, it buys
**visibility without capability**: a downstream engine's lexer skips comments
(`src/lexer.rs:264`), so the marker cannot make a pipeline safe, only look safe. Its
only beneficiary is a human reading the stream, which is §1's lowest-priority target
user.

**The residual hazard is real and is recorded in §14 rather than left to lore.** The
truncation contract's own motivating sentence — "a shell pipeline reads stdout and
cannot see a stderr warning at all" — is only half answered by an exit code. In
`datalog a.dl | datalog - -q '…'` the downstream sees **only** stdout, and the
upstream's code is invisible without `set -o pipefail`. So a withheld run is
indistinguishable downstream from an empty one, which is precisely the
`not p(X)`-over-incomplete-`p` unsoundness the contract exists to stop. Nothing on
stdout can fix it short of emitting invalid Datalog, which S2 forbids. The closure
property and the truncation contract are in genuine tension, and the exit code is
the only channel that carries the difference.

## D. Withholding is specified and unnumbered, because it has no trigger

**Ruled: ship `0/1/2`.** §17 2026-08-16 named three live sources of truncation.
Measured against the source, none of them is one:

| claimed source | measured |
|---|---|
| §13's unrepresentable import cells | **not live.** Every unrepresentable case is a *structured error* — a nested/compound value, a precision-losing widening, a cell that will not coerce under an explicit schema. `null → absent` is ratified **data** (§13, 2026-07-24), not loss. |
| an external signal | **not live.** `eval_capped`'s round cap is test-only, runs at `u32::MAX` in production, and `src/engine/mod.rs` says outright that it "is not a budget and must not be read as one". |
| §9's aggregate skips | live, but **the relation is not short** — every row is present and a *cell* is absent, which §4 defines as data. |

So the entry's own inventory was wrong in a way nothing had noticed, and the
distinction it turns on is one the entry does not draw: **short by rows** (rows the
engine never produced) is not **absent in a cell** (data the source did not have).
Only the first is truncation, and today nothing produces it.

Numbering a code with no trigger would have been the worse error, and the range
invariant from axis B makes waiting free: withholding takes the next code up when a
trigger exists (a hosted surface's budget, an external signal). §15 says this
plainly rather than implying a mechanism that is not there.

**Rejected: widening "short by rows" to cover a lost value**, so a fold over a
relation carrying a failed cast withholds. It would have given the code a trigger
today — at the price of re-opening the 2026-08-16 cast decision from behind, which
chose `absent` over an error precisely so one bad cell in 170k rows would not kill
the query. **Rejected: withholding only where nothing can warn** (the query-level
fold), which targets the measured hole but lets a mechanism — "can we report it?" —
decide a semantics.

## E. What is actually lossy today, and what it now says

The genuinely lossy, genuinely silent case is the **failed `as` cast** (2026-08-16,
the same date as the truncation entry, and the case its "§13's cells" bullet was
reaching for): a value existed, could not be represented, and became `absent`.
Measured before the fix, on one program:

| shape | what the caller saw |
|---|---|
| `?- num(K, V).` — projected | `num("b", absent).` — visible in the data, no warning |
| `total(T) :- T = sum { V | num(_, V) }.` — folded in a **rule** | `total(42).` + §9's skip warning, which says *missing* where the truth is *malformed* |
| `?- T = sum { V | num(_, V) }.` — folded in a **query** | `answer(42).` — **entirely silent** |

The third row was a documented §9 limit ("Not covered by the warning: an aggregate
appearing only in a query, since queries are answered as projections and record no
derivations"), and the documentation was wrong about its own cause: `Model::answer`
**does** build premises — `Premise::Aggregate { skipped, present, … }` and all — and
discards them at the callback. The count was already there; nothing read it.

Both are now reported, and the wording carries the distinction the value model
draws: an absent that arrived as data reports as *missing*, an absent the engine
manufactured by failing a conversion reports as *malformed*.

**Why a warning and not an error or a withholding.** The 2026-08-16 cast decision
already ruled that an unrepresentable conversion yields `absent`; making it fatal
here would overturn it sideways. And a *static* warning — firing on every program
containing a `string as int` — is the known-bad design: the sibling engine measured
exactly that shape firing on correct programs and leading a model to a destructive
fix (`notes/tsdl-cross-project-review.md`). These fire only when a conversion
actually failed, on the run where it failed, with the count.
