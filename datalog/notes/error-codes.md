# The error code vocabulary — what an agent branches on

*Recorded 2026-08-25, from the §12 code-vocabulary design session. Status:
**designed**, built in the same session. `spec.md` §12 is the normative statement
of the contract; this note holds the census the vocabulary was derived from, the
folds that shrank it, and the alternatives that lost.*

## The problem, as measured

A consumer of a diagnostic has four things to branch on today — `ErrorKind::Lex`,
`Parse`, `Semantic`, `Source` (`src/error.rs`). Those name **which stage rejected
the program**, which is a fact about the engine's internals, not about what is
wrong. Everything an agent would act on is in the prose:

```
semantic error: unsafe rule for `reachable`: head variable `Y` is not bound by
the body — it must occur in a positive body atom, or be bound by an
`=`-assignment or an aggregate result (at 5:1)
```

`Semantic` is shared by that, by a stratification failure, by a type clash and by
thirty other things whose fixes have nothing in common. So an agent that wants to
branch — retry with a bound variable, or restructure the negation, or fix a
column's type — has to match English, which is precisely what §12's opening
sentence says a consumer must never have to do. This is the last thing between
`spec.md` §1's S3 and v1.

## The census

Every emission site, clustered. Counted 2026-08-25 by reading each site, not by
grepping the constructor — the local helpers (`lexer.rs`'s `error`, `parser.rs`'s
`error_expected`, `lower.rs`'s `semantic`) mean the constructor count and the
diagnostic count differ by a factor of three.

| stage | file | sites |
|---|---|---|
| lex | `lexer.rs` | 19 |
| parse | `parser.rs` | 28 |
| semantic — lowering | `lower.rs` | 26 |
| semantic — types | `typecheck.rs`, `ir.rs` | 9 |
| semantic — evaluation | `engine/mod.rs` | 35 |
| source — modules | `resolve.rs` | 7 |
| source — data | `sources/*` | 29 |

**The site is the wrong unit**, which the span work of 2026-08-24 learned the
expensive way: 113 sites became 15 diagnostic families and two wrappers. A code
per site would be 153 codes, none of them stable, most of them naming a line of
Rust rather than a thing that is wrong.

## The unit that is right

**A code exists where an agent's fix differs in kind.** Not where the message
differs — the message says *which* field is unknown; the code says *the named
arguments do not match the schema*, and the fix is in the same place either way.

Applied honestly this cuts both ways, and both directions were taken:

- **Split** where one stage-level label hid unrelated fixes. `Semantic` alone
  covers `unsafe-rule` (bind the variable), `unstratified` (restructure the
  recursion), `type-clash` (fix a column), `arithmetic-error` (the data overflowed)
  and `internal-error` (report an engine bug). Nothing about those is one thing.
- **Fold** where distinct messages share a fix. `unknown field`, `field given
  twice`, `missing field` and `duplicate field in the schema` are all *the named
  arguments do not match the schema*; they became `field-mismatch`. Likewise a
  curly quote and a stray `\` (`unsupported-token` — replace the character), and a
  non-ground fact and a non-ground `?why` goal (`not-ground` — a variable stands
  where a constant is required).

The result is **38 codes**, not the ~25–35 the session opened by guessing. The
number is what the fold rule produced; it was not trimmed to a target, because a
code deleted to hit a number is one an agent then has to recover from prose.

**One fold was wrong, and a test found it within the hour.** `UnsupportedConversion`
originally covered both *there is no such conversion* (`30 as symbol`) and *the
conversion exists and would lose the value* (`@…T10:00 as date`). The engine's own
conversion-table test classifies those two outcomes differently, and it went red
the moment it stopped matching prose and started reading the code — because the
messages had carried the distinction all along, in the *"type error:"* /
*"conversion error:"* prefixes the code was replacing. They are `LossyConversion`
and `UnsupportedConversion` now: the lossy one has a fix (`truncate`), and that is
a fix differing in kind. The fold rule is not self-applying, and this is what
checking it against a consumer looks like.

## The vocabulary

Kebab-case, one line each. The stage stays as `ErrorKind`, so a code belongs to
exactly one category and the two axes never disagree.

**Lexical (3).** `unsupported-token` (`=<`, `!`, `\=`, `\+`, `//`, `/* */`, a
stray `?`, a curly quote, any unexpected character — a spelling from another
language, and the suggestion carries ours) · `unterminated-string` ·
`malformed-literal` (an unparseable float, an int outside `i64`, a bad temporal,
an unknown escape).

**Syntax (7).** `unexpected-token` (every *expected X, found Y*) ·
`trailing-comma` · `uppercase-relation` · `chained-comparison` ·
`compound-term` · `unknown-aggregate` (`foo { … }`, or a `{` with no operator) ·
`unsupported-construct` (disjunction in a query, a nullary predicate, prefix `-`
on an expression, mixed named and positional arguments, a `?why` goal that is not
one atom).

**Schema (5).** `arity-mismatch` · `field-mismatch` · `schema-conflict` (two
declarations of one relation disagree) · `no-schema` (named arguments with no
known field names — add a `declare`) · `name-collision` (a relation both defined
and provided by a `std` module; a query named after a defined relation).

**Safety (4).** `unsafe-rule` · `unsafe-premise` · `circular-premise` ·
`unstratified`.

**Misuse of a construct (4).** `aggregate-misplaced` (in a fact, in a `?why`
goal) · `not-ground` · `absent-misuse` (`absent` as a body argument, or compared
against) · `builtin-misuse` (a `std` builtin with named arguments, negated, or
`truncate` with a computed or unknown unit).

**Types (5).** `type-clash` (inference contradicts itself) ·
`declared-type-mismatch` (a `declare` contradicts what was inferred) ·
`type-mismatch` (an operation got a value of the wrong type — comparison operands,
arithmetic operands, a builtin's input, `min`/`max` across types, a non-numeric in
`sum`/`avg`, an ambiguous temporal operand) · `unsupported-conversion` (an `as` the conversion table does not have) ·
`lossy-conversion` (one that exists and would lose the value — a timestamp to a
date, a float with no exact integer; the fix is `truncate`).

**Evaluation (2).** `arithmetic-error` (overflow, division by zero, a date
arithmetic remainder) · `internal-error` (every *malformed IR*).

**Data sources (6).** `file-not-found` · `unsupported-format` (an extension with
no reader, a reserved `table "…"`, a `.dl` given `as`, a build without the
`duckdb` feature) · `unreadable-source` (cannot open, not UTF-8, a fetch that
failed or returned nothing) · `source-schema-mismatch` (the explicit schema and
the source disagree; a row with the wrong column count; an empty file; an illegal
or duplicate source field name) · `unconvertible-cell` (one cell that cannot
become a value) · `unsupported-column` (a column mixing two types, or a source
type that does not map onto the value model).

**Modules (2).** `module-not-found` · `module-misuse` (a query inside a module,
the reserved `std/` prefix, a URL as a module, an `as`-less import of a data
file).

## Decisions, and what lost

**An enum, not a string.** `ErrorCode` is a `#[non_exhaustive]` enum with
`as_str()` giving the slug. A bare `&'static str` was the alternative and is what
the rendered line and any future `--format json` actually carry — but a typo in a
string is a new code, silently, and the whole contract is that the set is
enumerable. The enum makes the compiler the sweep. `#[non_exhaustive]` because a
consumer must fall through to the category on a code it does not know, which is
the same rule the stability contract states.

**The code is a required constructor argument.** `Error::semantic(code, message)`,
not `Error::semantic(message).with_code(code)`. This is the direct lesson of the
span work: an optional field is how **113 sites carried no span** from §12's
drafting on 2026-07-25 until 2026-08-24, while §12 claimed errors were
structured throughout. An optional code would have decayed the
same way, and nothing would have said so. The cost is real — every site had to be
read and assigned — and that reading *is* the census above.

**No generic per-stage fallback.** The session planned one for a long tail and
found no tail: every site has a family. A generic code is a place to put the next
diagnostic without thinking, so declining it keeps the vocabulary honest. A
genuinely new kind of wrongness gets a genuinely new code, which the stability
contract permits (additive) and the pinned-set test makes visible.

**The category stays.** Dropping `ErrorKind` once a code implies its stage was
considered and rejected: the category is the one part of the line a *human* reads
first, and §12's "which vocabulary the message speaks" is still true. The code is
the machine's axis; the category is the reader's.

**Rendering: `semantic error [unsafe-rule]: …`.** The code goes in brackets after
the category, before the colon. Alternatives: rustc's `error[E0499]:` (drops the
category, and our categories are words rather than numbers), and a trailing
`[code]` after the position (which puts the machine-readable part behind the
prose the machine is trying not to read). The blast radius is every pinned
diagnostic — five in `experiments/reference/malformed/`, plus §16's fences.

## Two things the build changed, and one wart it left

**53 messages carried a pseudo-category prefix** — *"type error: "*,
*"arithmetic error: "*, *"conversion error: "*, *"malformed IR: "* — which §12
already forbade (*the diagnostic sentence: no location, no suggestion, no
category prefix*) and which nothing had enforced, because until there was a code
those prefixes were the only way to tell a type error from an overflow. They are
gone; the code says it properly.

**The crate's own tests were the first consumer to stop reading prose.** Nine
assertions matched on those prefixes, one of them a `match` arm classifying
conversion outcomes by `contains("conversion error")`. They read `error.code`
now, which is both a better test and the demonstration the vocabulary is for.

**The wart:** `internal-error` renders as `semantic error [internal-error]`,
because a *malformed IR* is raised through the semantic stage. The category is
wrong for it — nothing about the program's semantics is at fault, and the reader
it addresses is us, not the author. Fixing it means a fifth `ErrorKind`, which is
normative §12 text and an exit-code question; filed rather than folded in.

## What this does not do

A code says what kind of thing is wrong. It does not say *whether the same
program will fail the same way tomorrow*: nothing here numbers a code the way
rustc numbers `E0499`, so a code is a name, and renaming one is a breaking change
rather than a redirect. Nor does it touch **suggestion coverage** (13 of 77
semantic sites), which is the other half of "actionable" and has its own hazard
note in §12. Nor does a code reach a **warning**'s consumer any differently than
the warning's own enum variant already does — `Warning::code()` exists for
uniformity of the rendered surface, not because a `Warning` was ever unbranchable.
