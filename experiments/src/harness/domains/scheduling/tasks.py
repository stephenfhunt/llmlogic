"""Four questions over the roster."""

from __future__ import annotations

from harness.domains.scheduling import fixture, truth
from harness.generate import Degenerate
from harness.task import Task

DOMAIN = "scheduling"

#: The two definitions a reading can differ from the grader on without either
#: being unreasonable — so they are stated, not left to be inferred. Stated
#: *separately*, and each attached only to the questions that turn on it: a rule
#: quoted where it does nothing is not neutral, because a careful reader looks
#: for the use. Both were once glued onto all four questions, and a subject that
#: took the eligibility rule at its word applied it to `assignment.csv` before
#: looking for clashes — which was the right thing to do with the roster as it
#: then stood. See ``decisions.md`` 2026-08-24.
OVERLAP = (
    "`shift.csv` gives each shift's start and end as timestamps; a shift may run "
    "past midnight. Two shifts overlap when one starts strictly before the other "
    "ends and vice versa — shifts that merely touch do not overlap."
)

ELIGIBILITY = (
    "A person can work a shift only if they are qualified for its role "
    "(`qualified.csv`) and are not listed against it in `unavailable.csv`."
)


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {"domain": DOMAIN, "fixture": fx}
    return [
        Task(
            id="double-booked",
            question=(
                "Which assignments put a person on a shift that overlaps another "
                "shift they are assigned to? Report the person and the shift, once "
                f"per assignment involved. {OVERLAP}"
            ),
            truth=truth.double_booked(),
            question_class="constraint",
            answer_shape=("person", "shift"),
            notes=(
                "Every pair of one person's shifts has to be compared, and the "
                "overlapping ones are not adjacent in the file."
            ),
            **common,
        ),
        Task(
            id="unstaffable-shifts",
            question=(
                "Which shifts can nobody work — no person is both qualified for the "
                f"role and available for that shift? {ELIGIBILITY}"
            ),
            truth=truth.unstaffable_shifts(),
            question_class="negation",
            answer_shape=("shift",),
            notes=(
                "A negation over a join: it is not enough that someone is qualified "
                "or that someone is free, and 'nobody' has to hold across everyone."
            ),
            **common,
        ),
        Task(
            id="forced-assignments",
            question=(
                "Which shifts have exactly one person who could work them, and who "
                f"is that person? {ELIGIBILITY} Someone already assigned elsewhere still "
                "counts, including to a shift that clashes with this one: the "
                "question is who the roster permits, not who is left once the "
                "current assignments are taken as fixed."
            ),
            truth=truth.forced_assignments(),
            question_class="constraint",
            answer_shape=("shift", "person"),
            notes=(
                "Counting the candidates rather than finding one. A shift with two "
                "possible people looks exactly like a shift with one until both are "
                "found."
            ),
            **common,
        ),
        Task(
            id="rest-violations",
            question=(
                "Which assignments start less than "
                f"{int(truth.MIN_REST.total_seconds() // 3600)} hours after the end "
                "of another shift the same person is assigned to? Report the person "
                "and the shift that starts too soon. Overlapping shifts do not count "
                f"here — those are a different problem. {OVERLAP}"
            ),
            truth=truth.rest_violations(),
            question_class="temporal",
            answer_shape=("person", "shift"),
            notes=(
                "Arithmetic on timestamps across a day boundary: the night shift's "
                "end is on the following date, and a comparison on clock time alone "
                "gets a plausible answer."
            ),
            **common,
        ),
    ]


def generated(seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh slate over a fresh roster.

    The four pinned questions re-asked, plus a fifth the pinned slate cannot
    ask: *which shifts is nobody on, though somebody could work them*. That one
    exists because every wrong answer to the four is a **subset** of the right
    one, so their shape cannot say which mistake was made. This one is two
    negations over two different relations — an arm that gets the candidates
    wrong drops rows, and one that misses an assignment adds them — so the
    answer moves in both directions and its shape says which.

    The `at-scale` track asks the **same** five questions rather than narrowed
    ones, which is the exception among the packs and is stated in
    `fixture.AT_SCALE_SHIFTS_EACH`: everyone but the crafted handful works one
    shift, so the answers are bounded by what was planted however large the
    roster gets.
    """
    roster = fixture.generate(seed, difficulty, track)
    fx = fixture.to_fixture(roster)
    tag = f"{seed:x}"[-4:]
    hours = int(roster.min_rest.total_seconds() // 3600)
    common = {"domain": DOMAIN, "fixture": fx, "track": track, "difficulty": difficulty}
    return [
        Task(
            id=f"g{tag}-d{difficulty}-double-booked",
            question=(
                "Which assignments put a person on a shift that overlaps another "
                "shift they are assigned to? Report the person and the shift, once "
                f"per assignment involved. {OVERLAP}"
            ),
            truth=truth.double_booked(roster),
            question_class="constraint",
            answer_shape=("person", "shift"),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-unstaffable-shifts",
            question=(
                "Which shifts can nobody work — no person is both qualified for the "
                f"role and available for that shift? {ELIGIBILITY}"
            ),
            truth=truth.unstaffable_shifts(roster),
            question_class="negation",
            answer_shape=("shift",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-forced-assignments",
            question=(
                "Which shifts have exactly one person who could work them, and who "
                f"is that person? {ELIGIBILITY} Someone already assigned elsewhere "
                "still counts, including to a shift that clashes with this one: the "
                "question is who the roster permits, not who is left once the "
                "current assignments are taken as fixed."
            ),
            truth=truth.forced_assignments(roster),
            question_class="constraint",
            answer_shape=("shift", "person"),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-rest-violations",
            question=(
                f"Which assignments start less than {hours} hours after the end of "
                "another shift the same person is assigned to? Report the person and "
                "the shift that starts too soon. Overlapping shifts do not count "
                f"here — those are a different problem. {OVERLAP}"
            ),
            truth=truth.rest_violations(roster=roster),
            question_class="temporal",
            answer_shape=("person", "shift"),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-uncovered-but-staffable",
            question=(
                "Which shifts has nobody been assigned to at all, even though at "
                f"least one person could work them? {ELIGIBILITY} A shift nobody "
                "could work is not an answer here."
            ),
            truth=truth.uncovered_but_staffable(roster),
            question_class="negation",
            answer_shape=("shift",),
            **common,
        ),
    ]


def check(task: Task) -> None:
    """This pack's own invariants, on top of the universal ones.

    The first is the whole reason this pack is checked mechanically. On
    2026-08-24, 21 of 29 assignments put someone on a shift they could not work,
    so a subject that applied the stated eligibility rule before looking for
    clashes correctly answered `double-booked` with an empty file — and three of
    the four subjects did. The roster has to obey the rule its own questions
    state, and this is what says so about each item rather than about the
    generator that produced it.
    """
    fx = task.fixture

    def rows(name: str) -> list[list[str]]:
        return [line.split(",") for line in fx.text(name).splitlines()[1:] if line.strip()]

    role_of = {row[0]: row[1] for row in rows("shift.csv")}
    qualified = {(person, role) for person, role in rows("qualified.csv")}
    unavailable = {(person, shift) for person, shift in rows("unavailable.csv")}
    illegal = [
        (person, shift)
        for person, shift in rows("assignment.csv")
        if (person, role_of[shift]) not in qualified or (person, shift) in unavailable
    ]
    if illegal:
        raise Degenerate(
            f"{task.id}: {len(illegal)} assignments break the eligibility rule the "
            f"questions state, e.g. {illegal[0]} — the roster contradicts itself, and "
            "an arm that applies the rule first is right to answer nothing"
        )
    suffix = task.id.split("-", 2)[2] if task.id.startswith("g") else task.id
    if suffix in ("unstaffable-shifts", "forced-assignments") and len(task.truth.rows) < 2:
        raise Degenerate(
            f"{task.id}: one answer row — an arm that finds it and stops scores the "
            "same as one that checked every shift"
        )
