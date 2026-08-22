"""The ontology oracle, checked independently.

Two pieces of real algorithm live in `ontology/truth.py`: the upward closure, and
"the nearest declaration wins". Each gets a second formulation rather than a
restatement — two spellings of one search would agree on the same bug.
"""

from hypothesis import given
from hypothesis import strategies as st

from harness.domains.ontology import fixture, truth


def fixpoint_ancestors(classes, subclass):
    """The same closure by iterating to a fixpoint instead of by worklist."""
    seen = set(classes)
    changed = True
    while changed:
        changed = False
        for child, parent in subclass:
            if child in seen and parent not in seen:
                seen.add(parent)
                changed = True
    return seen


def most_specific_candidate(instance, prop):
    """ "Nearest declaration" as *the bottom of a chain* rather than as override.

    `truth.effective_property` keeps the candidates nothing sits strictly below;
    this keeps the one that sits strictly below everything else. The two coincide
    exactly when the candidates form a chain — which is the condition that makes
    the question answerable at all, so this checks the semantics and the
    fixture's well-posedness in one move.

    A shortest-path reading was tried here first and is *unsound*: `i12` is a
    `computer` and `managed`, which puts `device` and `thing` one hop away each,
    and distance calls that a tie where subsumption does not.
    """
    declaring = {c: v for c, p, v in fixture.DECLARES if p == prop}
    kinds = truth.kinds_of(instance)
    candidates = {c for c in declaring if c in kinds}
    bottom = [
        c
        for c in candidates
        if all(other == c or truth.strictly_below(c, other) for other in candidates)
    ]
    assert len(bottom) == 1, f"{instance}'s candidates for {prop} are not a chain: {candidates}"
    return {declaring[bottom[0]]}


_CLASS = st.sampled_from(["a", "b", "c", "d", "e", "f"])


@given(
    classes=st.sets(_CLASS, max_size=4),
    subclass=st.sets(st.tuples(_CLASS, _CLASS), max_size=12),
)
def test_ancestors_agrees_with_an_independent_formulation(classes, subclass):
    assert truth.ancestors(classes, tuple(subclass)) == fixpoint_ancestors(classes, subclass)


@given(subclass=st.sets(st.tuples(_CLASS, _CLASS), max_size=12))
def test_ancestors_is_idempotent(subclass):
    edges = tuple(subclass)
    once = truth.ancestors({"a"}, edges)
    assert truth.ancestors(once, edges) == once


def test_a_cycle_in_the_hierarchy_terminates():
    # Ontologies acquire cycles by accident, and a closure that revisits would
    # hang the whole run rather than fail one cell.
    cyclic = (("a", "b"), ("b", "c"), ("c", "a"))
    assert truth.ancestors({"a"}, cyclic) == {"a", "b", "c"}


def test_a_class_reached_by_two_routes_is_one_ancestor():
    # `laptop` is a `computer` and `portable`, both of which are `asset`s.
    kinds = truth.ancestors({"laptop"})
    assert {"computer", "portable", "asset", "thing"} <= kinds
    assert len(kinds) == len(set(kinds))


def test_every_instance_has_exactly_one_effective_power_profile():
    # Well-posedness of the task, not a property of the code: two unrelated
    # declaring classes would give an instance two values and make the question
    # unanswerable as asked.
    rows = truth.effective_property("power_profile").rows
    instances = [instance for instance, _ in rows]
    assert sorted(instances) == sorted(fixture.INSTANCES)
    assert len(instances) == len(set(instances))


def test_the_effective_value_agrees_with_a_most_specific_reading():
    for instance in fixture.INSTANCES:
        by_override = {
            value
            for member, value in truth.effective_property("power_profile").rows
            if member == instance
        }
        assert by_override == most_specific_candidate(instance, "power_profile"), instance


def test_the_root_default_reaches_instances_no_other_declaration_covers():
    # The quiet half of inheritance: an instance under `licence` matches no
    # `power_profile` declaration but the one on `thing`, and an arm that only
    # looks at the near classes drops it.
    rows = dict(truth.effective_property("power_profile").rows)
    inherited_from_root = [i for i, value in rows.items() if value == "unknown"]
    assert inherited_from_root


def test_inconsistency_is_found_above_the_declared_classes():
    flagged = {instance for (instance,) in truth.inconsistent_instances().rows}
    assert flagged
    declared_pairs = set(fixture.DISJOINT) | {(b, a) for a, b in fixture.DISJOINT}
    for instance in flagged:
        classes = truth.classes_of(instance)
        direct = {(a, b) for a in classes for b in classes if (a, b) in declared_pairs}
        assert not direct, f"{instance}'s conflict is visible without the closure"


def test_a_consistent_instance_is_not_flagged():
    flagged = {instance for (instance,) in truth.inconsistent_instances().rows}
    assert flagged != set(fixture.INSTANCES)
    for instance in set(fixture.INSTANCES) - flagged:
        kinds = truth.kinds_of(instance)
        assert not any(a in kinds and b in kinds for a, b in fixture.DISJOINT)


def test_an_empty_class_has_nothing_beneath_it_either():
    empty = {name for (name,) in truth.classes_with_no_instances().rows}
    assert empty
    for instance in fixture.INSTANCES:
        assert not (truth.kinds_of(instance) & empty)
