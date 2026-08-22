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

from harness.grade import ANSWER_FILE
from harness.task import FIELD_SEPARATOR, Fixture, Task


class SchemaDrift(Exception):
    """A fixture's declared schema no longer matches its data."""


def verify(fixture: Fixture) -> None:
    """Check every declared schema against the file that carries it.

    Called before a fixture is ever put in front of a subject. The failure this
    prevents is silent: a run over a drifted schema still produces a full grid of
    plausible verdicts about a question nobody asked.
    """
    for relation, fields in fixture.schemas.items():
        filename = f"{relation}.csv"
        if filename not in fixture.files:
            raise SchemaDrift(
                f"schema declares relation {relation!r} but {filename!r} is not in the fixture"
            )
        lines = [ln for ln in fixture.files[filename].splitlines() if ln.strip()]
        if not lines:
            raise SchemaDrift(f"{filename!r} is empty")
        header = tuple(field.strip() for field in lines[0].split(","))
        if header != tuple(fields):
            raise SchemaDrift(
                f"{filename!r} header is {header} but the schema declares {tuple(fields)}"
            )

    for filename in fixture.files:
        if filename.endswith(".csv"):
            relation = filename[: -len(".csv")]
            if relation not in fixture.schemas:
                raise SchemaDrift(f"{filename!r} has no declared schema")


def relation_catalogue(fixture: Fixture) -> str:
    lines = []
    for relation in sorted(fixture.schemas):
        fields = ", ".join(fixture.schemas[relation])
        rows = len([ln for ln in fixture.files[f"{relation}.csv"].splitlines() if ln.strip()]) - 1
        lines.append(f"- `{relation}.csv` ({rows} rows) — columns: {fields}")
    return "\n".join(lines)


def assemble(task: Task) -> str:
    """The prompt put in front of the subject. Identical for both arms."""
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

Use whatever approach you think is best.
"""
