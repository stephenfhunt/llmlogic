"""Applicants, their dependants and their sanctions.

Two entry points, answering different questions.

``build()`` returns the **pinned** caseload — sixteen applicants from one fixed
seed, small because the questions are about applying four criteria without
dropping a case, not about volume. A fact base big enough to be tedious would
measure patience.

``generate(seed, difficulty, track)`` returns a fresh one. Difficulty here is
**how many ways a value can be absent**, not how many rows there are: at the
bottom only income can be missing, and by the top three columns can, so
*undetermined* stops being a synonym for *blank income* and starts needing the
criteria worked through per applicant. The thresholds are **calibrated to the
drawn population** rather than fixed, because a threshold nobody is near is a
criterion that decides nothing.
"""

from __future__ import annotations

import random
from dataclasses import dataclass

from harness.task import Fixture

SEED = 20260822

APPLICANTS = tuple(f"a{i:02d}" for i in range(1, 17))
HOUSEHOLDS = tuple(f"h{i:02d}" for i in range(1, 7))

#: Applicants with no income recorded. Not a gap in the generator — the case the
#: `undetermined` question exists for, and the one that decides whether a blank
#: is being read as a zero.
NO_INCOME = ("a04", "a09", "a13")

SANCTION_REASONS = ("missed_review", "unreported_change", "duplicate_claim")

#: The pinned thresholds. A generated caseload carries its own, calibrated to
#: what it drew; these are the ones the four pinned questions state.
MIN_AGE = 21
MAX_INCOME = 32_000
MIN_RESIDENCY_YEARS = 3
#: A household clears this many dependants across all of its members.
DEPENDANT_THRESHOLD = 4

#: The criteria, in the order the questions state them. Three are comparisons
#: against a column that can be absent; the fourth is a row in another table.
CRITERIA = ("age", "income", "residency", "sanction")

#: Columns a value can go missing from, in the order difficulty admits them.
#: Income first because it is the pinned case, and because money is the field
#: real caseloads actually lack.
UNKNOWABLE = ("income", "residency", "age")

#: Values written over the seeded ones, each for a case the questions need and a
#: random draw did not supply. Planted rather than re-seeded because these are
#: *coverage*, not shape: with them, every criterion is the sole reason someone
#: is refused, and "no income recorded" is not the same set as "cannot be told".
#:
#: - ``a02`` is under age and clears everything else — without it the age
#:   criterion never decides anything and three of the four questions never test
#:   it.
#: - ``a13`` has no income *and* fails residency, so listing the blank incomes is
#:   not an answer to `undetermined`.
PLANTED: dict[str, dict[str, int]] = {
    "a02": {"age": 19, "residency_years": 6},
    "a13": {"residency_years": 1},
}

#: ``(applicants, households, unknowable columns, near-miss share, sanction share)``.
#:
#: The knob that matters is the third: at 1 an arm can answer *undetermined* by
#: listing the blank incomes and be nearly right, and at 3 it has to decide, per
#: applicant, whether an absent value is the *only* thing missing. The near-miss
#: share is what keeps `blocked-by-exactly-one` from restating `eligible` — an
#: applicant who fails two criteria is not an answer, and nothing about their row
#: says so.
DIFFICULTY: dict[int, tuple[int, int, int, float, float]] = {
    1: (16, 4, 1, 0.15, 0.25),  # the pinned shape's neighbourhood
    2: (40, 6, 1, 0.25, 0.30),
    3: (90, 12, 2, 0.30, 0.30),
    4: (160, 18, 2, 0.35, 0.35),
    5: (260, 26, 3, 0.40, 0.35),
}

#: The ``at-scale`` multiplier on the caseload. Sized so the fixture actually
#: exceeds `cell.FIXTURE_TOKEN_BUDGET`, which is the entire point of the track;
#: a test pins that it stays over.
AT_SCALE = 700

#: The highest difficulty the ``at-scale`` track accepts. Scale is the variable
#: on that track, and crossing it with structure produces an item about neither.
AT_SCALE_MAX_DIFFICULTY = 2

#: How many applicants per household at `at-scale`. Households grow with the
#: caseload so one household stays a readable slice: held flat they would each
#: hold thousands, and the questions this track narrows to a household would
#: answer with a list nobody can write.

#: Ceilings on the engineered groups. A **fixed handful, not a proportion** —
#: the lesson `access_control` records after a proportional isolated set put
#: 1,920 users in one answer. At `at-scale` a share of the caseload is thousands
#: of rows, and an answer nobody can transcribe measures transcription.
MAX_UNDECIDED = 60
MAX_NEAR_MISSES = 200
MAX_TRAPS = 40
MAX_PENDING_HOUSEHOLDS = 6
MAX_BUSY_HOUSEHOLDS = 12
AT_SCALE_HOUSEHOLD_SIZE = 100


@dataclass(frozen=True)
class Caseload:
    """One caseload, and the criteria it is judged against.

    A value rather than module globals, so the oracle can be handed a
    *generated* caseload and checked against a second formulation on it. With
    globals the oracle can only ever be checked against the one caseload it was
    written for, which is how a fixture comes to be tuned to its truth.

    An applicant is ``(id, age, income, residency_years, household)`` and any of
    the three numbers may be ``None`` — absent, which is not the same as small.
    """

    applicants: tuple[tuple[str, int | None, int | None, int | None, str], ...]
    dependants: tuple[tuple[str, str], ...]
    sanctions: tuple[tuple[str, str], ...]
    households: tuple[str, ...]
    min_age: int = MIN_AGE
    max_income: int = MAX_INCOME
    min_residency_years: int = MIN_RESIDENCY_YEARS
    dependant_threshold: int = DEPENDANT_THRESHOLD


def _build() -> Caseload:
    rng = random.Random(SEED)
    applicants = []
    for name in APPLICANTS:
        planted = PLANTED.get(name, {})
        applicants.append(
            (
                name,
                planted.get("age", rng.randint(17, 64)),
                None if name in NO_INCOME else rng.randrange(8_000, 48_000, 500),
                planted.get("residency_years", rng.randint(0, 12)),
                rng.choice(HOUSEHOLDS),
            )
        )

    dependants = []
    for name, *_ in applicants:
        for index in range(rng.randint(0, 3)):
            dependants.append((f"d{len(dependants) + 1:02d}", name, index))

    sanctions = tuple(
        sorted({(rng.choice(APPLICANTS), rng.choice(SANCTION_REASONS)) for _ in range(4)})
    )
    return Caseload(
        applicants=tuple(applicants),
        dependants=tuple((d, a) for d, a, _ in dependants),
        sanctions=sanctions,
        households=HOUSEHOLDS,
    )


PINNED = _build()

#: The pinned caseload's rows, kept as names because the pinned tasks and the
#: reference corpus are written against them.
APPLICANT_ROWS, DEPENDANT_ROWS, SANCTION_ROWS = (
    PINNED.applicants,
    PINNED.dependants,
    PINNED.sanctions,
)


def _quantile(values: list[int], fraction: float) -> int:
    """The value ``fraction`` of the way up a sorted sample.

    Thresholds are set from the population rather than fixed, because a
    criterion nobody is near decides nothing: fixed at 32,000 against a draw
    that happens to top out at 20,000, *income* stops being one of four criteria
    and the item quietly becomes a three-criterion question.
    """
    ranked = sorted(values)
    return ranked[min(len(ranked) - 1, int(len(ranked) * fraction))]


def _threshold_leaving(counts: list[int], wanted: int) -> int:
    """A threshold that leaves about ``wanted`` values strictly above it.

    A quantile is the obvious spelling and is wrong on a small population: at
    four households, *the 60th percentile* names one household or none, and
    `validate` rightly calls a one-row answer guessable. Scanning the distinct
    values for the split closest to what was asked is the same idea without the
    rounding, and it degrades honestly when the counts are all equal.
    """
    candidates = sorted({0, *counts})
    best = candidates[0]
    for candidate in candidates:
        above = sum(1 for count in counts if count > candidate)
        if abs(above - wanted) < abs(sum(1 for c in counts if c > best) - wanted):
            best = candidate
    return best


def _rehouse(applicant, household: str):
    """The same applicant in a different household."""
    return applicant[:4] + (household,)


def _blank(applicant, column: str):
    """The same applicant with one value removed. Absent, not zero."""
    name, age, income, residency, household = applicant
    if column == "age":
        return (name, None, income, residency, household)
    if column == "income":
        return (name, age, None, residency, household)
    return (name, age, income, None, household)


def generate(seed: int, difficulty: int = 3, track: str = "in-context") -> Caseload:
    """A fresh caseload.

    Identifiers are regenerated per seed, so nothing here can be answered from
    memory and two runs at the same difficulty are two samples rather than the
    same items twice.

    The population is drawn and then **repaired**: a near-miss is made to meet
    everything and then broken on exactly one criterion, and an undetermined
    applicant is made to meet everything and then has one value removed.
    Engineering the cases rather than hoping for them is what makes every
    question's answer non-empty at every seed — a draw produces "fails exactly
    one" only by luck, and `validate` would spend its time rejecting.
    """
    if difficulty not in DIFFICULTY:
        raise ValueError(f"difficulty must be one of {sorted(DIFFICULTY)}")
    if track == "at-scale" and difficulty > AT_SCALE_MAX_DIFFICULTY:
        raise ValueError(
            f"at-scale takes difficulty 1-{AT_SCALE_MAX_DIFFICULTY}: scale is the "
            "variable on that track, and crossing it with structure produces an "
            "item that is about neither"
        )
    n_applicants, n_households, unknowable, near_miss, sanction_share = DIFFICULTY[difficulty]
    if track == "at-scale":
        n_applicants *= AT_SCALE
        n_households = max(n_households, n_applicants // AT_SCALE_HOUSEHOLD_SIZE)

    rng = random.Random(seed)
    tag = f"{seed:x}"[-4:]
    names = tuple(f"a{tag}{i:05d}" for i in range(n_applicants))
    households = tuple(f"h{tag}{i:04d}" for i in range(n_households))
    columns = UNKNOWABLE[:unknowable]

    rows = {
        name: (
            name,
            rng.randint(17, 64),
            rng.randrange(8_000, 48_000, 500),
            rng.randint(0, 12),
            households[index % n_households],
        )
        for index, name in enumerate(names)
    }
    # Calibrated so each criterion refuses a minority — near the middle of its
    # range, where the criterion is doing work, rather than at an end where it is
    # either free or fatal.
    thresholds = {
        "min_age": _quantile([row[1] for row in rows.values()], 0.30),
        "max_income": _quantile([row[2] for row in rows.values()], 0.70),
        "min_residency_years": _quantile([row[3] for row in rows.values()], 0.30),
    }
    min_age = thresholds["min_age"]
    max_income = thresholds["max_income"]
    min_residency = thresholds["min_residency_years"]

    sanctioned = {name for name in names if rng.random() < sanction_share}
    pool = list(names)
    rng.shuffle(pool)

    def take(count: int) -> list[str]:
        return [pool.pop() for _ in range(min(count, len(pool)))]

    def repair(name: str) -> None:
        """Make this applicant meet everything, so one break is the whole story."""
        household = rows[name][4]
        rows[name] = (
            name,
            rng.randint(min_age, 64),
            rng.randrange(8_000, max(9_000, max_income), 500),
            rng.randint(min_residency, 12),
            household,
        )
        sanctioned.discard(name)

    def break_one(name: str, criterion: str) -> None:
        _, age, income, residency, household = rows[name]
        if criterion == "age":
            rows[name] = (name, rng.randint(17, min_age - 1), income, residency, household)
        elif criterion == "income":
            rows[name] = (name, age, rng.randrange(max_income, 60_000, 500), residency, household)
        elif criterion == "residency":
            rows[name] = (name, age, income, rng.randint(0, max(0, min_residency - 1)), household)
        else:
            sanctioned.add(name)

    # Near-misses: meet everything, then fail exactly one — cycling the criteria
    # so each of the four is the sole reason someone is refused. Without that,
    # `blocked-by-exactly-one` can come back naming three criteria out of four
    # and an arm that never checks the fourth scores full marks.
    near_misses = take(min(MAX_NEAR_MISSES, max(len(CRITERIA), int(n_applicants * near_miss))))
    for index, name in enumerate(near_misses):
        repair(name)
        break_one(name, CRITERIA[index % len(CRITERIA)])

    # A qualifying group, repaired and left alone. Drawn at random, almost nobody
    # clears four criteria at once — d1 produced a single eligible applicant,
    # which `validate` rightly calls guessable. Engineering the positives is the
    # same move as engineering the near-misses, and for the same reason: an
    # answer set of one carries almost nothing about dropped rows.
    qualifiers = take(max(3, int(n_applicants * 0.15)))
    for name in qualifiers:
        repair(name)

    # The undetermined: everything met, and one value simply absent. Cycled over
    # the columns difficulty admits, so at the top the answer is not the blanks
    # in any one column.
    pending = households[: max(2, min(MAX_PENDING_HOUSEHOLDS, n_households // 4))]
    ordinary = households[len(pending) :] or households
    undecided = take(min(MAX_UNDECIDED, max(3, 2 * len(pending), int(n_applicants * 0.08))))
    for index, name in enumerate(undecided):
        repair(name)
        rows[name] = _blank(rows[name], columns[index % len(columns)])

    # And the trap: an absent value *and* a failure elsewhere. Blank in the same
    # columns as the applicants above and yet not undetermined, so an arm that
    # answers `undetermined` by listing the blanks scores rows too many and looks
    # right. The break is always on a column that is still recorded, or the
    # failure would be invisible rather than merely unnoticed.
    traps = take(min(MAX_TRAPS, max(2, int(n_applicants * 0.05))))
    for index, name in enumerate(traps):
        repair(name)
        blanked = columns[index % len(columns)]
        break_one(name, "sanction" if blanked == "income" else "income")
        rows[name] = _blank(rows[name], blanked)

    # Households are assigned last, by the part each applicant plays, because two
    # of the questions are *about* households and a uniform sprinkling answers
    # them trivially. `pending` households hold no qualifier at all, so they are
    # the ones with more undecided members than qualifying ones; a third of the
    # undecided are placed among the ordinary households instead, or the
    # `at-scale` narrowing — *undetermined, and nobody in their household
    # qualifies* — would select every undetermined applicant and narrow nothing.
    concentrated = undecided[: max(2 * len(pending), 2 * len(undecided) // 3)]
    for index, name in enumerate(concentrated):
        rows[name] = _rehouse(rows[name], pending[index % len(pending)])
    for index, name in enumerate(traps + near_misses):
        rows[name] = _rehouse(
            rows[name], (pending + ordinary)[index % (len(pending) + len(ordinary))]
        )
    for index, name in enumerate(qualifiers + undecided[len(concentrated) :] + pool):
        rows[name] = _rehouse(rows[name], ordinary[index % len(ordinary)])

    applicants = tuple(rows[name] for name in names)
    dependants = []
    for name in names:
        for _ in range(rng.randint(0, 3)):
            dependants.append((f"d{tag}{len(dependants):06d}", name))
    sanctions = tuple(
        sorted((name, rng.choice(SANCTION_REASONS)) for name in names if name in sanctioned)
    )

    # The dependant threshold is calibrated last, against the counts actually
    # drawn: fixed, it names either every household or none.
    counts: dict[str, int] = dict.fromkeys(households, 0)
    household_of = {row[0]: row[4] for row in applicants}
    for _, applicant in dependants:
        counts[household_of[applicant]] += 1

    return Caseload(
        applicants=applicants,
        dependants=tuple(dependants),
        sanctions=sanctions,
        households=households,
        dependant_threshold=_threshold_leaving(
            list(counts.values()), max(2, min(MAX_BUSY_HOUSEHOLDS, n_households // 3))
        ),
        **thresholds,
    )


def to_fixture(caseload: Caseload) -> Fixture:
    """The three CSVs a workspace gets. One place, so a generated caseload and
    the pinned one are laid out identically and no cell is decided by layout."""

    def cell(value: int | None) -> str:
        return "" if value is None else str(value)

    applicant_csv = "id,age,income,residency_years,household\n" + "".join(
        f"{name},{cell(age)},{cell(income)},{cell(residency)},{household}\n"
        for name, age, income, residency, household in caseload.applicants
    )
    dependant_csv = "id,applicant\n" + "".join(
        f"{dependant},{applicant}\n" for dependant, applicant in caseload.dependants
    )
    sanction_csv = "applicant,reason\n" + "".join(
        f"{applicant},{reason}\n" for applicant, reason in caseload.sanctions
    )
    return Fixture(
        files={
            "applicant.csv": applicant_csv,
            "dependant.csv": dependant_csv,
            "sanction.csv": sanction_csv,
        },
        schemas={
            "applicant": ("id", "age", "income", "residency_years", "household"),
            "dependant": ("id", "applicant"),
            "sanction": ("applicant", "reason"),
        },
    )


def build() -> Fixture:
    return to_fixture(PINNED)
