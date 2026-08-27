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
OLLAMA_CONTEXT_LENGTH=16384 OLLAMA_FLASH_ATTENTION=1 OLLAMA_KV_CACHE_TYPE=q4_0 \
  OLLAMA_KEEP_ALIVE=30m ollama serve
```

`preflight` refuses a run whose window is smaller than it assumes, so a wrong
setting fails loudly rather than quietly — but it cannot start the server for you.
`KEEP_ALIVE` matters because cells are grouped by model: without it the model is
evicted between cells and reloaded at ~4s a time.

**What fits, measured rather than computed** (`size_vram` against `size` from
`/api/ps` — equal means fully resident, less means spilled to CPU and roughly ten
times slower):

| model | window | KV | total | on GPU |
|---|---|---|---|---|
| `qwen3:8b` | 32k | f16 | 9.16 GiB | ✓ |
| `qwen3:8b` | 32k | q8_0 | 7.11 GiB | ✓ |
| `qwen3:14b` | 32k | f16 | 13.93 GiB | ✗ spills 4.6 GiB |
| `qwen3:14b` | 32k | q8_0 | 11.61 GiB | ✗ |
| `qwen3:14b` | 16k | q8_0 | 10.18 GiB | ✗ by ~0.7 GiB — what the desktop holds |
| **`qwen3:14b`** | **16k** | **q4_0** | **9.07 GiB** | **✓** |
| `qwen3:14b` | 8k | f16 | 9.61 GiB | ✓ |

**The desktop is holding 0.7 GiB of the card, and that is the whole margin.**
Measured 2026-08-26, with `Xorg` on the NVIDIA GPU:

| holder | VRAM |
|---|---|
| `Xorg` | 451 MiB |
| `firefox` | 174 MiB |
| `systemsettings` | 86 MiB |
| `plasmashell` | 23 MiB |
| `kwin_x11` | 8 MiB |
| **total** | **742 MiB** |

That is *exactly* the amount by which the 16k/q8 row above spills. The machine is
a Ryzen 7 5700G, so it has an iGPU (`RADV RENOIR`) the display could run on
instead, and the 3060 is where every one of those megabytes is being spent on a
desktop. **Moving the display to the iGPU buys the q8 KV cache**, which is the
one that matters: q4_0 is the aggressive setting, and it degrades exactly the
long-conversation attention every cell in a run depends on.

**The table above is not measurements alone — it solves.** Qwen3-14B is 40
layers, 8 KV heads, 128-wide keys and values, so one token of KV cache is
`40 × 8 × (128+128) = 81,920` elements: **160 KB at f16, 80 KB at q8, 40 KB at
q4**. Against the measured totals that fixes the weights at **8.45 GiB** and
leaves a residual of ~0.5 GiB for compute buffers:

| context | KV | predicted | measured | residual |
|---|---|---|---|---|
| 16k q4 | 0.62 | 9.07 | 9.07 | +0.00 |
| 16k q8 | 1.25 | 9.70 | 10.18 | +0.48 |
| 32k q8 | 2.50 | 10.95 | 11.61 | +0.66 |
| 32k f16 | 5.00 | 13.45 | 13.93 | +0.48 |
| 8k f16 | 1.25 | 9.70 | 9.61 | −0.09 |

So the next configuration does not need a survey, it needs arithmetic: with the
display moved, `8.45 + KV + 0.5 ≤ 11.9` allows **q8 KV to about 32k**.

**A 14B on this card costs the window, and the window is not free.** The 8k that
fits without KV quantization is *too small for the engine arm*: SKILL.md is ~3,200
tokens against a guard threshold of 6,144, before a single tool result. 16k with
q4 KV is the configuration that holds both a 14B and a working conversation.

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
| ollama / llama.cpp, q8 KV | ~30k tokens | **~38k** |
| vLLM, AWQ + fp8 KV | ~19k | ~27k |

PagedAttention reclaims per-sequence over-allocation across *many concurrent*
sequences; a harness that runs one cell at a time has none to reclaim. So the
answer to *"can we hold q8, or a bigger window?"* is the display move and
`OLLAMA_KV_CACHE_TYPE=q8_0`, on the stack already here.

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

## Four silent misconfigurations, and the pattern

The dangerous failures here were all **quiet**. Nothing errored; numbers came out;
they were worthless.

| what | how it was found | what it cost |
|---|---|---|
| a served context of 4,096 against a declared 32,768 | reading `nvidia-smi` and wondering why only 5 GiB was resident | one sweep and the whole model survey |
| one completion decoding to 11,963 tokens, unbounded | watching the GPU while committing | one sweep |
| the *conversation* filling a 32k window a 275-token fixture never could | measuring `input_tokens` per cell — median 9.9k, p90 265k, max 762k | the later half of one sweep |
| `ran_engine` true on a transcript whose only call was `Skill` | running a local model at all | contradicted `engine_use` in every record since it was written |

The pattern is that this harness's expensive failures do not announce themselves,
and that a preflight check pays for itself the first time it fires. It is the same
lesson as the stale engine binary (2026-08-25) and the fixture that moved under a
resume (2026-08-24), which is now three independent instances and no longer a
coincidence.

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
