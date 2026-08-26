"""Four questions over the applicants, and five over a generated caseload."""

from __future__ import annotations

from harness.domains.eligibility import fixture, truth
from harness.generate import Degenerate, pick_by_median
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


def criteria(caseload: fixture.Caseload) -> str:
    """The same rules against a generated caseload's own thresholds.

    All three columns are named as possibly blank whatever the difficulty, so
    the prompt is the same text at every setting and the difficulty lives in the
    **data**. Naming only the columns that happen to be blank would tell the
    subject where to look, which is the work.
    """
    return (
        "An applicant qualifies for the support programme when all four of these "
        f"hold: age is at least {caseload.min_age}; income is below "
        f"{caseload.max_income}; residency_years is at least "
        f"{caseload.min_residency_years}; and the applicant has no row in "
        "`sanction.csv`. Some applicants have no value recorded for age, income "
        "or residency_years — the cell is blank. A blank is not a zero: for that "
        "applicant, whether the criterion holds cannot be determined, so they "
        "neither meet it nor fail it."
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


def generated(seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh slate over a fresh caseload.

    The four pinned questions re-asked, plus a fifth the pinned slate cannot ask:
    *which households hold more undecided applicants than qualifying ones*. That
    one exists because every wrong answer to the four is a **subset** of the
    right one, so their shape cannot say which mistake was made; a miscount in
    either direction changes this one's answer in both.

    On the `at-scale` track the three broad questions are **narrowed, not
    dropped**. Unscoped over a 700x caseload they answer with thousands of rows,
    and an item like that measures whether a subject can emit a list. The fact
    base stays huge and the answer gets small, which is the combination the track
    is about.
    """
    caseload = fixture.generate(seed, difficulty, track)
    fx = fixture.to_fixture(caseload)
    tag = f"{seed:x}"[-4:]
    rules = criteria(caseload)
    # The subject household is picked by its answer, not by its index: a fixed
    # index picks a household nobody qualifies in on some seeds, and an empty
    # truth is an item a subject passes by writing an empty file.
    household = pick_by_median(caseload.households, truth.eligible_per_household(caseload))
    common = {
        "domain": DOMAIN,
        "fixture": fx,
        "track": track,
        "difficulty": difficulty,
    }
    return [
        Task(
            id=f"g{tag}-d{difficulty}-eligible",
            question=(
                f"Which applicants qualify? {rules}"
                if track == "in-context"
                else f"Which applicants in household {household} qualify? {rules}"
            ),
            truth=(
                truth.eligible(caseload)
                if track == "in-context"
                else truth.eligible_in(household, caseload)
            ),
            question_class="negation",
            answer_shape=("applicant",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-blocked-by-exactly-one",
            question=(
                "Which applicants fail exactly one of the four criteria, and which "
                f"criterion is it? {rules} Name the criterion as one of "
                f"{', '.join(truth.CRITERIA)}. Consider only applicants for whom "
                "every criterion can be decided — an applicant with a blank cell "
                "is not an answer here."
            )
            if track == "in-context"
            else (
                "Which households have no qualifying applicant at all? Each "
                f"applicant belongs to one household. {rules}"
            ),
            truth=(
                truth.blocked_by_exactly_one(caseload)
                if track == "in-context"
                else truth.households_with_no_eligible(caseload)
            ),
            question_class="negation",
            answer_shape=("applicant", "criterion") if track == "in-context" else ("household",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-undetermined",
            question=(
                "For which applicants is a missing value the only thing preventing "
                "a decision — at least one blank cell, and every criterion that "
                f"can be decided is met? {rules}"
            )
            if track == "in-context"
            else (
                "For which applicants is a missing value the only thing preventing "
                "a decision — at least one blank cell, every criterion that can be "
                "decided met — *and* their household contains no qualifying "
                f"applicant at all? {rules}"
            ),
            truth=(
                truth.undetermined(caseload)
                if track == "in-context"
                else truth.undetermined_where_nobody_qualifies(caseload)
            ),
            question_class="negation",
            answer_shape=("applicant",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-households-over-dependants",
            question=(
                f"Which households have more than {caseload.dependant_threshold} "
                "dependants in total across all of their members? Each applicant "
                "belongs to one household, and `dependant.csv` links each dependant "
                "to the applicant who claims them."
            ),
            truth=truth.households_over_dependants(caseload=caseload),
            question_class="aggregation",
            answer_shape=("household",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-more-undetermined-than-eligible",
            question=(
                "Which households contain strictly more applicants whose decision "
                "cannot be made than applicants who qualify? An applicant's "
                "decision cannot be made when they have at least one blank cell "
                "and every criterion that can be decided is met. "
                f"{rules}"
            ),
            truth=truth.households_with_more_undetermined_than_eligible(caseload),
            question_class="aggregation",
            answer_shape=("household",),
            **common,
        ),
    ]


def check(task: Task) -> None:
    """This pack's own invariants, on top of the universal ones.

    Read off the **emitted** fixture rather than the caseload that produced it,
    which is the point: a blank cell in `applicant.csv` is what the subject sees,
    and an oracle that agreed with the generator's in-memory rows while the
    rendered file said something else would be checking the wrong artefact.
    """
    suffix = task.id.split("-", 2)[2] if task.id.startswith("g") else task.id
    text = task.fixture.text("applicant.csv")
    rows = [line.split(",") for line in text.splitlines()[1:] if line.strip()]
    # One set per column, and their union: an arm that lists *any* of them must
    # score wrong, or the question is answerable by looking for empty cells.
    by_column = [{row[0] for row in rows if row[index] == ""} for index in (1, 2, 3)]
    blank = set().union(*by_column)
    answered = {row[0] for row in task.truth.rows}

    if suffix == "undetermined" and task.track == "in-context":
        if not blank:
            raise Degenerate(f"{task.id}: no applicant has a blank cell — the question is empty")
        for listed in [*by_column, blank]:
            if listed and answered == listed:
                raise Degenerate(
                    f"{task.id}: undetermined is exactly a set of blank cells — "
                    "listing the blanks would score correct, and the trap the pack "
                    "exists for is gone"
                )
    if suffix == "eligible" and answered == {row[0] for row in rows}:
        raise Degenerate(f"{task.id}: everyone qualifies — the criteria decide nothing")
    if suffix == "blocked-by-exactly-one" and task.track == "in-context":
        named = {row[1] for row in task.truth.rows}
        if named != set(truth.CRITERIA):
            missing = sorted(set(truth.CRITERIA) - named)
            raise Degenerate(
                f"{task.id}: no applicant fails only {missing} — an arm that never "
                "checks that criterion would score full marks"
            )
