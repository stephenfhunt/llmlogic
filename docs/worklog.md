# Worklog

A running handoff log for chaining agentic coding sessions. Each session ends by
adding an entry so the next session can get oriented in seconds — without re-reading
raw transcripts (Claude Code auto-saves those under
`~/.claude/projects/<repo-slug>/*.jsonl`; resume with `claude --resume`).

**Conventions**
- Newest entry on top (reverse-chronological).
- Keep each entry short and high-signal. Three fields:
  - **Done** — what changed this session (link commits/PRs where useful).
  - **Decided** — key decisions made (design decisions also go in `datalog/spec.md`
    §17; note them here too so the timeline is complete).
  - **Next up** — the concrete next threads, so the following session starts oriented.
- This is a curated summary, not a transcript. Don't paste raw output here.

---

## 2026-07-03 — Bootstrap

**Done**
- Installed Rust toolchain (rustup; `cargo 1.96.1`, edition 2024).
- Scaffolded `datalog/` as a self-contained crate (library + thin CLI binary) with a
  module skeleton (`ast, lexer, parser, engine, provenance, sources, api, error`).
  Builds warning-free; unit + integration smoke tests pass; clippy/rustfmt clean.
- Wrote `datalog/spec.md` — living spec: full-featured v1 outline, spec-driven
  process, decisions/open-questions log (§17), and six provisional worked examples
  (§16: recursion, negation, arithmetic, aggregation, import, provenance).
- Added `AGENTS.md` (repo-wide guidance) and this worklog.
- Initial commit `18d68a9`.

**Decided**
- Language scope for v1: **full-featured** (facts, rules, recursion, stratified
  negation, arithmetic/comparison builtins, aggregation).
- LLM-targeting pillars: provenance/explainability, LLM-friendly syntax + structured
  errors, programmatic (JSON) agent API.
- `datalog/` is self-contained (no root Cargo workspace); library + thin binary.
- Session continuity: curated worklog (this file) + Claude Code's auto-saved
  transcripts; no raw-transcript directory in-repo.

**Next up**
- Formalize `spec.md` §3–§5 (lexical structure → data model → EBNF grammar) against
  the §16 examples.
- Ratify the **type-discipline** open question first (untyped terms vs declared
  predicate schemas) — it ripples into imports (§13), errors (§12), and the grammar.
- Then resolve the related open questions in §17 (meaning of `=`, aggregate syntax,
  import declaration syntax, provenance query syntax).
