"""What every pack's generator needs, in one place.

A generated item is worthless in the same three ways whatever domain it came
from, so the rule lives here rather than seven times over — the drift mechanism
`../../datalog/bugs/resolved/003` names is a rule stated in more than one place,
and two sweeps each finding a different subset of them.

A pack adds its **own** invariants on top by calling `validate` and then checking
what only it can know: that `eligibility`'s undetermined set is not just the
blank-income set, that `scheduling`'s roster obeys the eligibility rule its own
questions state. Those are not restatements of the rules below; they are the
things a hand-written fixture got by being read by a person.
"""

from __future__ import annotations

from harness.task import Task


class Degenerate(Exception):
    """A generated item a subject could pass without doing the work."""


def validate(task: Task) -> None:
    """Refuse an item that carries no information.

    Three ways a generated item is worthless, all of which the pinned slate
    checks for by hand in `tests/test_controls_hold.py` and none of which a
    generator gets for free:

    - an **empty** truth is passed by writing an empty file without looking;
    - a truth that is the **whole universe** is passed by copying a column;
    - a truth of **one row** over a large universe is close enough to guessable,
      and carries almost nothing about whether rows were dropped — **except on a
      negative control**, where a single-row answer is the entire point. The rule
      exists so an item carries information about whether the engine helped, and
      a control is the item that is deliberately trivial; *which department is
      carol in?* has one row by construction and is not thereby worthless.

    Raised rather than filtered, because a generator that silently drops a third
    of its items is a generator whose difficulty setting no longer means what it
    says.
    """
    rows = task.truth.rows
    if not rows:
        raise Degenerate(f"{task.id}: empty truth — doing nothing scores correct")
    if len(task.answer_shape) == 1:
        universe = {
            field.strip()
            for name, contents in task.fixture.files.items()
            if isinstance(contents, str) and name.endswith(".csv")
            for line in contents.splitlines()[1:]
            for field in line.split(",")
        }
        answered = {row[0] for row in rows}
        if universe and answered == universe:
            raise Degenerate(f"{task.id}: the answer is everything — copying a column passes")
    if len(rows) == 1 and len(task.fixture.files) > 1 and task.engine_expected_to_help:
        raise Degenerate(f"{task.id}: a single-row answer is close to guessable")


def pick_by_median(candidates: tuple[str, ...], sizes: dict[str, int]) -> str:
    """The candidate with the median non-empty answer size.

    Picking a question's subject by *index* is what a hand-written fixture does,
    and on a generated one it picks an entity nobody can reach on some seeds —
    an empty truth, which is an item a subject passes by writing an empty file.
    The median keeps the item off both extremes without tuning the fixture to it.

    ``sizes`` is a precomputed map rather than a callable: calling the oracle per
    candidate is quadratic and stopped an `at-scale` slate from building at all.

    Falls back to the first candidate only if *every* one answers nothing, which
    means the fixture itself is degenerate — `validate` is what says so, and it
    says it about the item rather than silently here.
    """
    scored = sorted(((sizes.get(name, 0), name) for name in candidates), key=lambda pair: pair[0])
    non_empty = [name for count, name in scored if count > 0]
    return non_empty[len(non_empty) // 2] if non_empty else candidates[0]
