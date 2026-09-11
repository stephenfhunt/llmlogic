---
id: 010
title: a JSONL file with no records does not import, even under an explicit schema
severity: crash
area: import
spec: ["§13"]
found: 2026-09-10
resolution: fixed 2026-09-10 — a record-less JSONL file is the empty relation under an explicit schema and a structured error without one, the empty-CSV rule (§13)
---

An empty `.jsonl` file — or one of blank lines only — fails to import whatever the
program says about it:

```datalog
import "none.jsonl" as none(id: string, n: int).
```

```
source error [source-schema-mismatch]: in `./none.jsonl`: the explicit schema names `id`, which the source does not have (source fields: json)
source error [source-schema-mismatch]: in `./none.jsonl`: the explicit schema names `n`, which the source does not have (source fields: json)
source error [source-schema-mismatch]: in `./none.jsonl`: the source has field `json`, which the explicit schema does not name (schema fields: id, n)
```

It should print nothing and exit 0: §13's anchor property is that an import means
the facts you would get by writing its cells as literals, and a file of no records
is no facts. The CSV reader already gets this right — an empty CSV under an
explicit schema is the empty relation, and without one it is an error naming the
cure.

Found building `skill/tools/ts-facts`, which writes one table per relation and
has many legitimately empty ones on a small project. The error names a field
(`json`) that appears nowhere in the file, which is the part that costs a round.

## Root cause

`src/sources/duckdb.rs` `read_jsonl`: DuckDB's `DESCRIBE … read_json` of a
record-less file reports a single column named `json` rather than none, so the
`columns.is_empty()` branch meant for this case never fired. And had it fired,
`table.rs` `arrange` bound `Some([])` by name against the schema and reported
every schema field missing — so both halves were wrong.

## Resolution

- `read_jsonl` confirms a lone `json` column with a record count and reports a
  record-less file as `columns: Some([])` with no rows. A real single-key `json`
  file still reads as data.
- `arrange` gives a self-describing source with no records its own arm: the
  explicit schema's fields, or the structured error.
- Acceptance, both halves (a reader rejecting every such file would satisfy the
  second alone): `tests/imports.rs`
  `a_jsonl_file_with_no_records_is_empty_under_a_schema_and_an_error_without`,
  and `sources::table::tests::a_self_describing_source_with_no_records_needs_a_schema`.
  Mutation: dropping the count check reddens the first.
- F4's schema-less round trip now draws at least one record: a record-less file
  names no fields, and passed F4 before only because F4 skipped its field check
  on an empty table — asserting nothing about the case this defect is.
