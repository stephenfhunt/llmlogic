"""Assembling the prompt — control 2, and control 3.

**Control 2:** the relation catalogue is *derived from the fixture's own schemas*,
never written by hand, and the schemas are checked against the data files
themselves. A renamed field therefore fails loudly instead of quietly turning the
experiment into a measurement of something else.

**Control 3:** the prompt never mentions Datalog, the engine, or the skill, and is
**byte-identical across both arms**. Whether the engine is available is a fact
about the workspace, not about the prompt — which is what makes *"did it reach for
the engine?"* a measurement rather than an instruction followed.
"""

from __future__ import annotations

import json

from harness.grade import ANSWER_FILE
from harness.task import DATA_SUFFIXES, FIELD_SEPARATOR, Fixture, Task


class SchemaDrift(Exception):
    """A fixture's declared schema no longer matches its data."""


def _rows_in(fixture: Fixture, filename: str) -> int:
    """How many data rows a file carries, whatever format it is in."""
    if filename.endswith(".parquet"):
        return _parquet_rows(fixture, filename)
    lines = [ln for ln in fixture.text(filename).splitlines() if ln.strip()]
    return len(lines) - 1 if filename.endswith(".csv") else len(lines)


def _parquet_schema(fixture: Fixture, filename: str) -> tuple[str, ...]:
    import pyarrow.parquet as pq  # imported here: the cost is one domain's, not every run's

    contents = fixture.files[filename]
    assert isinstance(contents, bytes)
    return tuple(pq.read_schema(_buffer(contents)).names)


def _parquet_rows(fixture: Fixture, filename: str) -> int:
    import pyarrow.parquet as pq

    contents = fixture.files[filename]
    assert isinstance(contents, bytes)
    return pq.read_metadata(_buffer(contents)).num_rows


def _buffer(contents: bytes):
    import pyarrow

    return pyarrow.BufferReader(contents)


def _header_of(fixture: Fixture, filename: str) -> tuple[str, ...]:
    """The field names a file declares, on its own terms."""
    if filename.endswith(".parquet"):
        return _parquet_schema(fixture, filename)

    lines = [ln for ln in fixture.text(filename).splitlines() if ln.strip()]
    if not lines:
        raise SchemaDrift(f"{filename!r} is empty")
    if filename.endswith(".csv"):
        return tuple(field.strip() for field in lines[0].split(","))
    if filename.endswith(".jsonl"):
        try:
            record = json.loads(lines[0])
        except json.JSONDecodeError as exc:
            raise SchemaDrift(f"{filename!r} line 1 is not JSON: {exc}") from exc
        if not isinstance(record, dict):
            raise SchemaDrift(f"{filename!r} line 1 is not a JSON object")
        return tuple(record)
    raise SchemaDrift(f"{filename!r} is not a format the engine imports")


def verify(fixture: Fixture) -> None:
    """Check every declared schema against every file that carries it.

    Called before a fixture is ever put in front of a subject. The failure this
    prevents is silent: a run over a drifted schema still produces a full grid of
    plausible verdicts about a question nobody asked.

    A relation stored in more than one format is checked in *each* of them, and
    the copies are required to agree on row count. That is what keeps a redundant
    Parquet copy honest rather than decorative — a stale copy is a different fact
    base wearing the same name.
    """
    for relation, fields in fixture.schemas.items():
        counts = {}
        for filename in fixture.spellings(relation):
            if filename not in fixture.files:
                raise SchemaDrift(
                    f"schema declares relation {relation!r} but {filename!r} is not in the fixture"
                )
            header = _header_of(fixture, filename)
            if header != tuple(fields):
                raise SchemaDrift(
                    f"{filename!r} declares fields {header} but the schema declares {tuple(fields)}"
                )
            counts[filename] = _rows_in(fixture, filename)
        if len(set(counts.values())) > 1:
            raise SchemaDrift(f"the copies of {relation!r} disagree on row count: {counts}")

    declared = fixture.declared_sources()
    for filename in fixture.files:
        if filename.endswith(DATA_SUFFIXES) and filename not in declared:
            raise SchemaDrift(f"{filename!r} has no declared schema")


def relation_catalogue(fixture: Fixture) -> str:
    lines = []
    for relation in sorted(fixture.schemas):
        fields = ", ".join(fixture.schemas[relation])
        spellings = fixture.spellings(relation)
        names = ", ".join(f"`{name}`" for name in spellings)
        rows = _rows_in(fixture, fixture.source(relation))
        same = " — the same rows in each format" if len(spellings) > 1 else ""
        lines.append(f"- {names} ({rows} rows) — columns: {fields}{same}")
    if assets := fixture.assets():
        lines.append(_asset_summary(fixture, assets))
    return "\n".join(lines)


def _asset_summary(fixture: Fixture, assets: list[str]) -> str:
    """One line for a whole source tree.

    Listing every path would put the tree in the prompt twice — once as a
    catalogue and once as the files themselves — and the subject is meant to walk
    it, not to be handed an index of it.
    """
    roots = sorted({name.split("/", maxsplit=1)[0] for name in assets})
    total_lines = sum(len(fixture.text(name).splitlines()) for name in assets)
    where = ", ".join(f"`{root}`" for root in roots)
    return f"- {where} — a source tree: {len(assets)} files, {total_lines} lines"


#: What `engine-forced` adds, and the whole of what it adds.
#:
#: Appended verbatim to the base prompt, so the two texts differ by this block
#: and nothing else — a test pins it as a strict suffix. Naming the binary and
#: the file extension is deliberate: the arm exists to measure whether *using*
#: the engine helps, so leaving the subject to discover how to invoke it would
#: put the adoption question back inside the arm that was built to exclude it.
MANDATE = """
You must answer using the `datalog` logic engine, which is on your PATH.

- Write your reasoning as a Datalog program in a `.dl` file.
- Run it with `datalog` and read the facts it prints.
- Your answer must be what the engine derived, not what you worked out yourself.

If the engine rejects your program, repair it from the diagnostic and run it
again.
"""


def assemble(task: Task, arm: str = "prose") -> str:
    """The prompt put in front of the subject.

    Byte-identical on ``prose`` and ``engine`` — that identity *is* control 3, and
    it is asserted in ``tests/test_controls_hold.py``. ``engine-forced`` is that
    same text plus ``MANDATE``, appended and nothing else.
    """
    return _base(task) + (MANDATE if arm == "engine-forced" else "")


def _example(shape: tuple[str, ...]) -> str:
    """Two lines of placeholders, with this task's own arity.

    Derived rather than fixed, and that is the point. A fixed two-field example
    shown to a one-field question is an invitation to add a field, which is
    exactly the mistake being fixed: `qwen3:8b` returned the three correct order
    ids as `o2|150`, right answer with the amount appended. The placeholders are
    obviously not values and obviously not field names.
    """
    letters = "abcdefgh"[: len(shape)]
    return "\n".join(FIELD_SEPARATOR.join(f"{letter}{row}" for letter in letters) for row in (1, 2))


def _base(task: Task) -> str:
    shape = FIELD_SEPARATOR.join(task.answer_shape)
    return f"""\
The files in your current working directory describe a situation. Answer the
question below from them. Refer to the files by their plain names, as listed —
they are in the directory you are already in, and nothing you need is outside it.

Files present:
{relation_catalogue(task.fixture)}

Question:
{task.question}

Write your final answer to `{ANSWER_FILE}` in that same directory:

- one result per line, and nothing else in the file — no prose, no headers
- each line has the fields `{shape}`, separated by `{FIELD_SEPARATOR}`
- order does not matter; duplicates are ignored
- if the answer is empty, write an empty file

An answer with two results would be exactly these two lines, where each
placeholder stands for one value you worked out:

{_example(task.answer_shape)}

Use whatever approach you think is best.
"""
