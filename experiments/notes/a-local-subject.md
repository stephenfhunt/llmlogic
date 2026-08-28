# A local subject

Long-form for the 2026-08-26 local-model work. `decisions.md` holds the rulings;
this file holds the measurements behind them and — the part worth keeping — **the
four times the harness was willing to report plausible numbers from an instrument
that was misconfigured**. `ROADMAP.md` holds the work items.

## Why a local subject at all

Two of the three claims in scope cannot be measured without a model weaker than
haiku, and there is none
([`discriminating-instrument.md`](discriminating-instrument.md)): *the engine
helps at the weak end*, and *a small model with the engine matches a frontier
model without it*. The second is the strongest single number this repo could
produce.

## What it runs on

ollama 0.33, rootless, in `~/.local/ollama`; one RTX 3060, 12 GiB, of which the
desktop holds ~0.8 GiB. One model is resident at a time.

**Start the server deliberately — the defaults are wrong for this.** ollama serves
at 4,096 tokens unless told otherwise, which is the defect that cost a sweep and
the whole model survey before it:

```sh
LLAMA_ARG_FIT_TARGET=640 OLLAMA_CONTEXT_LENGTH=20480 OLLAMA_FLASH_ATTENTION=1 \
  OLLAMA_KV_CACHE_TYPE=q8_0 OLLAMA_KEEP_ALIVE=30m ollama serve
```

`preflight` refuses a run whose window is smaller than it assumes, so a wrong
setting fails loudly rather than quietly — but it cannot start the server for you.
`KEEP_ALIVE` matters because cells are grouped by model: without it the model is
evicted between cells and reloaded at ~4s a time.

**What fits, measured rather than computed** (`size_vram` against `size` from
`/api/ps` — equal means fully resident, less means spilled to CPU and roughly ten
times slower). The `qwen3:14b` rows were re-measured 2026-08-27 and the q8 answer
changed; `decisions.md` 2026-08-27 (*q8 KV fits*) holds why.

| model | window | KV | needs | on GPU |
|---|---|---|---|---|
| `qwen3:8b` | 32k | f16 | 9.16 GiB | ✓ |
| `qwen3:8b` | 32k | q8_0 | 7.11 GiB | ✓ |
| `qwen3:14b` | 16k | q8_0 | 9.70 GiB | ✓ — 41/41 layers |
| **`qwen3:14b`** | **20k** | **q8_0** | **10.03 GiB** | **✓ — needs `FIT_TARGET=640`** |
| `qwen3:14b` | 24k | q8_0 | 10.37 GiB | ✓ — needs `FIT_TARGET=288`, 0.87 GiB spare |
| `qwen3:14b` | 18k | q8_0 | 10.35 GiB | ✗ *at the default margin* — 40/41 layers |
| `qwen3:14b` | 20k | q8_0 | 10.52 GiB | ✗ *at the default margin* |
| `qwen3:14b` | 24k | q8_0 | 10.89 GiB | ✗ *at the default margin* — 38/41 layers |
| `qwen3:14b` | 32k | q8_0 | 11.61 GiB | ✗ |
| `qwen3:14b` | 32k | f16 | 13.93 GiB | ✗ spills 4.6 GiB |
| `qwen3:14b` | 16k | q4_0 | 9.07 GiB | ✓ |
| `qwen3:14b` | 8k | f16 | 9.61 GiB | ✓ |

**The window's ceiling is a setting, not the card.** Every ✗ row above spills only
at ollama's *default* fit margin, and that margin is `LLAMA_ARG_FIT_TARGET` — the
free VRAM per device its fitter declines to spend, ~1.15 GiB unset. Lower it and
the same windows load 41/41: 20k at 640 MiB, 24k at 288. So the honest reading of
a ✗ is *"not at the default margin"*, and the question a window has to answer is
how much true headroom it leaves, which is the last column below.

Residency is corroborated two ways throughout, because `size_vram` alone is one
signal: the ollama log's `offloaded 41/41 layers`, and decode at **31.1 tok/s**,
the top of the resident band below. A spilled row is ~10× slower, so a plausible
`size_vram` with a 3 tok/s decode would mean the probe was being lied to.

**What each window actually leaves free**, measured at the peak of a `controls`
cell rather than at load — the number that decides whether the desktop can grow
into it:

| window | fit target | resident | free at peak |
|---|---|---|---|
| 16k | default | 9.70 GiB | 1.46 GiB |
| **20k** | **640 MiB** | **10.03 GiB** | **1.21 GiB** |
| 24k | 288 MiB | 10.37 GiB | 0.86 GiB |

**20k is the recommended rung, not 24k.** 24k works and was verified under load
(`results/run-20260828T004958Z`, 3/3), but 0.86 GiB is less than the 0.72 GiB the
desktop already holds — one browser and it OOMs mid-cell, which is a stopping rule
firing for a reason that has nothing to do with the experiment. 20k keeps a margin
wider than the whole desktop and still buys 25% more conversation than 16k.

**A spilled row's `needs` is inflated by the spill.** At 18k the breakdown is 9.67
GiB on the card plus 0.68 GiB of host buffers for the one layer that did not fit;
a configuration that fits pays no host side at all. So the right column to read
across rows is *on GPU*, and `needs` for a ✗ row is not what that window would
cost if it fit.

**The desktop holds 0.72 GiB of the card.** Measured 2026-08-27, a **Wayland**
session — `Xorg` is gone, and `plasmashell` has grown into part of what it held:

| holder | VRAM |
|---|---|
| `plasmashell` | 304 MiB |
| `systemsettings` | 82 MiB |
| `kwin_wayland` | 52 MiB |
| `Xwayland` + `xwaylandvideobridge` | 4 MiB |
| **`nvidia-smi` total, incl. driver** | **723 MiB** |

The machine is a Ryzen 7 5700G, so it has an iGPU (`RADV RENOIR`) the display
could run on instead, and the 3060 is where every one of those megabytes is being
spent on a desktop. **Moving the display now buys the window rather than the KV
type** — the arithmetic below puts q8 at ~32k with the card to itself, on top of
the 20k that the fit margin already reaches without it.

**The table above is not measurements alone — it solves**, and the block formats
are where the naive arithmetic goes wrong. Qwen3-14B is 40 layers, 8 KV heads,
128-wide keys and values, so one token of KV cache is
`40 × 8 × (128+128) = 81,920` elements — but `q8_0` stores 32 values in 32 bytes
**plus a 2-byte scale**, so it costs 1.0625 bytes each, and `q4_0` 0.5625:

| KV type | bytes/element | per token | 16k window |
|---|---|---|---|
| f16 | 2 | 160 KiB | 2.50 GiB |
| q8_0 | 1.0625 | **85 KiB** | **1.33 GiB** |
| q4_0 | 0.5625 | 45 KiB | 0.70 GiB |

The 85 KiB is not derived and then hoped for: the server prints it —
`llama_kv_cache: size = 1360.00 MiB (16384 cells)`. Against the measured totals
that fixes **weights at 8.23 GiB and compute buffers at ~0.15 GiB**, and the model
then predicts the resident rows to within 0.02 GiB (8k f16: 9.62 predicted, 9.61
measured; 16k q4: 9.08 against 9.07; 16k q8: 9.70 against 9.70).

**ollama does not spend the whole card, and that is the gap an earlier version of
this note could not account for.** Two deductions come off the 12 GiB before a
byte of model: the driver's own reserve, so llama.cpp sees **11,907 MiB**, and the
fitter's headroom — at 18k it logged `9901 MiB used, 1171 MiB free` and put a
layer on the host rather than spend that. So the budget is not *free VRAM*:

    usable ≈ (11.63 − desktop) − FIT_TARGET ≈ 10.81 − FIT_TARGET

**and the second term is a setting**, defaulting to ~1.15 GiB. That default is
what made 16k look like a ceiling. At `FIT_TARGET=640` the budget is 10.19 GiB and
20k q8 (10.03) fits; at 288 it is 10.53 and 24k (10.37) fits. The model predicts
each of those to within 0.03 GiB, which is the check that it is arithmetic and not
a story fitted after the fact.

The remaining lever is the display: on the iGPU, `usable ≈ 11.60 − FIT_TARGET`,
so **q8 to ~32k at a safe 640 MiB margin** — and that is where the 32k the earlier
arithmetic predicted actually lives. It was right about the destination and wrong
about the route, having worked from the card's nominal 12 GiB and no reserve at
all.

**A 14B on this card costs the window, and the window is not free.** The 8k that
fits without KV quantization is *too small for the engine arm*: SKILL.md is ~3,200
tokens against a guard threshold of 6,144, before a single tool result. 16k with
q8 KV is the configuration that holds both a 14B and a working conversation.

At the 32,768-token window the earlier sweeps used:

| model | VRAM at 32k (f16 KV) | tool calls |
|---|---|---|
| `llama3.1:8b` | 8.4 GiB | native ✓ |
| `qwen3:8b` | 9.2 GiB | native ✓, thinking off via `reasoning_effort` |
| `qwen2.5-coder:7b` | 6.1 GiB | **native ✗** — emits bare JSON the parser drops |
| `llama3.2:3b` | ~2 GiB | native ✓ |

`reasoning_effort: none` on `qwen3:8b` is 2.0s and 21 output tokens against 12.4s
and 699 — the difference between a slate that fits a sitting and one that does
not. Whether thinking *helps* is a separate question the harness can now ask,
because the thinking text is kept.

## What thinking costs, and the cap it walks into

`qwen3:14b` is a hybrid reasoning model and the harness had been running it at
`--reasoning-effort none` for throughput. Turning that off is the one source of
subject power that costs no VRAM, and the reason it is a knob rather than a
default is here.

**Thinking is recorded but never re-sent.** The assistant message appended to the
conversation carries `content` only, not `reasoning` (`local.py`, both protocol
loops), so thoughts do not accumulate in the window. The cost is per-turn decode,
not context — which is why a thinking run needs wall clock rather than a bigger
window.

**The trap is `max_tokens`, which bounds reasoning *and* answer.** Measured on one
structured completion, temperature 0:

| prompt | effort | wall | thinking | outcome |
|---|---|---|---|---|
| single-hop lookup | on | 13.5s | 1,134 chars | parses |
| three-hop closure | on | 12.6s | 1,442 chars | parses |
| five-constraint puzzle | on | 62.6s | 8,498 chars | **cap hit, does not parse** |
| the same, cap 4,096 | on | 60.0s | 7,246 chars | parses |
| all three | `none` | 1.4–5.4s | — | parse |

**The window and the output cap are one setting, not two.** `CONTEXT_BUDGET` gives
the conversation 75% of the window and leaves the rest for the reply: at 24k that
is 6,144 tokens against a 4,096 cap, comfortable. At 16k it would be 4,096 against
4,096 — a thinking turn allowed to fill the headroom exactly, with the overflow
guard and the output cap arriving at the same instant. That is the second reason
the window went to 24k, and it is why raising the cap without the window would
have traded one silent truncation for another.

At the 2,048 default the puzzle spent its whole budget thinking and returned
content that was not an action — a `malformed_calls` retry whose cause is
invisible in the record, because **ollama reports `completion_tokens` excluding
the reasoning it just charged against the cap** (67 tokens reported against ~1,870
actually spent). So the usage numbers under-report a thinking turn and the wall
clock is the honest signal. Hence `--max-output-tokens`, and 4,096 for a thinking
run.

**Thinking costs ~7.5× wall clock.** The `controls` smoke, three cells, same task,
same 24k/q8 server:

| arm | thinking off | thinking on |
|---|---|---|
| prose | 6.4s, `correct` | 108.2s, **`wrong`** |
| engine | 5.6s, `correct` | 28.6s, `correct` |
| engine-forced | 68.3s, `correct` | 465.9s, `correct` |
| **total** | **80s** | **605s** |

That prose cell is **not** representative — re-run, the same cell was `correct`,
and the full gate below says so. It is worth keeping only for what it shows about
the failure mode: thinking-on reasoned for 9,887 characters, explicitly raised the
risk — *"the case might matter … maybe use case-in[sensitive]"* — then ran
`awk -F, '/Carol/ {print $2}' employee.csv > answer.txt` against a fixture whose
row says `carol`, wrote an empty file, and declared itself done without looking.
More reasoning, more confident, no answer.

**The gate says thinking is a gain, and the gain is where the blocker is.** All 4
control tasks × 3 arms, one trial, 52 minutes (`results/run-20260828T012126Z`),
against the thinking-off cells of `run-20260827T015701Z` on the same units:

| arm | thinking off, per-trial | thinking on, per-trial |
|---|---|---|
| prose | 10/12 = 83% | 3/4 = 75% |
| engine | 11/12 = 92% | 4/4 = 100% |
| **engine-forced** | **3/12 = 25%** | **2/4 = 50%** |
| **overall** | **24/36 = 67%** | **9/12 = 75%** |

**Compare per-trial with per-trial.** `hypotheses.md` precondition 1 was recorded
as *"structured 10/12 = 83% PASS"*, and that figure is **any-of-3** — the same
cells score 67% majority-of-3 and 67% per-trial. A one-trial 75% read against an
any-of-3 83% is not a failed floor, it is two different statistics.

**Not one of the three failures is a wrong conclusion.** A Datalog-shaped answer
(`carol_dept("engineering").`), a prose answer carrying the CSV header row
(`id / o2 / o4 / o5`), and one cell that ran out of wall clock at 904s. The
reasoning was right in all three; the delivery was not. That is failure modes 1
and 2 below, and it is what a thinking pass should expect to spend its losses on.

## The card is power-bound, not thermally bound

Measured mid-run: 76 °C, **168.9 W against a 170 W cap**, SM clock 1777 of 2130
MHz, `SW Power Cap: Active`, and a hardware-thermal-slowdown counter of **0 µs**
across the whole session. The reduced clock is the governor holding the envelope,
not the card in trouble.

**The power cap cannot be raised** — `Max Power Limit` is 170.00 W, equal to the
default; only the 100 W floor is reachable. It would not help anyway: decode here
is **memory-bandwidth-bound**. 360 GB/s against 8.45 GiB of weights puts a ceiling
near 41 tok/s and the observed rate is 21–31, which is 50–75% of it — the
signature of a bandwidth limit, where a compute-bound job would sit at a few
percent of its FLOPs ceiling. Watts are not the lever; bytes per second are.

The iGPU is no use for *inference* for the same reason: it shares DDR4 at roughly
50 GB/s, about seven times less, and splitting layers runs the model at the slow
link. ollama declines it by default (`OLLAMA_IGPU_ENABLE`), which is correct. Its
value is displacing the desktop, above.

## Another stack: what vLLM would and would not buy

Worked from the memory model above rather than from the reputation, because the
reputation points the wrong way. vLLM's AWQ weights (~8.6 GiB) plus its
CUDA-graph and activation overhead (~1.2 GiB against llama.cpp's ~0.5) leave a
**smaller** KV budget, and **fp8 is its KV floor** where llama.cpp reaches q4:

| stack | today | display on the iGPU |
|---|---|---|
| ollama / llama.cpp, q8 KV | **20–24k, measured** | ~32k, predicted |
| vLLM, AWQ + fp8 KV | ~19k, projected | ~27k, projected |

**The vLLM row is a projection resting on an overhead figure that has since
moved**, so redo it before it decides anything: llama.cpp's compute buffers
measure ~0.15 GiB, not the ~0.5 assumed, and the ~1.15 GiB that actually goes
missing is ollama's fitter declining to spend it — a reserve vLLM manages
explicitly through `gpu_memory_utilization` and might not pay at all.

PagedAttention reclaims per-sequence over-allocation across *many concurrent*
sequences; a harness that runs one cell at a time has none to reclaim. So the
answer to *"can we hold q8?"* was `OLLAMA_KV_CACHE_TYPE=q8_0` on the stack
already here, and the remaining question — *a bigger window* — is the display
move, not a second stack.

What vLLM is worth trying for is **grammar-constrained decoding**, which
`structured` — the protocol decided on precisely because it is a grammar the
model cannot leave — pays for under llama.cpp's GBNF. Median cell time, native
against structured, on the 2026-08-26 sweep:

| model | arm | native | structured | |
|---|---|---|---|---|
| `llama3.1-8b` | engine | 4.7s | 36.7s | **×7.8** |
| `llama3.1-8b` | engine-forced | 6.5s | 48.6s | ×7.5 |
| `qwen2.5-coder-7b` | engine-forced | 1.5s | 50.5s | **×33** |
| `qwen3-8b` | engine-forced | 44.4s | 89.8s | ×2.0 |

xgrammar compiles the grammar once instead of walking it per token. Second prize
is **prefix caching**, which a three-trial slate reuses by construction. Third is
continuous batching, which needs the harness to run cells concurrently and would
change what the per-cell wall clock — a *stopping rule* — means.

The costs are a multi-GB torch install, a separate AWQ/GPTQ artifact, no
co-residency with ollama, and a `preflight` branch: vLLM's silent
misconfiguration is `gpu_memory_utilization`, not a 4096 default, and the rule
from three instances stands — refuse, do not degrade.

## Two tool protocols, and only one of them is enforced

`native` sends a `tools` array and reads `tool_calls` back. `structured`
constrains the decoder to `action_schema` — a discriminated union, one variant per
action, each pinning its own argument names.

The difference is not stylistic. Given a schema requiring `zebra` and `quantity`,
all three models emitted exactly those keys, where `{"type": "json_object"}`
produced free-form JSON: the structured path is a **grammar the model cannot
leave**. Given a *tool* whose sole required parameter was `zebra`, `qwen3:8b`
called it with `{"file": ...}` — a parameter absent from the schema — while
`llama3.1:8b` complied: the `tools` array is a **description the model may
follow**.

A free-form `arguments` object is not enough on its own. Constrained only to
*some* JSON, three of three models invented `file` for `file_path`, which is a
malformed call wearing valid syntax.

Which protocol wins is therefore a property of the model, not a design choice, and
both are kept so the harness can answer it the way it answers everything else.

## What the models actually get wrong

Not, mostly, the reasoning. Read the transcripts and the failures sort into three
kinds, none of which is "could not work out the answer":

1. **Never delivering it.** On `qwen3:8b` the thought reads *"Carol is listed
   under the 'engineering' department. The answer is 'engineering'"* — and then it
   finishes without a `Write`, because in conversation saying the answer *is*
   delivering it. 55% of one sweep graded `no-answer`, much of it right and filed
   nowhere.
2. **The shape.** `qwen3:8b` found exactly the right three orders and wrote
   `o2|150` — the answer with the amount appended. `llama3.1:8b` wrote the field
   name as the value, six times over.
3. **Looping.** 58 cells of that sweep hit the turn cap without writing anything,
   almost all under `structured`, which at the time gave the model nowhere to plan
   between actions.

Each has a fix in `decisions.md`, and each fix is bounded, applied to every arm,
and recorded — because each one moves the instrument.

## What is wrong with engine-forced

It should not be possible for `engine-forced` to do worse than `engine` on the
tasks where `engine` reaches — it has the same tools and strictly more
instruction. It was, by a lot, and the arm's runtime was an order out. Both facts
had one cause and two passengers.

**The headline, over every run on disk (2026-08-27):**

| arm | n | correct | no-answer | ended at its cap |
|---|---|---|---|---|
| prose | 853 | 19% | 31% | 16% |
| engine | 441 | **29%** | 39% | 20% |
| `engine-forced` | 343 | **5%** | **64%** | **48%** |

**Nearly half of `engine-forced` was terminated by the turn cap, and 75% of those
wrote nothing.** Its 5% was substantially a reading of the cap.

**The cause is structural, not incidental.** `prose` answers in read-then-write.
`engine-forced` must read, write a program, run it, read the diagnostic, repair,
re-run, *then* write an answer — and every repair is a round trip. Its
**successful** cells took a median 13 turns against `engine`'s 4, topping out at
21 against a 24 cap. An equal cap over unequal work is not a held constant.

**What it is not.** Two plausible explanations, both checked and both wrong:

- *It doesn't read the skill.* It reads it **more** — `Skill` invoked in 203/343
  cells against `engine`'s 149/417. Better informed, and worse.
- *The arm is mis-built.* It is the base prompt plus `MANDATE`, appended,
  test-pinned as a strict suffix. There is no wiring defect.

**The two passengers.**

- **The mandate had no exit.** *"Your answer must be what the engine derived, not
  what you worked out yourself"* means a subject whose program will not compile
  has no legal move. `engine` falls back to `grep` and scores 29%;
  `engine-forced` cannot, and loops until a stopping rule ends it.
- **The engine's output format violates the answer contract.** `datalog` prints
  `answer("engineering").`; the contract wants `engineering` and "nothing else in
  the file". The arm was told to transcribe something whose literal form the
  grader rejects — 3.7% of its answers, 0% of both other arms, all `wrong`.

**And the reason none of this was visible for three sessions:** the turn cap
recorded nothing. The wall clock wrote `_OUT_OF_TIME` and the context guard wrote
`_OUT_OF_CONTEXT` from the day each was written; the loop simply ran out, leaving
a transcript identical to one that finished and wrote no answer. **A stopping rule
that does not record itself is a silent truncation.** That is this project's
standing failure mode found for the fourth time — and the first time in its own
bounds rather than in someone else's default.

### What the fixes did, and what they uncovered (2026-08-28)

Re-gated on `controls`, all 4 tasks x 3 arms, thinking on
(`results/run-20260828T104124Z`). The mechanism moved exactly as intended, and
the arm's accuracy is now a different question than it was:

| | before (every run on disk) | this gate |
|---|---|---|
| ended at its cap | 48% | **0/4** |
| turns | median 13, max 21 of 24 | 4, 5, 6, 9 of 48 |
| wall clock | 617s mean, 900s cap firing | 135-327s of 1800s |
| fact-shaped answers | 3.7% of answers | **none** — `Sales`, `acme`, bare values |
| the mandate's exit | did not exist | `engine_unusable` used once, correctly |

**It is not the cap any more, and it is not the format. It is the syntax.** All
four of the arm's first programs invented a way to load a CSV — `read_csv/4`,
`csv_load/3`, `csv_read_line/2`, `csv_read/4` — four different guesses, none of
them the engine's `import "employee.csv" as employee.`. **Not one of the four
cells called `Skill`**, where the previous gate's did in 2 of 4. The workspace
carries the skill at `.claude/skills/datalog/SKILL.md` and the tool advertises
it, and the arm wrote from priors regardless.

So `engine-forced` at 1/4 is now measuring *whether the subject can write this
engine's Datalog unaided*, which is a real question about S1 but not the one the
arm was built for. `MANDATE` names the engine, the answer format and the repair
loop, and says nothing about the reference sitting in the workspace — deliberate
on `engine`, where reaching unprompted is the measurement (control 3), and
load-bearing in a way nobody chose on `engine-forced`, where reaching is already
mandated.

**And one number the gate puts under pressure:** `answered-from` was 3/4 = 75%
against precondition 2's 80% floor, the missing cell being the `engine_unusable`
one — a cell that complied with the mandate as written. A compliance check that
counts the mandate's own legal exit as non-compliance is measuring the wrong
thing; whether that verdict belongs in the numerator is an open question in
`decisions.md`, not a number to adjust after seeing it.

### What briefing the arm bought (2026-08-28)

`engine-briefed` — `engine-forced` plus `SKILL.md` in the prompt — measured the
same night on the same four control tasks (`results/run-20260828T132615Z`):

| | `engine-forced` | `engine-briefed` |
|---|---|---|
| first program used `import "f.csv" as r.` | **0/4** | **3/4** |
| `answered-from` (compliance) | 3/4 | **4/4** |
| correct | 1/4 | 2/4 |
| ended at its budget | 0/4 | 0/4 |

**The discovery hypothesis holds on the thing it predicted**: given the manual,
the subject stops inventing the import syntax. Accuracy at n=4 says nothing yet,
and the arm's one flip — `department-of`, `Sales` → `engineering` — is a single
cell.

**What the fix exposed underneath.** Two failures that the syntax barrier had
been hiding:

- **The engine's quotes.** `orders-above-100` ran a correct program, stripped
  the fact wrapper, and kept the quoting: `"o2"` where the contract wants `o2`,
  graded `wrong` with 3 rows missing and 3 extra. `datalog` has no bare-value
  output mode, so every consumer must strip quotes and every instruction must
  say so. The mandate now does.
- **A fabricated fact base.** `engineering-headcount` wrote its facts inline —
  `employee(name: "Alice", department: "Engineering", …)` — inventing rows
  rather than importing the CSV sitting beside it, then spent 22 turns and 1,027s
  and wrote no answer. Having the manual did not stop it; it is a different
  failure from not knowing the syntax.

## What the 900s cap actually was (2026-08-28)

The parked pass's headline — *7 of 12 prose cells at the 900s wall clock* — was
read as **multi-hop items are slow**, and that reading is what produced the 42h
projection. The transcripts say something else.

**Per cell, on `results/cal-20260828T110615Z`** (`reasoning` block lengths, in
characters; a block near 18,000 is one that spent the whole 4,096-token output
cap):

| cell | tool calls | blocks near the cap | wall |
|---|---|---|---|
| `d1-resources-for-user` | 5 | **0** | 119s |
| `d2-resources-for-user` | 6 | **0** | 137s |
| `d1-who-can-read` | 6 | 1 | 237s |
| `g2825-d1-resources-for-user` | 5 | 1 | 281s |
| `g2825-d1-who-can-read` | 5 | 1 | 483s |
| `d1-delete-without-read` | 5 | 2 | **900s** |
| `d2-who-can-read` | 6 | 2 | **947s** |
| `d2-delete-without-read` | 7 | 3 | **900s** |
| `d1-no-access` | 7 | 4 | **900s** |
| `d1-needs-the-full-closure` | 17 | 4 | **902s** |
| `d2-no-access` | 3 | 5 | **900s** |
| `d2-needs-the-full-closure` | **0** | **6** | **900s** |

**10 of 12 carry at least one; 28 in all.** The two carrying none are the two
that finished. The last row is the mechanism at its purest: six completions,
24,576 output tokens — exactly six times the cap — **zero tool calls**, and a
record reading `turns=0`, `no-answer`, *cell exceeded its wall clock*.

**What is in the tokens, and why the cap is the wrong lever.** Not a degenerate
repetition loop — 213 distinct sentences out of 220. The model is planning the
whole algorithm in its head, before reading a file, and never committing:

> *"But how to find all roles that are reachable in two steps or more…"*
> *"But since I can't process the data, I need to make an educated guess."*

The healthy first block on the same pack is ~1,800 characters and ends *"Let me
read that file."* A legitimate action is ~120 tokens. So 4,096 is not a tight cap
on the *action*; it is being consumed by a plan that does not terminate, and a
larger cap buys a longer spiral. On `controls`, where every cell finishes, output
per turn is 113-185 tokens across all four arms and not one completion truncates.

**Why it laps.** `content` comes back empty, `json.loads("")` raises, and the loop
called it a malformed call and said so — a false statement about what the model
did, naming a fault it could not act on. The reasoning is not fed back, so the
next lap starts from an unchanged conversation and is identical. ~150s each.

## Five silent misconfigurations, and the pattern

The dangerous failures here were all **quiet**. Nothing errored; numbers came out;
they were worthless.

| what | how it was found | what it cost |
|---|---|---|
| a served context of 4,096 against a declared 32,768 | reading `nvidia-smi` and wondering why only 5 GiB was resident | one sweep and the whole model survey |
| one completion decoding to 11,963 tokens, unbounded | watching the GPU while committing | one sweep |
| the *conversation* filling a 32k window a 275-token fixture never could | measuring `input_tokens` per cell — median 9.9k, p90 265k, max 762k | the later half of one sweep |
| `ran_engine` true on a transcript whose only call was `Skill` | running a local model at all | contradicted `engine_use` in every record since it was written |
| a completion cut off mid-thought, retried as a malformed call | reading `reasoning` block lengths against wall clock, cell by cell | the 240-cell pass, parked on a 42h projection that was partly this |

The pattern is that this harness's expensive failures do not announce themselves,
and that a preflight check pays for itself the first time it fires. It is the same
lesson as the stale engine binary (2026-08-25) and the fixture that moved under a
resume (2026-08-24), which is now three independent instances and no longer a
coincidence.

**The fifth is the one that should be uncomfortable**, because the bound was ours.
`finish_reason` sat in every reply and was read by nothing, so the harness
inferred *the model emitted junk* from evidence that equally meant *we cut it
off*. The two want opposite responses. Every prior instance was a vendor default
or a stale artefact; this one and the turn cap before it were the harness
mis-describing its own behaviour to itself.

So: preflight refuses a server that is not there, a model that was never pulled,
and a window smaller than the run assumes; a cell is bounded in wall clock, in
tokens per turn, and in conversation size; and every one of those bounds is a
*stopping rule* rather than an error, because an ERROR cell is one `resume` owes
forever and would fail the same way on every sitting.

## What is deliberately unequal

Two things the local subject gets that the SDK subject does not, both recorded in
`decisions.md` and both applied identically to every arm, so the comparison the
experiment is actually about — prose against engine, within one subject — is
untouched:

- **the completion reminder**, at most twice, naming only the file;
- **`thought`** on every structured action, which is parity with what `native`
  already allows in the assistant message rather than an extra.

The answer-format example is *not* in this list: it went into the shared prompt,
so both subjects and every arm see it.
