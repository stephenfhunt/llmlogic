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

ollama 0.33, rootless, in `~/.local/ollama`; one RTX 3060, 12 GiB. At the
32,768-token window the harness holds it to, one model is resident at a time:

| model | VRAM at 32k | tool calls |
|---|---|---|
| `llama3.1:8b` | 8.4 GiB | native ✓ |
| `qwen3:8b` | 9.2 GiB | native ✓, thinking off via `reasoning_effort` |
| `qwen2.5-coder:7b` | 6.1 GiB | **native ✗** — emits bare JSON the parser drops |
| `llama3.2:3b` | ~2 GiB | native ✓ |

`reasoning_effort: none` on `qwen3:8b` is 2.0s and 21 output tokens against 12.4s
and 699 — the difference between a slate that fits a sitting and one that does
not. Whether thinking *helps* is a separate question the harness can now ask,
because the thinking text is kept.

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
