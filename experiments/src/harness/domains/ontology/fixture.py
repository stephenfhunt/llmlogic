"""A class hierarchy, hand-written; instances, generated from a fixed seed.

The hierarchy is written out rather than generated because its *shape* is what
the domain tests — a diamond, two mixins and three levels of depth are properties
a random DAG has only by luck. The instances are seeded, so which individuals sit
where is not quietly tuned to be easy.
"""

from __future__ import annotations

import random

from harness.task import Fixture

SEED = 20260822

#: `(class, superclass)` — read "every class is a superclass". Multiple
#: inheritance is deliberate: `laptop` is both a `computer` and `portable`, and
#: `server` is both a `computer` and `managed`, so the ancestor set of an
#: instance is reached by more than one route.
SUBCLASS: tuple[tuple[str, str], ...] = (
    ("asset", "thing"),
    ("location", "thing"),
    ("managed", "thing"),
    ("portable", "asset"),
    ("device", "asset"),
    ("licence", "asset"),
    ("computer", "device"),
    ("sensor", "device"),
    ("network_gear", "device"),
    ("laptop", "computer"),
    ("laptop", "portable"),
    ("server", "computer"),
    ("server", "managed"),
    ("rack_server", "server"),
    ("blade_server", "server"),
    ("thermal_sensor", "sensor"),
    ("pressure_sensor", "sensor"),
    ("handheld_sensor", "sensor"),
    ("handheld_sensor", "portable"),
    ("switch", "network_gear"),
    ("router", "network_gear"),
    ("software_licence", "licence"),
    ("building", "location"),
    ("room", "location"),
)

CLASSES: tuple[str, ...] = tuple(sorted({name for pair in SUBCLASS for name in pair} | {"thing"}))

#: `(class, property, value)`. Two properties, and only one of them is asked
#: about: `power_profile` is declared along a single spine (`thing` → `device` →
#: `sensor`/`server`) and nowhere else, so every instance has exactly one nearest
#: declaration even when it carries two unrelated classes. `owner_team` is
#: declared on both `device` and `licence`, which *are* unrelated — it is the
#: distractor, and an arm that ignores the property column answers with it.
DECLARES: tuple[tuple[str, str, str], ...] = (
    ("thing", "owner_team", "unassigned"),
    ("asset", "owner_team", "facilities"),
    ("device", "owner_team", "it"),
    ("server", "owner_team", "platform"),
    ("rack_server", "owner_team", "datacentre"),
    ("sensor", "owner_team", "plant"),
    ("licence", "owner_team", "procurement"),
    ("thing", "power_profile", "unknown"),
    ("device", "power_profile", "medium"),
    ("sensor", "power_profile", "low"),
    ("server", "power_profile", "high"),
)

#: Declared disjoint: nothing is both. Symmetric in meaning, listed once — the
#: question says so, so neither arm has to guess.
DISJOINT: tuple[tuple[str, str], ...] = (
    ("device", "licence"),
    ("asset", "location"),
    ("portable", "network_gear"),
)

#: Classes deliberately left with no instance anywhere beneath them, so
#: "which classes have nothing in them?" has a real answer that is not the whole
#: vocabulary. A floor, not the answer: the seeded assignment leaves others empty
#: too, and the oracle is what says which.
EMPTY_CLASSES = ("blade_server", "pressure_sensor", "building")

#: What an instance can be declared as. The abstract tops and the two mixins are
#: excluded: an instance whose only class is `thing` inherits the root's default
#: and tells the inheritance question nothing.
ABSTRACT = ("thing", "asset", "location", "managed", "portable")

#: Instances carrying two classes that are disjoint through the hierarchy — the
#: mislabelling a consistency check exists to find. Crafted rather than seeded:
#: a random assignment produces them only by accident, and never the diamond
#: case where the conflict is two levels above both classes.
INCONSISTENT: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("i03", ("laptop", "software_licence")),
    ("i07", ("thermal_sensor", "software_licence")),
    ("i11", ("laptop", "switch")),
    ("i14", ("router", "room")),
)

INSTANCES = tuple(f"i{i:02d}" for i in range(1, 19))


def _build() -> tuple[tuple[str, str], ...]:
    rng = random.Random(SEED)
    populated = [name for name in CLASSES if name not in EMPTY_CLASSES + ABSTRACT]
    crafted = dict(INCONSISTENT)
    memberships = set()
    for instance in INSTANCES:
        if instance in crafted:
            memberships.update((instance, name) for name in crafted[instance])
            continue
        memberships.add((instance, rng.choice(populated)))
    # A handful of honest second classes, so two classes on one instance is not
    # itself the signal that something is wrong.
    for _ in range(4):
        instance = rng.choice([i for i in INSTANCES if i not in crafted])
        memberships.add((instance, rng.choice(("managed", "portable"))))
    return tuple(sorted(memberships))


INSTANCE_OF = _build()


def build() -> Fixture:
    class_csv = "class\n" + "".join(f"{name}\n" for name in CLASSES)
    subclass_csv = "class,superclass\n" + "".join(f"{c},{s}\n" for c, s in SUBCLASS)
    instance_csv = "instance,class\n" + "".join(f"{i},{c}\n" for i, c in INSTANCE_OF)
    declares_csv = "class,property,value\n" + "".join(f"{c},{p},{v}\n" for c, p, v in DECLARES)
    disjoint_csv = "class,other_class\n" + "".join(f"{a},{b}\n" for a, b in DISJOINT)
    return Fixture(
        files={
            "class.csv": class_csv,
            "subclass.csv": subclass_csv,
            "instance.csv": instance_csv,
            "declares.csv": declares_csv,
            "disjoint.csv": disjoint_csv,
        },
        schemas={
            "class": ("class",),
            "subclass": ("class", "superclass"),
            "instance": ("instance", "class"),
            "declares": ("class", "property", "value"),
            "disjoint": ("class", "other_class"),
        },
    )
