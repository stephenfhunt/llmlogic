"""The generated slate: is every item askable, and is it still about derivations?

The pinned three were checked by eye. These are the same checks made mechanical
and run over many graphs, plus the one that is specific to this pack: an answer
here must be *smaller* than the extension it is about. A provenance question
whose answer is every reader, or every role a user holds, has quietly turned back
into the lookup the rest of the slate already asks.
"""

from __future__ import annotations

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.domains.access_control import truth as access
from harness.domains.provenance import fixture, truth
from harness.domains.provenance.tasks import check, generated
from harness.generate import validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = (1, 2, 3, 4, 5)
ACTION = fixture.ACTION


class TestGeneratedItems:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_generated_item_is_worth_asking(self, seed, difficulty):
        """No tolerance for a rejection here, unlike `access_control`: the subject
        resource is picked *from the resources that qualify*, so an item that
        `validate` refuses means no resource qualified and the pack should have
        said so itself."""
        for task in generated(seed, difficulty):
            validate(task)
            check(task)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_items(self, seed):
        one = {task.id: task.truth.rows for task in generated(seed, 3)}
        other = {task.id: task.truth.rows for task in generated(seed, 3)}
        assert one == other

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        one = set(fixture.generate(seed, 3).users)
        other = set(fixture.generate(seed + 1, 3).users)
        assert not (one & other)

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_the_fixture_stays_inside_the_budget(self, difficulty):
        task = generated(20260825, difficulty)[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET
        assert task.track == "in-context"

    def test_at_scale_is_refused_with_its_reason(self):
        with pytest.raises(ValueError, match="in-context pack"):
            fixture.generate(20260825, 1, "at-scale")

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)

    def test_the_three_questions_are_all_there_at_every_rung(self):
        for difficulty in DIFFICULTIES:
            items = generated(15838, difficulty)
            asked = {task.id.split(f"-d{difficulty}-", 1)[-1] for task in items}
            assert asked == {"critical-grant", "minimal-repair", "access-path"}

    def test_every_item_is_the_provenance_question_class(self):
        for task in generated(23757, 4):
            assert task.question_class == "provenance"


class TestItIsStillAboutTheDerivation:
    """The failure mode with no error message: an item that reads as a provenance
    question and grades as a lookup."""

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_not_every_grant_a_user_holds_is_critical(self, seed):
        policy = fixture.generate(seed, 4)
        sizes = truth.critical_grants_per_resource(ACTION, policy)
        resource = max(sorted(sizes), key=lambda name: sizes[name])
        critical = truth.critical_grants(resource, ACTION, policy).rows
        readers = {
            user for user in policy.users if (resource, ACTION) in access.permissions(user, policy)
        }
        # Every critical row is one of the readers, and the rows do not simply
        # re-list them: a reader with two routes contributes none.
        assert {user for user, _, _ in critical} <= readers
        assert len(critical) < len(readers) * len(policy.grants)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_a_repair_is_never_someone_who_already_has_the_access(self, seed):
        policy = fixture.generate(seed, 4)
        sizes = truth.repairs_per_resource(ACTION, policy)
        resource = max(sorted(sizes), key=lambda name: sizes[name])
        for user, group in truth.single_membership_repairs(resource, ACTION, policy).rows:
            assert (resource, ACTION) not in access.permissions(user, policy)
            assert (user, group) not in policy.members

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_roles_on_a_chain_are_fewer_than_the_roles_held(self, seed):
        policy = fixture.generate(seed, 4)
        sizes = truth.justifications_per_resource(ACTION, policy)
        resource = max(sorted(sizes), key=lambda name: sizes[name])
        on_path = truth.justifying_roles(resource, ACTION, policy).rows
        held = {
            (user, role) for user in policy.users for role in access.effective_roles(user, policy)
        }
        assert set(on_path) < held


class TestTheGraphTheseQuestionsAssume:
    def test_the_role_hierarchy_has_no_cycles(self):
        """`truth._roles_between` reads *on a chain* as two reachability walks, and
        that reading only coincides with the other one on an acyclic hierarchy.
        The generator builds a chain; this is what says it still does."""
        for seed in SEEDS[:6]:
            for difficulty in DIFFICULTIES:
                policy = fixture.generate(seed, difficulty)
                edges: dict[str, set[str]] = {}
                for holder, included in policy.includes:
                    edges.setdefault(holder, set()).add(included)
                for role in edges:
                    assert role not in truth._reachable_from(set(edges[role]), edges)

    def test_the_pinned_graph_is_deeper_than_access_controls(self):
        """The reason this pack pins its own: a two-level hierarchy answers the
        derivation question with one role."""
        from harness.domains.access_control import fixture as shallow

        assert len(fixture.PINNED.includes) > len(shallow.PINNED.includes)
