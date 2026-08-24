"""Four questions over the roster."""

from __future__ import annotations

from harness.domains.scheduling import fixture, truth
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
