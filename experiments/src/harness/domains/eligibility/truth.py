"""Ground truth, in plain Python. Control 1: never the datalog engine.

Written so an absent value is handled in one place and visibly: ``None`` is
never compared, it is asked about. Every criterion is a named predicate, so
"failed exactly one" is a count over the same definitions the eligibility answer
uses rather than a second reading of the rules.

Every function takes the ``Caseload`` it is asked about, defaulting to the pinned
one. That is what lets the oracle be checked against a second formulation on a
*generated* caseload: an oracle that can only run against the single caseload it
was written for can only ever be checked against that caseload's own answers,
which is how a fixture comes to be tuned to its truth.
"""

from __future__ import annotations

from functools import lru_cache

from harness.domains.eligibility.fixture import PINNED, Caseload
from harness.task import Answer

#: The pinned caseload's thresholds, under the names the four pinned questions
#: quote. A generated caseload carries its own; these are `fixture`'s, re-exported
#: so the question text has one place to read them from.
MIN_AGE = PINNED.min_age
MAX_INCOME = PINNED.max_income
MIN_RESIDENCY_YEARS = PINNED.min_residency_years
DEPENDANT_THRESHOLD = PINNED.dependant_threshold

#: The criteria, in the order the question states them.
CRITERIA = ("age", "income", "residency", "sanction")


def row(applicant: str, caseload: Caseload = PINNED):
    return next(record for record in caseload.applicants if record[0] == applicant)


def sanctioned(applicant: str, caseload: Caseload = PINNED) -> bool:
    return any(name == applicant for name, _ in caseload.sanctions)


@lru_cache(maxsize=8)
def _verdicts(caseload: Caseload) -> dict[str, tuple[frozenset[str], frozenset[str]]]:
    """Each applicant's ``(failed, unknown)`` criteria, in one pass.

    ``Caseload`` is frozen with tuple fields, so it hashes; a different caseload
    is a different cache key. The pass matters at `at-scale`, where the obvious
    spelling rescans `sanctions` per applicant.
    """
    under_sanction = {name for name, _ in caseload.sanctions}
    verdicts = {}
    for name, age, income, residency, _ in caseload.applicants:
        failed, unknown = set(), set()
        if age is None:
            unknown.add("age")
        elif age < caseload.min_age:
            failed.add("age")
        if income is None:
            unknown.add("income")
        elif income >= caseload.max_income:
            failed.add("income")
        if residency is None:
            unknown.add("residency")
        elif residency < caseload.min_residency_years:
            failed.add("residency")
        if name in under_sanction:
            failed.add("sanction")
        verdicts[name] = (frozenset(failed), frozenset(unknown))
    return verdicts


def failures(applicant: str, caseload: Caseload = PINNED) -> set[str]:
    """Which criteria this applicant fails.

    An unrecorded value fails nothing and satisfies nothing: it is not a
    failure, it is the absence of an answer. `undetermined` is where those
    applicants are reported.
    """
    return set(_verdicts(caseload)[applicant][0])


def unknowns(applicant: str, caseload: Caseload = PINNED) -> set[str]:
    """Which criteria cannot be decided for this applicant at all."""
    return set(_verdicts(caseload)[applicant][1])


def income_known(applicant: str, caseload: Caseload = PINNED) -> bool:
    return row(applicant, caseload)[2] is not None


def eligible(caseload: Caseload = PINNED) -> Answer:
    verdicts = _verdicts(caseload)
    return Answer.of(
        *[
            name
            for name, *_ in caseload.applicants
            if not verdicts[name][0] and not verdicts[name][1]
        ]
    )


def blocked_by_exactly_one(caseload: Caseload = PINNED) -> Answer:
    """``(applicant, criterion)`` for applicants who fail one criterion only."""
    verdicts = _verdicts(caseload)
    rows = []
    for name, *_ in caseload.applicants:
        failed, unknown = verdicts[name]
        if not unknown and len(failed) == 1:
            rows.append((name, next(iter(failed))))
    return Answer.of(*rows)


def undetermined(caseload: Caseload = PINNED) -> Answer:
    """Applicants whose missing value is the only thing standing between them and
    a verdict — every criterion that *can* be judged met, and something absent."""
    verdicts = _verdicts(caseload)
    return Answer.of(
        *[name for name, *_ in caseload.applicants if verdicts[name][1] and not verdicts[name][0]]
    )


def dependants_per_household(caseload: Caseload = PINNED) -> dict[str, int]:
    household_of = {name: household for name, _, _, _, household in caseload.applicants}
    counts = dict.fromkeys(caseload.households, 0)
    for _, applicant in caseload.dependants:
        counts[household_of[applicant]] += 1
    return counts


def households_over_dependants(threshold: int | None = None, caseload: Caseload = PINNED) -> Answer:
    limit = caseload.dependant_threshold if threshold is None else threshold
    counts = dependants_per_household(caseload)
    return Answer.of(*[household for household, count in counts.items() if count > limit])


# ---- The generated slate's questions ----------------------------------------
#
# Scoped or aggregated forms of the four above. They exist because an `at-scale`
# caseload answers the unscoped questions with thousands of rows, and an item
# like that measures whether a subject can emit a list rather than compute one.


def _by_household(caseload: Caseload) -> dict[str, list[str]]:
    grouped: dict[str, list[str]] = {household: [] for household in caseload.households}
    for name, _, _, _, household in caseload.applicants:
        grouped[household].append(name)
    return grouped


def eligible_in(household: str, caseload: Caseload = PINNED) -> Answer:
    verdicts = _verdicts(caseload)
    return Answer.of(
        *[
            name
            for name in _by_household(caseload)[household]
            if not verdicts[name][0] and not verdicts[name][1]
        ]
    )


def eligible_per_household(caseload: Caseload = PINNED) -> dict[str, int]:
    """How many eligible applicants each household holds, in one pass.

    Asking `eligible_in` per candidate is the obvious spelling and is quadratic;
    picking a question's subject that way is what stopped an `at-scale` slate
    building in `access_control`.
    """
    verdicts = _verdicts(caseload)
    return {
        household: sum(1 for name in names if not verdicts[name][0] and not verdicts[name][1])
        for household, names in _by_household(caseload).items()
    }


def households_with_no_eligible(caseload: Caseload = PINNED) -> Answer:
    """Households where nobody qualifies — a negation over a join, and one whose
    answer stays the size of the household list however large the caseload is."""
    counts = eligible_per_household(caseload)
    return Answer.of(*[household for household, count in counts.items() if count == 0])


def undetermined_where_nobody_qualifies(caseload: Caseload = PINNED) -> Answer:
    """Undetermined applicants in a household that has no eligible member.

    Two strata: the per-applicant verdict, and then a negation over a count of
    it. An arm that computes either alone gets a superset.
    """
    verdicts = _verdicts(caseload)
    empty = {row[0] for row in households_with_no_eligible(caseload).rows}
    return Answer.of(
        *[
            name
            for name, _, _, _, household in caseload.applicants
            if household in empty and verdicts[name][1] and not verdicts[name][0]
        ]
    )


def households_with_more_undetermined_than_eligible(caseload: Caseload = PINNED) -> Answer:
    """Two aggregates over two derived relations, compared.

    The question the pinned slate cannot ask: every wrong answer to the four
    pinned ones is a subset of the right one, so their shape cannot say *which*
    mistake was made. This one is false for a household an arm miscounted in
    either direction, so a wrong answer here has rows the truth does not.
    """
    verdicts = _verdicts(caseload)
    rows = []
    for household, names in _by_household(caseload).items():
        qualified = sum(1 for name in names if not verdicts[name][0] and not verdicts[name][1])
        pending = sum(1 for name in names if verdicts[name][1] and not verdicts[name][0])
        if pending > qualified:
            rows.append(household)
    return Answer.of(*rows)


def blank_column(column: str, caseload: Caseload = PINNED) -> set[str]:
    """Applicants with no value in one column — the set `undetermined` is *not*.

    Not a question; the invariant `tasks.check` asserts with it is that the two
    sets differ, because an arm that answers `undetermined` by listing the blanks
    must score wrong.
    """
    index = {"age": 1, "income": 2, "residency": 3}[column]
    return {record[0] for record in caseload.applicants if record[index] is None}
