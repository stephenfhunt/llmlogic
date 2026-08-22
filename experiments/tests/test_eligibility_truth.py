"""The eligibility oracle, and the two distinctions it exists to keep.

Both are easy to state and easy to lose in code: an unrecorded income fails
nothing, and "failed one" is a count rather than a search. Each is checked
against a formulation that does not share the implementation's shape.
"""

import pytest

from harness.domains.eligibility import fixture, truth


def evaluated_independently(applicant):
    """Every criterion re-read off the raw row, as three-valued logic.

    ``True`` met, ``False`` failed, ``None`` unknown — spelled out rather than
    folded into a set of failures, which is how `truth.failures` does it.
    """
    _, age, income, residency, _ = truth.row(applicant)
    return {
        "age": age >= truth.MIN_AGE,
        "income": None if income is None else income < truth.MAX_INCOME,
        "residency": residency >= truth.MIN_RESIDENCY_YEARS,
        "sanction": not any(name == applicant for name, _ in fixture.SANCTION_ROWS),
    }


def test_eligibility_agrees_with_a_three_valued_reading():
    eligible = {name for (name,) in truth.eligible().rows}
    for name, *_ in fixture.APPLICANT_ROWS:
        verdicts = evaluated_independently(name)
        assert (name in eligible) is all(verdict is True for verdict in verdicts.values())


def test_failures_and_verdicts_never_disagree():
    for name, *_ in fixture.APPLICANT_ROWS:
        failed = truth.failures(name)
        for criterion, verdict in evaluated_independently(name).items():
            assert (criterion in failed) is (verdict is False), (name, criterion)


def test_an_unrecorded_income_is_neither_a_pass_nor_a_failure():
    for name in fixture.NO_INCOME:
        assert "income" not in truth.failures(name)
        assert name not in {applicant for (applicant,) in truth.eligible().rows}


def test_undetermined_is_a_strict_subset_of_the_blank_incomes():
    # The whole point of the question: an arm that lists the blanks is wrong, and
    # only wrong by one row, which is the kind of wrong that survives review.
    blanks = set(fixture.NO_INCOME)
    undetermined = {name for (name,) in truth.undetermined().rows}
    assert undetermined
    assert undetermined < blanks


def test_undetermined_applicants_meet_every_other_criterion():
    for (name,) in truth.undetermined().rows:
        assert truth.failures(name) == set()


def test_blocked_by_one_names_the_criterion_that_is_actually_failed():
    for name, criterion in truth.blocked_by_exactly_one().rows:
        assert truth.failures(name) == {criterion}
        assert truth.income_known(name)


def test_every_criterion_is_the_sole_reason_for_someone():
    # Coverage of the slate itself: a criterion that never decides anything is a
    # rule the questions do not test, and it would go unnoticed.
    named = {criterion for _, criterion in truth.blocked_by_exactly_one().rows}
    assert named == set(truth.CRITERIA)


def test_someone_fails_more_than_one_criterion_and_is_left_out():
    multi = [
        name
        for name, *_ in fixture.APPLICANT_ROWS
        if truth.income_known(name) and len(truth.failures(name)) > 1
    ]
    assert multi
    reported = {name for name, _ in truth.blocked_by_exactly_one().rows}
    assert not (set(multi) & reported)


def test_household_counts_cover_every_dependant_exactly_once():
    assert sum(truth.dependants_per_household().values()) == len(fixture.DEPENDANT_ROWS)


@pytest.mark.parametrize("threshold", range(0, 15))
def test_households_over_a_threshold_shrink_as_it_rises(threshold):
    wider = {household for (household,) in truth.households_over_dependants(threshold).rows}
    tighter = {household for (household,) in truth.households_over_dependants(threshold + 1).rows}
    assert tighter <= wider


def test_some_households_clear_the_threshold_and_some_do_not():
    over = {household for (household,) in truth.households_over_dependants().rows}
    assert over and over != set(fixture.HOUSEHOLDS)
