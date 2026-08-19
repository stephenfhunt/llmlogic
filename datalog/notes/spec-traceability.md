# Spec → implementation traceability

Which code validated each `spec.md` section, and when. Moved out of the spec
2026-08-16 (§17): the traceability is worth keeping, and a language definition
should not need to know the engine has a file called `lower.rs`.

**This table is the only place a spec section's implementation history lives.**
The spec states what the language *is*; §17 says why; this says what proved it.
Rows are the status lines the sections used to carry, verbatim in substance.

| § | validated by | when |
|---|---|---|
| 3 Lexical structure | the hand-rolled lexer, `src/lexer.rs` | 2026-07-22 |
| 4 Data model & types | the AST/IR prototype, `src/ast.rs` + `src/ir.rs` | 2026-07-19 |
| 4 — named-argument rules | lowering, `src/lower.rs` | 2026-07-20 |
| 4 — type inference | a post-lowering pass, `src/typecheck.rs` | 2026-07-21 |
| 4 — `declare`-signature verification | declared types on `ir::PredicateInfo.field_types`, verified in `typecheck` | 2026-07-21 |
| 5 Syntax | the AST/IR prototype (`src/ast.rs`), then the recursive-descent parser (`src/parser.rs`) | 2026-07-19, 2026-07-22 |
| 6 Declarative semantics | the differential and oracle suite the operator's claims rest on — **B1**/**C3**, the independent group-by oracle, the `absent` value laws, **C9**, **C10** (`testing.md`) | 2026-08-18 |
| 7 Negation | anti-join evaluation + Ullman relaxation numbering (`src/lower.rs`, `src/engine/`) | 2026-07-20 |
| 8 Arithmetic & comparison | the engine and the naive oracle (`src/engine/`), lowered with the assignment-safety exception (`src/lower.rs`) | 2026-07-21 |
| 9 Aggregation | design session ratified §17; `count`/`sum`/`min`/`max`/`avg` shipped | 2026-07-24 |
| 10 Recursion & safety | range restriction, plus the Termination rule as a warning (`lower::value_creating_recursion`) | 2026-08-18 |
| 11 Provenance | the data model, recorded in the fixpoint (`src/provenance.rs`); the query surface is designed, not built | 2026-07-19 |
| 12 Error model | the diagnostic shape, `src/error.rs` | 2026-07-25 |
| 13 External data / fact sources | design deep-dive ratified §17; DuckDB reader shipped as milestone 7 | 2026-07-23 |
| 14 Programmatic / agent API | `-q` one-shot queries and the CLI, milestone 6 | 2026-07-23 |
| 15 Evaluation strategy | stratified semi-naive fixpoint, milestone 2, negation at milestone 4 | 2026-07-19, 2026-07-20 |
| 16 Worked examples | encoded as AST/IR fixtures (`ast::fixtures`, `ir::fixtures`); **no example names its own test** | 2026-07-19 |

**A gap here is not a gap in the spec.** A row without a date means what the
section describes is partly unwritten, which its own *Not covered* footer states;
this table records validation, not completeness. Every section now carries one —
§10 since the Termination rule shipped and §6 since its extension, both 2026-08-18.

Keep it current the way the status lines were kept current: when a section's
behaviour is first proved by code, add the row in the same commit.
