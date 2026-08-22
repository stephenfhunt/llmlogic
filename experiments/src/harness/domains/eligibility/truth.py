"""Ground truth, in plain Python. Control 1: never the datalog engine.

Written so the missing-income case is handled in one place and visibly: ``None``
is never compared, it is asked about. Every criterion is a named predicate, so
"failed exactly one" is a count over the same definitions the eligibility answer
uses rather than a second reading of the rules.
"""

from __future__ import annotations

from harness.domains.eligibility.fixture import (
    APPLICANT_ROWS,
    DEPENDANT_ROWS,
    HOUSEHOLDS,
    SANCTION_ROWS,
)
from harness.task import Answer

MIN_AGE = 21
MAX_INCOME = 32_000
MIN_RESIDENCY_YEARS = 3
#: A household clears this many dependants across all of its members.
DEPENDANT_THRESHOLD = 4

#: The criteria, in the order the question states them.
CRITERIA = ("age", "income", "residency", "sanction")


def row(applicant: str):
    return next(record for record in APPLICANT_ROWS if record[0] == applicant)


def sanctioned(applicant: str) -> bool:
    return any(name == applicant for name, _ in SANCTION_ROWS)


def failures(applicant: str) -> set[str]:
    """Which criteria this applicant fails.

    An unrecorded income fails nothing and satisfies nothing: it is not a
    failure, it is the absence of an answer. `undetermined` is where those
    applicants are reported.
    """
    _, age, income, residency, _ = row(applicant)
    failed = set()
    if age < MIN_AGE:
        failed.add("age")
    if income is not None and income >= MAX_INCOME:
        failed.add("income")
    if residency < MIN_RESIDENCY_YEARS:
        failed.add("residency")
    if sanctioned(applicant):
        failed.add("sanction")
    return failed


def income_known(applicant: str) -> bool:
    return row(applicant)[2] is not None


def eligible() -> Answer:
    return Answer.of(
        *[name for name, *_ in APPLICANT_ROWS if income_known(name) and not failures(name)]
    )


def blocked_by_exactly_one() -> Answer:
    """``(applicant, criterion)`` for applicants who fail one criterion only."""
    rows = []
    for name, *_ in APPLICANT_ROWS:
        if not income_known(name):
            continue
        failed = failures(name)
        if len(failed) == 1:
            rows.append((name, failed.pop()))
    return Answer.of(*rows)


def undetermined() -> Answer:
    """Applicants whose income is the only thing standing between them and a
    verdict — every other criterion met, and no income on file."""
    return Answer.of(
        *[name for name, *_ in APPLICANT_ROWS if not income_known(name) and not failures(name)]
    )


def dependants_per_household() -> dict[str, int]:
    household_of = {name: household for name, _, _, _, household in APPLICANT_ROWS}
    counts = dict.fromkeys(HOUSEHOLDS, 0)
    for _, applicant in DEPENDANT_ROWS:
        counts[household_of[applicant]] += 1
    return counts


def households_over_dependants(threshold: int = DEPENDANT_THRESHOLD) -> Answer:
    return Answer.of(
        *[household for household, count in dependants_per_household().items() if count > threshold]
    )
