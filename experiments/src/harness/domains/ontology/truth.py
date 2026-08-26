"""Ground truth, in plain Python. Control 1: never the engine being measured.

The closure is written the boring way — a worklist over the declared edges —
because nothing else checks it. What it must get right that a chain-following
reading does not: an ancestor reached by two routes is one ancestor, and the
*nearest* declaration is the one no other candidate sits strictly below.

Every function takes the ``Ontology`` it is asked about, defaulting to the
pinned one. That is what lets the oracle be checked against a second formulation
on a *generated* hierarchy: an oracle that can only run against the single
hierarchy it was written for can only ever be checked against that hierarchy's
own answers, which is how a fixture comes to be tuned to its truth.

**Shape, not cleverness, is what makes it fast enough.** Instances share class
sets: thousands of them over a few dozen classes have a few dozen distinct sets
between them, so the closure and the property it implies are computed per class
set and looked up per instance.
"""

from __future__ import annotations

from collections import defaultdict, deque
from functools import lru_cache

from harness.domains.ontology.fixture import ASKED, PINNED, Ontology
from harness.task import Answer

#: The pinned hierarchy's relations, under the names the four pinned questions
#: and the reference corpus are written against.
CLASSES = PINNED.classes
SUBCLASS = PINNED.subclass
DECLARES = PINNED.declares
DISJOINT = PINNED.disjoint
INSTANCE_OF = PINNED.instance_of
INSTANCES = PINNED.instances


def ancestors(classes: set[str], subclass: tuple[tuple[str, str], ...] = SUBCLASS) -> set[str]:
    """Every class reachable upward, the given classes included.

    ``subclass`` is a parameter so the closure can be checked against an
    independent formulation on generated hierarchies — this is the one piece of
    real algorithm in the file.
    """
    seen = set(classes)
    queue = deque(classes)
    while queue:
        current = queue.popleft()
        for child, parent in subclass:
            if child == current and parent not in seen:
                seen.add(parent)
                queue.append(parent)
    return seen


def strictly_below(
    lower: str, upper: str, subclass: tuple[tuple[str, str], ...] = SUBCLASS
) -> bool:
    """Is ``lower`` a strict subclass of ``upper``?"""
    return lower != upper and upper in ancestors({lower}, subclass)


@lru_cache(maxsize=8)
def _declared_classes(ontology: Ontology) -> dict[str, frozenset[str]]:
    """Each instance's directly declared classes.

    ``Ontology`` is frozen with tuple fields, so it hashes; a different
    hierarchy is a different cache key.
    """
    grouped: dict[str, set[str]] = defaultdict(set)
    for instance, name in ontology.instance_of:
        grouped[instance].add(name)
    return {instance: frozenset(grouped[instance]) for instance in ontology.instances}


@lru_cache(maxsize=8)
def _kinds(ontology: Ontology) -> dict[frozenset[str], frozenset[str]]:
    """Class set -> its upward closure, computed once per distinct set."""
    return {
        declared: frozenset(ancestors(set(declared), ontology.subclass))
        for declared in set(_declared_classes(ontology).values())
    }


def classes_of(instance: str, ontology: Ontology = PINNED) -> set[str]:
    return set(_declared_classes(ontology)[instance])


def kinds_of(instance: str, ontology: Ontology = PINNED) -> set[str]:
    """The instance's classes and everything above them."""
    return set(_kinds(ontology)[_declared_classes(ontology)[instance]])


def instances_of(class_name: str, ontology: Ontology = PINNED) -> Answer:
    kinds = _kinds(ontology)
    declared = _declared_classes(ontology)
    return Answer.of(
        *[instance for instance in ontology.instances if class_name in kinds[declared[instance]]]
    )


def instances_of_but_not(class_name: str, other: str, ontology: Ontology = PINNED) -> Answer:
    """The `at-scale` shape of the reachability question: the fact base stays
    large and the answer gets small, which is the combination that track is
    about."""
    kinds = _kinds(ontology)
    declared = _declared_classes(ontology)
    return Answer.of(
        *[
            instance
            for instance in ontology.instances
            if class_name in kinds[declared[instance]] and other not in kinds[declared[instance]]
        ]
    )


def instances_per_class(ontology: Ontology = PINNED) -> dict[str, int]:
    """How many instances each class holds, transitively, in one pass.

    Asking `instances_of` per candidate is the obvious spelling and is
    quadratic; picking a question's subject that way is what stopped an
    `at-scale` slate building in `access_control`.
    """
    counts: dict[str, int] = dict.fromkeys(ontology.classes, 0)
    kinds = _kinds(ontology)
    for declared in _declared_classes(ontology).values():
        for name in kinds[declared]:
            counts[name] += 1
    return counts


@lru_cache(maxsize=8)
def _nearest(ontology: Ontology, prop: str) -> dict[frozenset[str], tuple[str, ...]]:
    """Class set -> the values the nearest declaring ancestors carry.

    A declaration is *overridden* when another declaring class sits strictly
    below it and also applies — which is what "most specific wins" means once
    the hierarchy stops being a chain.
    """
    declaring = {name for name, declared, _ in ontology.declares if declared == prop}
    value_of = {name: value for name, declared, value in ontology.declares if declared == prop}
    result = {}
    for declared, kinds in _kinds(ontology).items():
        candidates = declaring & kinds
        nearest = {
            name
            for name in candidates
            if not any(
                other != name and strictly_below(other, name, ontology.subclass)
                for other in candidates
            )
        }
        result[declared] = tuple(sorted(value_of[name] for name in nearest))
    return result


def effective_property(prop: str, ontology: Ontology = PINNED) -> Answer:
    """``(instance, value)`` for the nearest declaring ancestor of each instance."""
    nearest = _nearest(ontology, prop)
    declared = _declared_classes(ontology)
    rows = []
    for instance in ontology.instances:
        for value in nearest[declared[instance]]:
            rows.append((instance, value))
    return Answer.of(*rows)


def effective_property_within(class_name: str, prop: str, ontology: Ontology = PINNED) -> Answer:
    """The same, scoped to one class. The `at-scale` form: a value per instance
    over 12,600 instances is a transcription exercise."""
    nearest = _nearest(ontology, prop)
    declared = _declared_classes(ontology)
    kinds = _kinds(ontology)
    rows = []
    for instance in ontology.instances:
        if class_name in kinds[declared[instance]]:
            for value in nearest[declared[instance]]:
                rows.append((instance, value))
    return Answer.of(*rows)


def dead_declarations(prop: str = ASKED, ontology: Ontology = PINNED) -> Answer:
    """Declared values that are no instance's effective value.

    The question the pinned slate cannot ask. Every wrong answer to the four
    pinned questions is a **subset** of the right one, so their shape cannot say
    which mistake was made. This one is a negation over a derived relation: an
    arm that stops the override walk one class early reports a value as live
    that is dead, and one that overshoots reports a live value as dead — the
    answer moves in both directions, so its shape says which.
    """
    live = {value for _, value in effective_property(prop, ontology).rows}
    return Answer.of(
        *{
            value
            for _, declared, value in ontology.declares
            if declared == prop and value not in live
        }
    )


def inconsistent_instances(ontology: Ontology = PINNED) -> Answer:
    """Instances that land in two classes declared disjoint.

    Neither class has to be declared on the instance: the conflict is usually
    several levels up, which is why it survives a reading of the fact base.
    """
    kinds = _kinds(ontology)
    declared = _declared_classes(ontology)
    found = []
    for instance in ontology.instances:
        reached = kinds[declared[instance]]
        if any(left in reached and right in reached for left, right in ontology.disjoint):
            found.append(instance)
    return Answer.of(*found)


def classes_with_no_instances(ontology: Ontology = PINNED) -> Answer:
    counts = instances_per_class(ontology)
    return Answer.of(*[name for name in ontology.classes if not counts[name]])
