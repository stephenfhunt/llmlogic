# Value interning — design

*Drafted and reviewed 2026-09-14 (§17 2026-09-14). Step 4 of the fact store
(`notes/fact-store.md` § Seek-heavy queries, measured), chosen over a B-tree for
the older rows. Building on branch `fact-store`. The review's answers are at the
end.*

## Why now, against the recorded judgement

ROADMAP files interning as *a memory item, not a time one*. The 2026-08-20 profile
had `Value::eq` and `memcmp` at 2–3% of a run, and symbol length never drove time
(`notes/profile-2026-08-20.md`). That was true of the engine that kept every tuple
in its own allocation, with a value's `String` allocated beside it.

The fact store removed those per-tuple allocations. What was left exposed is where
a value's string lives:

| query, at `7ecfc33` | against `efcda71` | where its time goes |
|---|---|---|
| `lib/flow.dl` | +32%, cache misses 2.5× | 35% of cycles in `memmove` |
| `q_coh.dl` | +26% | about 40% in comparisons (`memcmp`, `Value::partial_cmp`) |
| `lib/cohesion.dl` | +24% | the same shape |

- **The join does the same work as the baseline's.** `heaptrack` counts 121,241,530
  `String::clone` calls from the join on both binaries, and the same prefix clones.
  Each copy and each comparison now reads string bytes from memory that is no
  longer near its row.
- **The old judgement was about a different engine.** On this branch, strings are
  a time item.

The principle is the store's own, one level down: **a string is held once, and a
value refers back to it** (§ Beyond this design names it).

## Shape

### A value holds a reference to its string

`Value::Symbol` and `Value::String` hold a `Sym`, a thin reference to a string
held once for the process:

```rust
pub struct Sym(&'static str);   // private field: only the interner makes one
pub enum Value { Absent, Symbol(Sym), String(Sym), Int(i64), … }
```

- **Size.** A `Value` shrinks from 32 to 24 bytes. A binding copies 16 bytes and
  allocates nothing, where today it clones a `String`.
- **Equality and hashing are by pointer.** They are correct only because every
  `Sym` comes from the interner, so equal contents are one pointer. The private
  field is what keeps that true.
- **Order is by content, as §14 requires.** `Ord` compares the strings, with a
  pointer-equality fast path to `Equal`. Canonical output, the runs' order, and
  which proof prints cannot move. Most comparisons inside a seek meet the same
  symbol, which the fast path answers without reading a byte.
- **The interner** is a process-global `Mutex<HashSet<&'static str>>`. Interning a
  new string leaks one copy and returns its reference. Only interning takes the
  lock: reading a `Sym` is a plain dereference, so a comparison or a print locks
  nothing.

### Where strings are born

Every string value is made at one of these sites, from the scope check:

| site | where |
|---|---|
| program constants | `lower.rs` (a `Constant::Symbol` or `Constant::String`) |
| imported cells | `sources/table.rs`, `coerce`: text to `string`, and a symbol by `classify_symbol` |
| casts | `engine/mod.rs`, `apply_cast`: `as string` and `as symbol` |
| fixtures and generators | `ir::fixtures`, `testgen` |

The loader interns as it types each cell. Grafana's `symbol` table has 5.8 M
string cells holding 263 MB of text, 96 MB of it distinct, so the text alone
shrinks 2.7× (`notes/memory-profile-2026-09-12.md`).

### What does not change

- **Printing** reads `sym.as_str()`.
- **Typecheck** reads the variant, not the text.
- **Hashed containers.** None keyed by a `Value` or a `Fact` iterates in an order
  that reaches output. `fact_spans` is only looked up, and lowering's maps are
  keyed by names.
- **The naive oracle** interns through the same constructors. B1 stays a
  differential of evaluation strategy, not of value representation.

## Alternatives weighed

- **`u32` ids into a global table.** Values would be 16 bytes. But every
  comparison and every print needs a table lookup, which means a lock or an
  unsafe concurrent read, and ids cannot be ordered by content as they are issued.
- **`Rc<str>` or `Arc<str>`.** This is shared ownership: count traffic on every
  copy, and no single owner. It is the shape the shared tuples measured slower
  (§17 2026-09-13 (later iii), ***Falsified***). Rejected.
- **An interner owned by `Program` or `Model`.** A value would stop being
  self-describing. Printing, typechecking, errors and the public `Model` API would
  all need the context passed in. That is a far larger change for the same bytes.
- **Order by id inside, content at the boundary.** This is cheaper still, but it
  breaks § Rules' content order everywhere a consumer sees it: a trace's near-miss,
  error-path pruning, the scan printer. It would need that audit first.

## What the tests become

- **An equivalence claim, so a property:** *interned equality is content equality.*
  - Over generated strings, including empty ones, non-ASCII ones, one-character
    differences and repeats, two values are equal exactly when their texts are,
    and hash alike when equal.
  - Their order is the texts' order.
  - The oracle is `str`'s own `Eq` and `Ord`.
  - *Mutations:* interning a string already held into a fresh copy; `Ord`
    comparing pointers.
- **A1–A5 already state the value laws,** over `arb_value`, which then draws
  interned values.
- **Guards as for every step:** the harness diff (1,109 cases, byte for byte), the
  deep seeds, and the widened gate with `q_coh.dl`, `lib/cohesion.dl` and
  `lib/flow.dl`.
- **Test churn is mechanical.** `Value::Symbol("a".to_string())` becomes
  `Value::symbol("a")` at every literal site.

## The review's answers (2026-09-14, the user's)

1. **The leak is accepted,** and recorded as the design's cost. The CLI exits after
   one run. A long-lived library user would keep every distinct string it
   interned. The fix, if one appears, is an interner owned by `Program`.
2. **Both symbols and strings are interned.**
3. **Interning alone,** measured on the widened gate before anything more about
   seek cost or the merge is decided.

## Measured (2026-09-14): faster than the baseline on every gate program

Built as `60a5495`. The harness diff is empty over 1,109 cases. Deep seeds 2 and 3
pass 525 of 525, peaking at 351 and 348 MB (442 MB at `7ecfc33`). One sitting,
interleaved, median of 3:

| program | `efcda71` s | `7ecfc33` s | interned s | RSS MB, `efcda71` → interned |
|---|---|---|---|---|
| `pointsto.dl` | 10.56 | 10.00 | 7.40 | 533 → 318 |
| `pointsto.dl` `?why` | 9.11 | 7.62 | 5.96 | 815 → 410 |
| `callreach.dl` `?why` | 1.46 | 0.91 | 0.60 | 379 → 145 |
| `sparse_800` | 1.22 | 0.91 | 0.41 | 125 → 61 |
| `q_coh.dl` | 6.42 | 8.03 | 4.09 | 251 → 220 |
| `lib/cohesion.dl` | 7.77 | 9.60 | 4.80 | 259 → 207 |
| `lib/flow.dl` | 23.19 | 30.67 | 12.58 | 438 → 288 |

- **The seek regression is closed without a B-tree.** `lib/flow.dl`'s cycles fall
  from 139.9 to 57.0 G against `7ecfc33`, and its cache misses from 619 to 142 M
  (`efcda71`: 247 M).
- **Instructions fall too** (`lib/flow.dl` 256 → 199 G): a binding copies 16 bytes
  instead of cloning a `String`, and equality compares a pointer.
- **Not separated:** how much comes from pointer comparison and how much from the
  smaller `Value`. Nothing here depends on it.
- **Not measured:** the leak's cost to a long-lived library user, and lock
  contention, which a single-threaded engine does not have.
