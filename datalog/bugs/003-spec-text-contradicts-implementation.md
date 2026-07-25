---
id: 003
title: Three places where spec.md asserts something the implementation contradicts
severity: doc
area: spec
spec: ["§3", "§8", "§10"]
found: 2026-07-25
resolution:
---

Three independent normative errors in `spec.md`, grouped because they are one
sitting's work and one commit. Each was verified against the built binary. None is
a code bug — the implementation is right and the spec is wrong, which for a
document whose stated job is to define the language is the more serious direction.

## 1. §10's range restriction names one binder; there are three

§10 (`spec.md:654`) states:

> every variable in a rule head, every *named* variable in a negated atom, and
> every variable occurring only in comparisons must also occur in a positive body
> atom

An `=`-assignment target and an aggregate result also bind. Verified — both run
today:

```datalog
p(1).
h(N) :- p(A), N = A + 1.            % h(2).   §10 as written rejects this
h2(N) :- N = count { X | p(X) }.    % h2(1).  ditto
```

The implementation's own message is correct where §10 is not: "it must occur in a
positive body atom, or be bound by an `=`-assignment or an aggregate result".

§8 states the assignment exception and §9 the aggregate one, so the rule is
correct *somewhere* — but §10 presents itself as the normative home of range
restriction, and it is the section §7 and §9 point at. **This duplication is the
drift mechanism, not just the drift:** the same safety rule is stated in four
sections, and updating three of them was enough to look done. §10 should own it and
§7/§8/§9 should cross-reference it.

## 2. §3's reserved-word list is incomplete

§3 (`spec.md:97`) lists `import`, `as`, `declare`, `not`, `true`, `false`. Also
reserved, verified by probing:

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

§8's closing italic (`spec.md:498`) says precedence is "deferred to the parser
(§5, Phase D); the AST already carries whatever grouping the parser chose".
Precedence was resolved 2026-07-22 and is stated earlier **in the same section**
(`spec.md:421-424`). Leftover scaffolding.

## Acceptance criteria

- §10 states all three binders and is the single normative statement of range
  restriction; §7/§8/§9 reference it rather than restating it.
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
