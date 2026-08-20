# Temporal values — the design, and what it rejected

Long form for the 2026-08-19 decisions in `spec.md` §17 (temporal values; `std`
module builtins). §3/§4/§8/§9/§13 hold the normative text; this file holds the
argument, the alternatives that lost, and the measurements that ranked them.

## Why this ranked above the profile

`spec.md` §1's **S4** read "met **except dates**" — the one criterion the
2026-08-18 v1 ruling had to correct, and what made temporal types v1 rather than
a preference (`notes/v1-scope.md`). §13 imports CSV, JSONL and Parquet from real
files, where date columns are ubiquitous and landed as strings or ints. A fast
engine that cannot read a date column loses to a slow engine that can.

## What was actually missing — the reframe that set the scope

**Date *filtering* already worked.** ISO-8601 text sorts lexicographically and
`<` accepts strings (`bugs/006` widened the checker to match §8), so
`D >= "2024-01-01"` was already a correct range query. The three things missing
were **arithmetic**, **period grouping**, and **validation**.

This is why extraction and truncation are in the same session as the types. A
design that shipped `date`/`timestamp`/`duration` and their arithmetic but no way
to group by month would have shipped the half that already worked, and S4 would
still have read "met except dates". It is also why the ROADMAP item's own framing
(§4/§8/§13, "no arithmetic over it") understated the job.

## The sibling engine's finding, generalized

`notes/tsdl-cross-project-review.md`: a subject asked for "days" wrote
`(C - O) as number` and got `172800000` — milliseconds, no error, a number
plausible enough to ship. Their first fix was a paragraph in the guide stating the
unit; that paragraph is gone, replaced by retiring `duration as number` in favour
of `duration / duration → number`, so `(C - O) / @P1D` says days and **the divisor
is where a program names its unit**. *"A finding closed by deleting the construct
rather than documenting it."*

Taking it one step further is what produced §8's rule rather than a table. Their
fix is dimensional analysis on the *vector* side only. Adding the point side —
dates and timestamps are positions, durations are displacements — generates every
allowed and forbidden pair from one sentence:

- point − point = vector; point ± vector = point; **point + point is meaningless**
- vector ± vector = vector; vector × scalar = vector; **vector ÷ vector = scalar**
- and therefore no conversion at all between `duration` and a number

The last line is the `172800000` bug excluded *by construction*: there is no
spelling of "this duration as a number" that does not name a unit. A rule that
generates the table is also what a model can re-derive at the point of use, which
a table is not.

`duration / duration` is **`float`, not `int`**: `@36h / @1d` is `1.5`, and an
integer result would truncate exactly the way the retired cast did — the same
class of silent loss, reintroduced at the construct built to prevent it.

## Decisions, and the alternatives each beat

### Three types, `@`-sigilled literals

`date`, `timestamp`, `duration`. The literal needed a sigil because **§14's
closure property decides it**: output must re-parse as input, so a first-class
value needs a spelling `print.rs` can emit. That rules out the tempting
zero-new-syntax option — `"2026-08-19" as date` — since a *fact* has no body to
hoist a cast into, and a computed date would print as something the grammar could
not read back.

`@` was free in the lexer, begins nothing else, and makes the literal
self-delimiting. **Rejected: bare ISO** (`2026-08-19`), which today lexes as
`2026 - 08 - 19` and evaluates to `1999`. Accepting it would silently change the
meaning of existing arithmetic — the exact failure class this repo exists to
catch — and no scan-ahead rescues it, because both readings are well-formed.

### `timestamp` is civil; there are no time zones

A timestamp is a date and a clock reading, not an instant. **What this buys:** the
core engine stays zero-dependency (no IANA database), the semantics is one
sentence, and `timestamp - timestamp` is exact. **What it costs:** a zoned source
column is converted to UTC at import and its offset dropped (§13), so the
displayed clock reading can change; the instant is preserved. Stated in §13
rather than absorbed, because it changes what a row reads as.

**Rejected: instants with UTC normalization** — it preserves zoned columns
faithfully, but puts a zone concept in the value model to serve a column type
that arrives already normalized in most of what §13 reads.

### Durations are exact, and spelled `@1d12h`

No month or year unit. This is not a simplification, it is what makes the
spelling unambiguous: with no months, `m` can only mean minutes. ISO-8601 has the
same collision and spends the `T` on it (`P1M` months, `PT1M` minutes) — a known
model error, and one the friendly form cannot make.

**Rejected: calendar durations** (`@1mo`, `@1y`). A calendar duration has no fixed
length, so `duration` would split into two kinds with different algebra — `@1mo /
@1d` has no value, and `D + @1mo` needs a clamping rule for 31 January. That is
the `Period`/`Duration` split Java and JS Temporal both arrived at, and it is a
second type masquerading as one. `std/time`'s `truncate` covers period *grouping*
without it; period *arithmetic* is the acknowledged gap (open question below).

ISO-8601 is accepted as an input alias, because agents and other tools emit it and
rejecting a well-formed standard spelling buys nothing. Canonical print is the
friendly form: printing is a function of the value, not of how it was written.

### CSV inference produces temporal types

**Rejected: explicit schema only** — the rule `symbol` already lives under, and
the safest option: no existing program changes meaning. It lost because the
default CSV path is precisely the case S4 is about, and a feature whose motivating
use needs a schema line the agent has to know to write has not met the criterion.

The anchor property is restated rather than broken. It read "an import means
precisely the facts you would get by writing its cells as in-program literals";
it now adds "each cell delimited as its type requires". The precedent is already
in the rule: a CSV `alice` becomes the string `"alice"`, not the symbol `alice` —
the quotes are supplied by the reader, and `@` is the same kind of delimiter.

Inference is strict (ISO-8601 extended only); the space-separated
`2026-08-19 10:30:00` common in exports stays a string, and an explicit schema
coerces it. §13 already split inference from coercion this way; the design makes
the split deliberate.

### `timestamp as date` stays an error

The sharpest tension in the design. Truncating a timestamp to its day is what
group-by-day needs, and §8's ratified rule (2026-08-16) says a *lossy* conversion
is a structured error — `2.5 as int` errors, `as` neither rounds nor truncates.

Resolved **in favour of the existing rule**, with `std/time` supplying what the
cast refuses: the error names `truncate(T, day, D)` and the import that provides
it. Two reasons. Bending the cast rule for one pair would make "does `as` round?"
a per-type question, and the construct it would bend toward is *less* capable
anyway — `truncate` reaches week and quarter, which no cast target could.

**Rejected: granularity types** (`month` and `year` as types whose values are
`@2026-08` and `@2026`, truncation spelled `T as month`). Genuinely elegant: the
group key is a first-class printable value that sorts correctly, and closure is
exact. It lost on two counts — it doubles the type count for one operation, and it
requires arguing that a projection onto a coarser domain is not lossy, which is
the same argument the cast rule was ratified to refuse. The cost of not taking it
is real and is recorded in §16.14: a truncated value prints as a *date*
(`@2026-06-01`), so "June 2026" is a start-of-period convention rather than a
month value.

## `std` modules — the mechanism, and why it was designed here

The gate is **not a fix for a collision problem**. At prefixed names
(`date_part`, `date_trunc`) collisions are already unlikely. The gate is what makes
the *short* names affordable — `year`, `month`, `day` are exactly the names a
program wants and exactly the names a data column has. Unimported they stay
ordinary relations, so no existing program changes meaning; imported, a collision
is an error naming both origins.

**Rejected: silent user-wins shadowing**, the obvious alternative — a program
defining its own `year` would quietly get its own. That is a silent meaning
change, the class `bugs/001` and the wildcard entry both cost this project a
defect over.

**Why the mechanism and not just the module.** §17's deferred *builtin scalar
functions* question (`abs`, `length`, `lower`, `substr`) was blocked on the
`ident (` atom-vs-call ambiguity that ruled out `float(A)`. A builtin **relation**
never meets that ambiguity — a body literal opening with an identifier can only be
an atom — so `std/math` and `std/text` now have both a home and a spelling, and
stay deferred on their own merits rather than on a missing shape. Deciding only
what `std/time` needed would have set that precedent by accident, which is the
"whichever is settled first silently constrains the other" hazard from
`notes/taking-stock-2026-08-18.md`.

**The honest cost:** an agent that writes `year(D, Y)` without the import gets an
error on its first attempt. That is why §12 carries the did-you-mean naming the
import line, and why the message is part of this session's deliverable rather than
a follow-on.

## Deliberately excluded

Each is a decision, not an omission.

- **`now` / `today`.** A run-scoped clock makes the same program answer differently
  on two runs, against §13's determinism claim and §11's provenance. The agent
  already knows today's date and inlines it — which is the agent-native answer, and
  costs the language nothing.
- **Time zones and IANA data**, with the civil-timestamp decision above.
- **Calendar durations and `D + @1mo`**, with the exact-duration decision above.
- **`duration // duration → int`.** `//` stays reserved for floor division (§3),
  and a truncating duration ratio is the trap `/` was made `float` to avoid.
- **Qualified module namespacing.** `std/` is a reserved prefix that *gates* names,
  not a namespace system that *qualifies* them. §17's namespacing question stays
  open and now has a consumer.

## Open, after this session

- **Period arithmetic.** "Same day next month" and "age in years" are not
  expressible as arithmetic. `year`/`month` extraction plus comparison gets most
  of the way; a calendar duration or a granularity type is what would close it,
  and both were rejected here on their own terms rather than on this need.
- **Whether a truncated value should print as its period** rather than as the
  date that starts it (§16.14's *still raises*). This is the granularity-type
  question arriving from the output side.
- **A `duration` from a number.** Deliberately absent, and the one shape a
  consumer might genuinely want is a literal-scaled `N * @1d`, which already
  works. Recorded so the absence is not re-litigated as an oversight.
