"""Applicants, their dependants and their sanctions, from a fixed seed.

Small — sixteen applicants — because the questions are about applying four
criteria without dropping a case, not about volume. A fact base big enough to be
tedious would measure patience.
"""

from __future__ import annotations

import random

from harness.task import Fixture

SEED = 20260822

APPLICANTS = tuple(f"a{i:02d}" for i in range(1, 17))
HOUSEHOLDS = tuple(f"h{i:02d}" for i in range(1, 7))

#: Applicants with no income recorded. Not a gap in the generator — the case the
#: `undetermined` question exists for, and the one that decides whether a blank
#: is being read as a zero.
NO_INCOME = ("a04", "a09", "a13")

SANCTION_REASONS = ("missed_review", "unreported_change", "duplicate_claim")

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


def _build():
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
    return tuple(applicants), tuple((d, a) for d, a, _ in dependants), sanctions


APPLICANT_ROWS, DEPENDANT_ROWS, SANCTION_ROWS = _build()


def build() -> Fixture:
    applicant_csv = "id,age,income,residency_years,household\n" + "".join(
        f"{name},{age},{'' if income is None else income},{residency},{household}\n"
        for name, age, income, residency, household in APPLICANT_ROWS
    )
    dependant_csv = "id,applicant\n" + "".join(
        f"{dependant},{applicant}\n" for dependant, applicant in DEPENDANT_ROWS
    )
    sanction_csv = "applicant,reason\n" + "".join(
        f"{applicant},{reason}\n" for applicant, reason in SANCTION_ROWS
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
