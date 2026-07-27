---
id: 002
title: "`-q` rejects a disjunctive rule, and blames the query grammar for it"
severity: usability
area: api
spec: ["§5", "§14"]
found: 2026-07-25
resolution: fixed 2026-07-26 — the `-q` classifier accepts N same-head clauses as one rule
---

A rule with a `;` disjunction is accepted in a program file and rejected via `-q`,
with an error that describes a grammar the user did not write. `-q` is the primary
agent surface (`docs/agent-skill.md`, `skill/SKILL.md`), so the misleading message
lands on the consumer least able to diagnose it.

## Repro

```sh
$ cat d.dl
p(1). p(9). s(9).

$ datalog d.dl -q 'r(X) :- p(X), X < 5 ; p(X), s(X)'
syntax error: expected `.` to end the query, found `:-` (at 1:9)
```

The same rule in a file is fine:

```datalog
p(1). p(9). s(9).
r(X) :- p(X), X < 5 ; p(X), s(X).
?- r(X).
% r(1).  r(9).
```

A non-disjunctive rule via `-q` is also fine (`-q 'r(X) :- p(X), X < 5'` →
`r(1).`), so the trigger is specifically `;`.

## Root cause

`src/api.rs:145-152`. The `-q` classifier decides "is this a define-and-select
rule?" with

```rust
if let Ok(program) = parse(&format!("{core}."))
    && let [statement] = &program.statements[..]
    && let StatementKind::Clause(clause) = &statement.kind
    && !clause.body.is_empty()
```

The single-statement pattern `[statement]` is the bug: the parser expands
top-level DNF into **one clause per disjunct** (§17, 2026-07-22, "the parser
expands each disjunct into its own clause sharing the head"). A two-disjunct rule
parses to two statements, the pattern fails, and control falls through to the
query path at `src/api.rs:156`, which builds `?- r(X) :- p(X), … .` — hence
"expected `.` to end the query".

Two ratified decisions, each sound alone, whose composition was never checked.
`§14`'s description of `-q` ("a single clause with a non-empty body is a rule")
reads as a language-level statement but is really describing this `[statement]`
match, and stops being true once the parser desugars one clause into several.

## Fix sketch

Accept N ≥ 1 statements that are all clauses with non-empty bodies **and the same
head predicate/arity**, taking the synthesized query from the first head. The
same-head condition keeps `-q 'a(X) :- p(X). b(X) :- p(X)'` (two unrelated rules,
outside the documented contract) on the query-body path where it belongs, rather
than silently selecting only `a`.

Worth checking the head atoms are structurally identical, not merely same-arity:
the parser shares one head across disjuncts, so they will be — an assertion
documents that rather than assuming it.

## Acceptance criteria

- `-q 'r(X) :- p(X), X < 5 ; p(X), s(X)'` answers `r(1). r(9).`, matching the file
  form exactly.
- A `-q` argument and the equivalent file program produce identical output for
  every §16 rule shape — the general property this bug is one instance of.
  **This now exists**: `api::tests::dash_q_rule_equals_the_same_rule_in_a_file`
  (testing.md C8), `#[ignore]`d against this defect and failing on the minimal
  case `d(K) :- n(K, V), V = 0 ; n(K, V), V = 0`. Fixing the bug means deleting
  the `#[ignore]`; the recorded proptest seed replays the shrunk case.
- `-q 'a(X) :- p(X). b(X) :- p(X)'` still reports a *parse* error attributed to
  the argument, not a silent partial answer.
- §14's `-q` prose stops implying a single-clause parse; state it as "one rule,
  however many clauses it desugars to".

## Fallout

Nothing outside `-q`. The file path is correct, so this is a classifier defect
rather than a language one — but it does mean `-q` and file input have diverged,
which §14 presents as impossible ("`-q` … is sugar for appending `?- ...` to the
loaded program"). Any future `-q` sugar needs a spelling-equivalence test.

## Resolution

**fixed 2026-07-26** (`0c86ab1`) — `-q 'r(X) :- p(X), X < 5 ; p(X), s(X)'`
answers `r(1). r(9).`, byte-identical to the file form.

**The fix sketch was taken as written** and needed no extension: `[statement]`
became `split_first`, plus an all-clauses-share-the-head condition. The sketch's
suggested assertion that the heads are structurally identical became a *condition*
instead — cheaper and stronger. Comparing `print_atom` output rather than the AST
sidesteps span inequality without a span-zeroing helper, and it is what keeps
`-q 'a(X) :- p(X). b(X) :- p(X)'` on the query-body path, where it is still a
parse error attributed to the argument rather than a silent partial answer.

**The property was the whole acceptance criterion.** `bugs/002` is the second
defect in the "form A means form B" class (after `001`) and the first where the
property was written *before* the fix — so closing it was deleting one
`#[ignore]`, and the recorded seed replayed the shrunk case
`d(K) :- n(K, V), V = 0 ; n(K, V), V = 0` on the first run. Known failures went
three → two, both absent × negation.

**The diagnosis held completely**, including its root-cause reading that §14's
prose ("a single clause with a non-empty body is a rule") was describing a Rust
`match` arm rather than the language. That sentence was the actual carrier of the
bug — it read as normative, so nothing flagged it when the parser started
desugaring one clause into several. It is now stated as "one rule, however many
clauses it desugars to" (§14).
