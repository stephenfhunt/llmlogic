---
id: 003
title: Three places where spec.md asserts something the implementation contradicts
severity: doc
area: spec
spec: ["§3", "§8", "§10"]
found: 2026-07-25
resolution: fixed 2026-07-27 — §10 made the single normative home of range restriction; §3's list completed; §8's precedence note deleted
---

Three independent normative errors in `spec.md`, grouped because they are one
sitting's work and one commit. Each was verified against the built binary. None is
a code bug — the implementation is right and the spec is wrong, which for a
document whose stated job is to define the language is the more serious direction.

**Re-verified 2026-07-27**, before the session that will fix it. All three are
still open, but item 1 has *moved*: the §10 wording it named was corrected in
passing on 2026-07-25, and the same false claim turned up in §8 instead. Line
pointers below are current as of that date; `spec.md` shifts under edits, so
locate by quoted text if they drift again.

## 1. Range restriction is stated in four places, and one of them is still wrong

**Re-verified 2026-07-27.** §10's own wording — the half this item was opened for
— **is already fixed**: it now reads "must be **bound by the body** — it must
occur in a positive body atom, or be bound by an `=`-assignment or an aggregate
result", corrected in passing by the `bugs/001` session (2026-07-25). §7 and §9
likewise state all three binders.

What this item is actually about survives intact, and is the more important half:
**the same safety rule is stated in four sections**, so "updating it" means
finding all four. That is the drift mechanism, not merely the drift — and the
2026-07-25 sweep, which had this file open, still missed one:

§8 (`spec.md:521`), on the `is [not] absent` operator, says its operand

> must be positively bound like any comparison (§10)

**"Positively" has been false since 2026-07-25**, when `bugs/001` relaxed safety
to "bound by the body". Verified against the built binary 2026-07-27:

```datalog
p(1).
h(X) :- p(A), X = A + 1, X is not absent.   % h(2). §8 as written rejects this
?- h(X).
```

So the count is unchanged — one false assertion, in a different section than when
this was filed. §10 should own the rule and §7/§8/§9 should cross-reference it,
which is what stops the next relaxation from leaving a fifth residue.

## 2. §3's reserved-word list is incomplete

§3 (`spec.md:97`) lists `import`, `as`, `declare`, `not`, `true`, `false`. Also
reserved, verified by probing (re-verified 2026-07-27):

- **`absent`** — `absent(1).` is a syntax error ("expected a relation name").
  §5 says it is reserved; §3, which carries the canonical list, does not.
- **`is`** — `is(1).` is a syntax error, and `declare p(is: int).` is rejected as a
  field name. Documented **nowhere**; it arrived with the `is [not] absent`
  operator (§17, 2026-07-24) without reaching §3.

The converse is also unstated: type names and `table` are *contextual*, not
reserved. `int(2).` and `table(2).` are legal relations — correct behaviour, and
§5 says so for `table`, but §3 never says that a `type` keyword is usable as an
identifier, so a reader has to infer it.

## 3. §8 contradicts itself on operator precedence

§8's closing italic (`spec.md:587`) says precedence is "deferred to the parser
(§5, Phase D); the AST already carries whatever grouping the parser chose".
Precedence was resolved 2026-07-22 and is stated earlier **in the same section**
(`spec.md:482-484`). Leftover scaffolding. Re-verified 2026-07-27.

## Acceptance criteria

- §8:521's "positively bound" is corrected, and §10 is the single normative
  statement of range restriction — §7/§8/§9 reference it rather than restating
  it. (§10's own wording already landed 2026-07-25; the duplication did not.)
- §3's reserved list is `import, as, declare, not, is, true, false, absent`, with a
  sentence distinguishing reserved words from the contextual ones (`table`, the
  five type names, the five aggregate operator names).
- §8's trailing precedence note is deleted.
- Cheap regression guard, if it is wanted: a test asserting each reserved word is
  rejected as a relation name and each contextual one accepted. That turns §3 from
  prose into something checkable, which is the only durable fix for this class.

## Fallout

These three are the *verified* subset of a broader hygiene problem. The rest —
stale roadmap-step pointers, `§1`/`§2` still `TBD`, the dead `Stable` status rung,
§16's preamble contradicting §16.4, §17's inconsistent chronology, §6 never
extended to negation/aggregation/absent — are not defects in the same sense (they
are staleness and unwritten sections, not false assertions), so they are ROADMAP
items rather than entries here. See ROADMAP "Spec hygiene & §6".

## Resolution

**Fixed 2026-07-27.** All three items closed; §10 now opens by declaring itself
the single normative statement of what "bound" means, and §7/§8/§9/§14 apply it
to their own constructs by reference.

**The count was six, not four — and item 1 understated itself twice.** This file
was opened on "four sections", re-verified at "four", and the sweep found a
*sixth* restatement it had never named: §8's "Mode / safety (§10)" paragraph
opened `Every comparison operand variable must be bound by a positive atom`,
which is the same 2026-07-25 residue as §8:521 and equally false. Verified
against the release binary — both a comparison operand bound by an assignment and
one bound by an aggregate result are accepted:

```datalog
p(1). p(2).
g(X) :- p(A), X = A + 1, X > 2.            % g(3).
k(N) :- N = count { C | p(C) }, N > 1.     % k(2).
```

That is the item's own thesis paying out on itself: a rule stated in six places
is one nobody can enumerate, *including the file whose entire subject is the
enumeration*. Two sweeps by two sessions each found a different subset. The fix
is not that the sixth is now correct — it is that there is no longer a sixth to
find, because only §10 states it.

**The guard was taken.** Acceptance criterion 4 was optional ("if it is wanted");
it was wanted, and it is the only item here that outlives the sitting.
`parser::tests::every_reserved_word_is_rejected_as_a_relation_name` and
`contextual_keywords_are_ordinary_relation_names` are table-driven over §3's two
lists, so a keyword added to the lexer without a §3 edit now fails a test rather
than waiting for a spec review. Both were proved non-vacuous by moving `table`
into the reserved list and `is` into the contextual one and watching each fail.
`absent_is_reserved_and_cannot_name_a_relation` was deleted — the table subsumes
it.

**Nothing in the implementation changed**, as filed. The one code edit is the
test; the binary that verified the claims predates it.
