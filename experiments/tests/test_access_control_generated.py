"""The generated slate: is it valid, is it hard, and does the oracle still hold?

Generation multiplies the `scheduling` risk by the number of items — hand-checking
28 tasks proved the oracle matched the *author's* reading, and it could not prove
there was only one. So the checks that were done by eye for the pinned slate are
mechanical here, and they run over many seeds rather than one.
"""

from __future__ import annotations

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.domains.access_control import fixture, truth
from harness.domains.access_control.tasks import generated
from harness.generate import Degenerate, validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = sorted(fixture.DIFFICULTY)


def _fixpoint_closure(roles: set[str], includes) -> set[str]:
    """The closure again, as a fixpoint rather than a breadth-first walk.

    Control 1 says the truth never comes from the engine. It does not say the
    truth is above being checked: this is a second, independent formulation, and
    the closure is the one piece of real algorithm in the oracle.
    """
    seen = set(roles)
    changed = True
    while changed:
        changed = False
        for holder, included in includes:
            if holder in seen and included not in seen:
                seen.add(included)
                changed = True
    return seen


class TestOracle:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_the_closure_agrees_with_a_fixpoint_on_a_generated_graph(self, seed):
        policy = fixture.generate(seed, 3)
        for role in {r for r, _ in policy.grants}:
            assert truth.closure({role}, policy.includes) == _fixpoint_closure(
                {role}, policy.includes
            )

    @pytest.mark.parametrize("seed", SEEDS[:4])
    def test_the_index_agrees_with_the_unindexed_definition(self, seed):
        """The fast shape has to compute what the slow, obviously-correct one does.
        The slow spelling is quadratic, which is why it is not what ships — and
        exactly why it is worth keeping as the check."""
        policy = fixture.generate(seed, 2)
        for user in policy.users:
            roles = truth.effective_roles(user, policy)
            revoked = {res for holder, res in policy.revoked if holder == user}
            expected = {
                (res, action)
                for role, res, action in policy.permits
                if role in roles and res not in revoked
            }
            assert truth.permissions(user, policy) == expected

    @pytest.mark.parametrize("seed", SEEDS[:4])
    def test_no_user_with_no_access_appears_in_any_answer(self, seed):
        policy = fixture.generate(seed, 3)
        isolated = set(truth.users_with_no_access(policy).rows)
        reachable = {(user,) for user in policy.users if truth.permissions(user, policy)}
        assert not (isolated & reachable)


class TestGeneratedItems:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_almost_every_generated_item_is_worth_asking(self, seed, difficulty):
        """`validate` refuses; it does not silently filter. A generator that drops
        a third of its items is one whose difficulty setting no longer means what
        it says, so the rejection rate is watched rather than absorbed."""
        rejected = 0
        for task in generated(seed, difficulty):
            try:
                validate(task)
            except Degenerate:
                rejected += 1
        assert rejected <= 1

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_difficulty_deepens_the_hierarchy(self, seed):
        shallow = fixture.generate(seed, 1)
        deep = fixture.generate(seed, 5)
        assert len(deep.includes) > len(shallow.includes)
        assert len(deep.users) > len(shallow.users)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_graph(self, seed):
        """Two runs at one seed are the same items, or `results/` cannot be
        compared across runs at all."""
        assert fixture.generate(seed, 3) == fixture.generate(seed, 3)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        """Anti-memorisation: nothing here should be answerable from having seen
        it. Identifiers are regenerated per seed."""
        one = set(fixture.generate(seed, 3).users)
        other = set(fixture.generate(seed + 1, 3).users)
        assert not (one & other)

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_an_in_context_fixture_stays_inside_the_budget(self, difficulty):
        """The cap is the definition of that track."""
        task = generated(20260825, difficulty)[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET
        assert task.track == "in-context"

    def test_an_at_scale_fixture_actually_exceeds_it(self):
        """Otherwise it is an `in-context` item wearing the wrong label — the first
        multiplier tried produced a 20k-token file both arms could still read."""
        task = generated(20260825, 1, "at-scale")[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 > FIXTURE_TOKEN_BUDGET
        assert task.track == "at-scale"

    def test_at_scale_refuses_a_difficulty_it_cannot_carry(self):
        with pytest.raises(ValueError, match="at-scale takes difficulty"):
            fixture.generate(20260825, 5, "at-scale")

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)

    def test_the_generated_slate_asks_something_the_pinned_one_cannot(self):
        """Every wrong answer to the four pinned questions is a subset, so their
        shape cannot say *which* mistake was made. The closure-depth question is
        false for a shallow walk rather than short, so it can."""
        ids = {task.id.rsplit("-", maxsplit=1)[-1] for task in generated(20260825, 3)}
        assert "closure" in {part for task_id in ids for part in [task_id]}


class TestValidate:
    def _one(self):
        return generated(20260825, 3)[0]

    def test_an_empty_truth_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        with pytest.raises(Degenerate, match="empty truth"):
            validate(replace(self._one(), truth=Answer(frozenset())))

    def test_a_single_row_answer_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        with pytest.raises(Degenerate, match="guessable"):
            validate(replace(self._one(), truth=Answer.of(("only-one",))))
