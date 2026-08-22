"""Four questions over the applicants."""

from __future__ import annotations

from harness.domains.eligibility import fixture, truth
from harness.task import Task

DOMAIN = "eligibility"

#: The rules, stated once and quoted into every question that needs them. They
#: are prose rather than a table on purpose: rules as data would make both arms
#: write an interpreter, and the experiment would measure that instead.
CRITERIA = (
    "An applicant qualifies for the support programme when all four of these hold: "
    f"age is at least {truth.MIN_AGE}; income is below {truth.MAX_INCOME}; "
    f"residency_years is at least {truth.MIN_RESIDENCY_YEARS}; and the applicant "
    "has no row in `sanction.csv`. Some applicants have no income recorded at all "
    "— for those, whether income is below the limit cannot be determined, so they "
    "neither meet the income criterion nor fail it."
)


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {"domain": DOMAIN, "fixture": fx}
    return [
        Task(
            id="eligible",
            question=f"Which applicants qualify? {CRITERIA}",
            truth=truth.eligible(),
            question_class="negation",
            answer_shape=("applicant",),
            notes=(
                "Four criteria, one of them a negation over another table, and one "
                "that a blank cell silently satisfies if it is read as a zero."
            ),
            **common,
        ),
        Task(
            id="blocked-by-exactly-one",
            question=(
                "Which applicants fail exactly one of the four criteria, and which "
                f"criterion is it? {CRITERIA} Name the criterion as one of "
                f"{', '.join(truth.CRITERIA)}. Consider only applicants whose income "
                "is recorded."
            ),
            truth=truth.blocked_by_exactly_one(),
            question_class="negation",
            answer_shape=("applicant", "criterion"),
            notes=(
                "The near-miss question, and the one people actually ask. It needs "
                "the failures *counted*, not found: an applicant who fails two is "
                "not an answer, and nothing about their row says so."
            ),
            **common,
        ),
        Task(
            id="undetermined",
            question=(
                "For which applicants is the missing income the only thing "
                "preventing a decision — no income recorded, and every other "
                f"criterion met? {CRITERIA}"
            ),
            truth=truth.undetermined(),
            question_class="negation",
            answer_shape=("applicant",),
            notes=(
                "Not the same set as 'has a blank income': one applicant with no "
                "income also fails residency. An arm that lists the blanks scores "
                "one row too many and looks right."
            ),
            **common,
        ),
        Task(
            id="households-over-dependants",
            question=(
                f"Which households have more than {truth.DEPENDANT_THRESHOLD} "
                "dependants in total across all of their members? Each applicant "
                "belongs to one household, and `dependant.csv` links each dependant "
                "to the applicant who claims them."
            ),
            truth=truth.households_over_dependants(),
            question_class="aggregation",
            answer_shape=("household",),
            notes=(
                "A count over a join — dependants attach to applicants, and the "
                "question asks about households."
            ),
            **common,
        ),
    ]
