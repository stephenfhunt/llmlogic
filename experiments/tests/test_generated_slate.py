"""What must hold of *any* generated slate, whichever pack produced it.

`tests/test_controls_hold.py` asserts these over the 28 pinned tasks, where they
were also checked by a person reading them. A generated slate is read by nobody,
so the same invariants are asserted over every pack that has a generator — and
the set of packs is discovered, not listed, so a new generator is covered the day
it lands rather than the day someone remembers to add it here.
"""

from __future__ import annotations

import pytest

from harness import domains
from harness.cell import FIXTURE_TOKEN_BUDGET

SEEDS = [20260825, 7919, 31337]
PACKS = domains.generators()


def test_the_slate_has_at_least_one_generator():
    """A guard on the discovery above: an empty list would make every test below
    vacuously pass, which is how a suite comes to assert nothing."""
    assert PACKS


def test_a_pack_without_a_generator_says_why():
    with pytest.raises(ValueError, match="nothing to seed"):
        domains.generate("static_analysis", 20260825)


@pytest.mark.parametrize("name", PACKS)
@pytest.mark.parametrize("seed", SEEDS)
def test_every_generated_item_passes_its_own_validation(name, seed):
    for task in domains.generate(name, seed, 3):
        domains.validate(name, task)


@pytest.mark.parametrize("name", PACKS)
@pytest.mark.parametrize("seed", SEEDS)
def test_truth_matches_the_declared_shape(name, seed):
    for task in domains.generate(name, seed, 3):
        arity = len(task.answer_shape)
        for row in task.truth.rows:
            assert len(row) == arity, f"{task.key} declares {task.answer_shape}, truth has {row}"


@pytest.mark.parametrize("name", PACKS)
@pytest.mark.parametrize("seed", SEEDS)
def test_an_in_context_fixture_stays_inside_the_budget(name, seed):
    """The cap is the definition of that track (`decisions.md` 2026-08-25)."""
    for task in domains.generate(name, seed, 3):
        assert task.track == "in-context"
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET, f"{task.key} is ~{chars // 4} tokens"


@pytest.mark.parametrize("name", PACKS)
def test_the_generator_records_what_produced_the_item(name):
    """`difficulty` is 0 for a hand-authored task, so a generated one that leaves
    it there cannot say what a calibration pass selected."""
    for difficulty in (1, 3):
        for task in domains.generate(name, 20260825, difficulty):
            assert task.difficulty == difficulty
