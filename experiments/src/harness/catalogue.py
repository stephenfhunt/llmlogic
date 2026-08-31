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

from harness.cell import BRIEFED_ARMS, MANDATED_ARMS, PROVENANCE_ARM
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
- The engine prints its results as Datalog facts, like `answer("x").`. The
  answer file takes the values themselves, in the format described above — write
  `x`, not the fact the engine printed it in, and not the quotes it printed
  around it.

If the engine rejects your program, repair it from the diagnostic and run it
again. If it still will not run your program after several attempts, use the
`engine_unusable` action to say so and stop — that is a real outcome and a
better one than an empty answer file.
"""


#: What introduces the briefing, so the subject knows what it is reading and
#: that it is the same document as the one in the workspace. One sentence: the
#: briefing arm exists to remove a *discovery* step, not to add instruction.
BRIEFING_HEADER = """
The reference documentation for the `datalog` engine follows. It is the same
document as the skill in your workspace, reproduced here so you do not have to
go looking for it.

---

"""


#: What `engine-briefed-provenance` adds, and the whole of what it adds.
#:
#: **An instruction, not documentation, and that distinction is the experiment.**
#: `?why` / `?whynot` are already documented in `SKILL.md` — worked examples, the
#: sigil-choice rule, the `repair: ask ?whynot …` chain — and `engine-briefed`
#: reproduces that document in the prompt. It produced **zero invocations across
#: 1,812 transcripts**. Adding more prose about provenance would re-run a
#: question already answered, so this block tells the subject *when to act and
#: what to type*, which is the step `MANDATE` takes for engine use.
#:
#: Naming the exact invocation is deliberate for the same reason `MANDATE` names
#: the binary: leaving the subject to derive the call from the manual would put
#: the discovery question back inside the arm built to exclude it.
#:
#: **Verified against the engine, not against the manual** (2026-08-31): the
#: `blocked at` and `repair:` lines are what `?whynot` actually prints, and both
#: goals exit 0. The honest caveat about the repair — that it advances the rule
#: rather than promising the goal, and is sometimes unnameable — is in the text
#: on purpose. `spec.md` §12 records the measured hazard: a suggestion a model
#: cannot act on costs a round, and one that is wrong is worse than none.
PROVENANCE_MANDATE = """
When your program runs but does not print what you expected, **do not rewrite
it.** Ask the engine why first, and repair from what it tells you.

- It printed nothing, and you expected a row? Ask why that row is missing:
  `datalog yours.dl -q '?whynot goal("x")'`
- It printed a row you did not expect? Ask what derived it:
  `datalog yours.dl -q '?why goal("x")'`

The goal must name one complete fact, with no variables in it. A wrong guess
between the two still answers, so pick by what you saw and do not deliberate.

`?whynot` reports, for each rule that could have derived the fact, the first
literal that `blocked` it and a `repair` — a step that advances that rule. The
repair is not a promise that the goal then holds, and sometimes there is none to
name. `?why` prints the derivation down to the facts it rests on, tagged
`[fact]`; when an answer is wrong, that is usually where the problem is.

Both print `%` comment lines and neither changes the exit code, so asking costs
you nothing.
"""


def assemble(task: Task, arm: str = "prose", briefing: str | None = None) -> str:
    """The prompt put in front of the subject.

    Byte-identical on ``prose`` and ``engine`` — that identity *is* control 3, and
    it is asserted in ``tests/test_controls_hold.py``. ``engine-forced`` is that
    same text plus ``MANDATE``, appended and nothing else. ``engine-briefed`` is
    *that* text plus the engine's reference documentation, appended and nothing
    else, so each arm is a strict suffix of the next and every pairwise delta has
    one cause.

    The briefing is **passed in, not read here**: this module derives a prompt
    from a `Task` and touches no filesystem, and `arms` already owns where the
    skill lives and which block an ablation cuts from it. Refused rather than
    defaulted when it is missing — a briefed arm with no briefing is
    `engine-forced` running under another name, and it would record as the arm it
    is not.
    """
    if arm in BRIEFED_ARMS and not briefing:
        raise ValueError(
            f"{arm} is a briefed arm — it is handed the engine's documentation — "
            "and none was supplied; running it without one would record an "
            "engine-forced cell under a briefed arm's name"
        )
    if briefing and arm not in BRIEFED_ARMS:
        names = ", ".join(sorted(BRIEFED_ARMS))
        raise ValueError(f"only {names} take a briefing; {arm} was given one")
    prompt = _base(task) + (MANDATE if arm in MANDATED_ARMS else "")
    if arm in BRIEFED_ARMS:
        prompt += BRIEFING_HEADER + briefing
    if arm == PROVENANCE_ARM:
        prompt += PROVENANCE_MANDATE
    return prompt


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


def _fields_line(task: Task) -> str:
    """The field bullet, which cannot be one sentence for both arities.

    Written as one, it told a single-column question that its lines had "the
    fields `order_id`, separated by `|`" — naming a separator where there is
    nothing to separate, while `_example` showed `a1` and `a2` on their own
    lines. Measured across `results/`: **15.4% of single-column cells graded
    `unparseable` against 1.9% of two-column ones**, and 144 of the 145
    single-column ones had more `|`-fields on a line than the question has
    columns. The models were following this line, not ignoring it.

    Grading stays strict, deliberately. Flattening those answers at parse time
    would have turned 119 of them into 104 `wrong` and 15 `correct` — mostly
    relabelling format failures as reasoning ones, some of it false credit where
    `engineering|engineering` collapses to the truth under set semantics — and
    the wrong-flips fall 47/34/23 across prose, engine and engine-forced, which
    is bias toward the engine (`grade.py`). Fix the instruction, not the ruler.
    """
    shape = FIELD_SEPARATOR.join(task.answer_shape)
    if len(task.answer_shape) == 1:
        return (
            f"- each line is one `{shape}` value and nothing else — do not join "
            f"values with `{FIELD_SEPARATOR}`"
        )
    return f"- each line has the fields `{shape}`, separated by `{FIELD_SEPARATOR}`"


def _base(task: Task) -> str:
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
{_fields_line(task)}
- order does not matter; duplicates are ignored
- if the answer is empty, write an empty file

An answer with two results would be exactly these two lines, where each
placeholder stands for one value you worked out:

{_example(task.answer_shape)}

Use whatever approach you think is best.
"""
