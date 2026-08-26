"""The generated hierarchies: is the closure right, does the override rule hold?

Hand-checking 28 tasks proved the oracle matched the *author's* reading, and it
could not prove there was only one (`decisions.md` 2026-08-24). Generation
multiplies that by the number of items, so the checks are mechanical here and run
over many seeds rather than one.

The invariant this pack lives or dies on is *every instance gets exactly one
value* — the questions say so. It is true because of **where the declarations
are put**, on a single root-to-leaf spine, not because of anything the closure
does; a random DAG with declarations sprinkled over it breaks it constantly. So
it is asserted on every seed and every difficulty.
"""

from __future__ import annotations

from collections import Counter

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.domains.ontology import fixture, truth
from harness.domains.ontology.tasks import check, generated
from harness.generate import Degenerate, validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = sorted(fixture.DIFFICULTY)


def _fixpoint_ancestors(classes: set[str], subclass) -> set[str]:
    """The closure again, as a fixpoint rather than a breadth-first walk.

    Control 1 says the truth never comes from the engine. It does not say the
    truth is above being checked: this is a second, independent formulation, and
    the closure is the one piece of real algorithm in the oracle.
    """
    seen = set(classes)
    changed = True
    while changed:
        changed = False
        for child, parent in subclass:
            if child in seen and parent not in seen:
                seen.add(parent)
                changed = True
    return seen


def _by_depth(instance: str, prop: str, ontology: fixture.Ontology) -> set[str]:
    """The effective value, worked out by **depth from the root** instead of by
    the not-below-anything rule the oracle uses.

    A second formulation of *most specific wins*: among the declaring classes
    that apply, the one furthest from the root wins. It agrees with the oracle
    exactly when the applicable declarations form a chain — which is the
    property the spine is there to guarantee, so a disagreement here is the
    generator having broken it, not the oracle being wrong.
    """
    kinds = truth.kinds_of(instance, ontology)
    applicable = [
        (name, value)
        for name, declared, value in ontology.declares
        if declared == prop and name in kinds
    ]
    if not applicable:
        return set()

    def depth(name: str) -> int:
        return len(truth.ancestors({name}, ontology.subclass))

    deepest = max(depth(name) for name, _ in applicable)
    return {value for name, value in applicable if depth(name) == deepest}


class TestOracle:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_the_closure_agrees_with_a_fixpoint(self, seed):
        ontology = fixture.generate(seed, 3)
        for name in ontology.classes:
            assert truth.ancestors({name}, ontology.subclass) == _fixpoint_ancestors(
                {name}, ontology.subclass
            )

    @pytest.mark.parametrize("seed", SEEDS)
    def test_the_override_rule_agrees_with_a_depth_ordering(self, seed):
        ontology = fixture.generate(seed, 3)
        effective = {
            instance: value
            for instance, value in truth.effective_property(fixture.ASKED, ontology).rows
        }
        for instance in ontology.instances:
            assert {effective[instance]} == _by_depth(instance, fixture.ASKED, ontology)

    @pytest.mark.parametrize("seed", SEEDS[:4])
    def test_the_per_class_counts_agree_with_the_unindexed_definition(self, seed):
        """The fast shape has to compute what the slow, obviously-correct one
        does. The slow spelling is quadratic, which is why it is not what ships —
        and exactly why it is worth keeping as the check."""
        ontology = fixture.generate(seed, 2)
        counts = truth.instances_per_class(ontology)
        for name in ontology.classes:
            assert counts[name] == len(truth.instances_of(name, ontology).rows)

    @pytest.mark.parametrize("seed", SEEDS[:4])
    def test_a_dead_value_is_nobody_s_value(self, seed):
        ontology = fixture.generate(seed, 3)
        live = {value for _, value in truth.effective_property(fixture.ASKED, ontology).rows}
        for (value,) in truth.dead_declarations(fixture.ASKED, ontology).rows:
            assert value not in live


class TestGeneratedItems:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_generated_item_is_worth_asking(self, seed, difficulty):
        rejected = 0
        for task in generated(seed, difficulty):
            try:
                validate(task)
                check(task)
            except Degenerate:
                rejected += 1
        assert rejected == 0

    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_instance_gets_exactly_one_value(self, seed, difficulty):
        """The question says so. It holds because the declarations sit on one
        root-to-leaf spine — put two of them on unrelated classes and an
        instance carrying both is under two nearest declarations at once."""
        ontology = fixture.generate(seed, difficulty)
        counts = Counter(
            instance for instance, _ in truth.effective_property(fixture.ASKED, ontology).rows
        )
        assert set(counts) == set(ontology.instances)
        assert set(counts.values()) == {1}

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_no_disjoint_pair_makes_a_class_inconsistent_by_construction(self, seed):
        """A pair sharing a descendant would make every instance of that class
        an error, which is a contradiction in the *ontology* — a different
        finding from the mislabelled instance the question asks about."""
        ontology = fixture.generate(seed, 3)
        for left, right in ontology.disjoint:
            below_left = {
                name
                for name in ontology.classes
                if left in truth.ancestors({name}, ontology.subclass)
            }
            below_right = {
                name
                for name in ontology.classes
                if right in truth.ancestors({name}, ontology.subclass)
            }
            assert not below_left & below_right

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_distractor_property_is_never_the_answer(self, seed):
        """`owner_team` is declared on unrelated classes on purpose. An arm that
        ignores the property column answers with it, and the two answers have to
        actually differ for that to be a mistake the grader can see."""
        ontology = fixture.generate(seed, 3)
        asked = truth.effective_property(fixture.ASKED, ontology)
        distractor = truth.effective_property(fixture.DISTRACTOR, ontology)
        assert asked.rows != distractor.rows

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_difficulty_deepens_the_hierarchy(self, seed):
        shallow = fixture.generate(seed, 1)
        deep = fixture.generate(seed, 5)
        assert len(deep.classes) > len(shallow.classes)
        assert len(deep.instances) > len(shallow.instances)
        depth = lambda o: max(  # noqa: E731
            len(truth.ancestors({name}, o.subclass)) for name in o.classes
        )
        assert depth(deep) > depth(shallow)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_hierarchy(self, seed):
        assert fixture.generate(seed, 3) == fixture.generate(seed, 3)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        one = set(fixture.generate(seed, 3).classes)
        other = set(fixture.generate(seed + 1, 3).classes)
        assert not (one & other)

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_an_in_context_fixture_stays_inside_the_budget(self, difficulty):
        task = generated(20260825, difficulty)[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET
        assert task.track == "in-context"

    def test_an_at_scale_fixture_actually_exceeds_it(self):
        task = generated(20260825, 1, "at-scale")[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 > FIXTURE_TOKEN_BUDGET
        assert task.track == "at-scale"

    def test_the_rare_class_holds_a_fixed_handful_at_any_scale(self):
        """What the `at-scale` questions are scoped to. A *share* of 36,000
        instances is an answer nobody can write — the lesson `access_control`
        records after a proportional isolated set produced 1,920 rows."""
        for track, difficulty in (("in-context", 3), ("at-scale", 1), ("at-scale", 3)):
            ontology = fixture.generate(20260825, difficulty, track)
            counts = truth.instances_per_class(ontology)
            assert counts[ontology.rare_class] == fixture.RARE_INSTANCES

    def test_at_scale_refuses_a_difficulty_it_cannot_carry(self):
        with pytest.raises(ValueError, match="at-scale takes difficulty"):
            fixture.generate(20260825, 5, "at-scale")

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)

    def test_the_generated_slate_asks_something_the_pinned_one_cannot(self):
        ids = {task.id for task in generated(20260825, 3)}
        assert any(task_id.endswith("dead-declarations") for task_id in ids)


class TestCheck:
    def test_two_values_for_one_instance_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        task = next(t for t in generated(20260825, 3) if t.id.endswith(fixture.ASKED))
        doubled = Answer.of(*task.truth.rows, ("o27d900000", "a-second-value"))
        with pytest.raises(Degenerate, match="two"):
            check(replace(task, truth=doubled))

    def test_an_unreported_instance_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        task = next(t for t in generated(20260825, 3) if t.id.endswith(fixture.ASKED))
        thinned = Answer.of(*sorted(task.truth.rows)[1:])
        with pytest.raises(Degenerate, match="report every one"):
            check(replace(task, truth=thinned))
