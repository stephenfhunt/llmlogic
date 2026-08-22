"""Ground truth, in plain Python. Control 1: never the engine being measured.

The closure is written the boring way — a worklist over the declared edges —
because nothing else checks it. What it must get right that a chain-following
reading does not: an ancestor reached by two routes is one ancestor, and the
*nearest* declaration is the one no other candidate sits strictly below.
"""

from __future__ import annotations

from collections import deque

from harness.domains.ontology.fixture import (
    CLASSES,
    DECLARES,
    DISJOINT,
    INSTANCE_OF,
    INSTANCES,
    SUBCLASS,
)
from harness.task import Answer


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


def strictly_below(lower: str, upper: str) -> bool:
    """Is ``lower`` a strict subclass of ``upper``?"""
    return lower != upper and upper in ancestors({lower})


def classes_of(instance: str) -> set[str]:
    return {name for member, name in INSTANCE_OF if member == instance}


def kinds_of(instance: str) -> set[str]:
    """The instance's classes and everything above them."""
    return ancestors(classes_of(instance))


def instances_of(class_name: str) -> Answer:
    return Answer.of(*[instance for instance in INSTANCES if class_name in kinds_of(instance)])


def effective_property(prop: str) -> Answer:
    """``(instance, value)`` for the nearest declaring ancestor of each instance.

    A declaration is *overridden* when another declaring class sits strictly
    below it and also applies — which is what "most specific wins" means once the
    hierarchy stops being a chain.
    """
    rows = []
    for instance in INSTANCES:
        kinds = kinds_of(instance)
        candidates = {name for name, declared, _ in DECLARES if declared == prop and name in kinds}
        nearest = {
            name
            for name in candidates
            if not any(other != name and strictly_below(other, name) for other in candidates)
        }
        for name in sorted(nearest):
            value = next(v for c, p, v in DECLARES if c == name and p == prop)
            rows.append((instance, value))
    return Answer.of(*rows)


def inconsistent_instances() -> Answer:
    """Instances that land in two classes declared disjoint.

    Neither class has to be declared on the instance: the conflict is usually
    several levels up, which is why it survives a reading of the fact base.
    """
    found = []
    for instance in INSTANCES:
        kinds = kinds_of(instance)
        if any(left in kinds and right in kinds for left, right in DISJOINT):
            found.append(instance)
    return Answer.of(*found)


def classes_with_no_instances() -> Answer:
    populated = set()
    for instance in INSTANCES:
        populated |= kinds_of(instance)
    return Answer.of(*[name for name in CLASSES if name not in populated])
