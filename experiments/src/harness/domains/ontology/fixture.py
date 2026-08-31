"""A class hierarchy, its instances, and what they inherit.

Two entry points, answering different questions.

``build()`` returns the **pinned** hierarchy — written out rather than generated
because its *shape* is what the domain tests: a diamond, two mixins and three
levels of depth are properties a random DAG has only by luck. The instances are
seeded, so which individuals sit where is not quietly tuned to be easy.

``generate(seed, difficulty, track)`` returns a fresh one, and the shape is built
**by construction** rather than hoped for: levels, then diamonds spliced in at a
shared grandparent, then mixins crossing the spine. Difficulty is depth and the
number of routes to the same class, not the number of rows. Property
declarations stay on a single root-to-leaf **spine**, which is what makes
"the most specific declaration wins" have exactly one answer per instance even
though the hierarchy is not a chain.
"""

from __future__ import annotations

import random
from dataclasses import dataclass

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

#: The property the questions are about, and the one that is only ever a
#: distractor. Named here because a generated hierarchy needs the same two roles.
ASKED = "power_profile"
DISTRACTOR = "owner_team"

#: ``(levels, classes per level, diamonds, spine declarations, instances,
#: disjoint pairs, crafted conflicts, empty leaves)``.
#:
#: The knobs that matter are the first three. Depth is where a closure slips a
#: generation; a *diamond* is where a class is reached two ways, so an arm that
#: counts ancestors rather than collecting them counts one twice; and the spine
#: declarations are the override depth — with two, "most specific wins" is a
#: choice between the root and one other, and with six it is a walk.
DIFFICULTY: dict[int, tuple[int, int, int, int, int, int, int, int]] = {
    1: (3, 5, 1, 2, 18, 2, 3, 3),  # the pinned shape's neighbourhood
    2: (4, 4, 2, 3, 40, 3, 4, 4),
    3: (5, 4, 3, 4, 80, 3, 6, 5),
    4: (6, 5, 4, 5, 140, 4, 8, 6),
    5: (7, 5, 6, 6, 220, 4, 10, 7),
}

#: The ``at-scale`` multiplier on the instance population. The hierarchy is
#: **not** multiplied: the closure is over classes, and a 700x class DAG makes
#: the item about reading a large graph rather than about inheritance. Sized so
#: the fixture actually exceeds `cell.FIXTURE_TOKEN_BUDGET`; a test pins it.
AT_SCALE = 2000

#: The highest difficulty the ``at-scale`` track accepts. Scale is the variable
#: on that track, and crossing it with structure produces an item about neither.
AT_SCALE_MAX_DIFFICULTY = 3

#: How many instances the **rare** class holds, whatever the population. A fixed
#: handful, not a share: it is what a question can be scoped to at `at-scale`
#: and still have an answer somebody can write.
RARE_INSTANCES = 9

#: Ceilings on the crafted conflicts, for the same reason.
MAX_CONFLICTS = 12


@dataclass(frozen=True)
class Ontology:
    """One hierarchy, its instances, and the two properties declared over it.

    A value rather than module globals, so the oracle can be handed a
    *generated* hierarchy and checked against a second formulation on it. With
    globals the oracle can only ever be checked against the one hierarchy it was
    written for, which is how a fixture comes to be tuned to its truth.
    """

    classes: tuple[str, ...]
    subclass: tuple[tuple[str, str], ...]
    declares: tuple[tuple[str, str, str], ...]
    disjoint: tuple[tuple[str, str], ...]
    instance_of: tuple[tuple[str, str], ...]
    instances: tuple[str, ...]
    #: A deep class holding a fixed handful of instances, so an `at-scale`
    #: question can be scoped to it. Empty on the pinned hierarchy, which has no
    #: `at-scale` form.
    rare_class: str = ""
    #: A mixin some but not all of the rare class's instances also carry — the
    #: *but not* half of the narrowed question.
    narrowing_class: str = ""


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

PINNED = Ontology(
    classes=CLASSES,
    subclass=SUBCLASS,
    declares=DECLARES,
    disjoint=DISJOINT,
    instance_of=INSTANCE_OF,
    instances=INSTANCES,
)


def _descendants(subclass: tuple[tuple[str, str], ...], root: str) -> set[str]:
    """Every class below ``root``, the root included."""
    below = {root}
    changed = True
    while changed:
        changed = False
        for child, parent in subclass:
            if parent in below and child not in below:
                below.add(child)
                changed = True
    return below


def generate(seed: int, difficulty: int = 3, track: str = "in-context") -> Ontology:
    """A fresh hierarchy and population.

    Identifiers are regenerated per seed, so nothing here can be answered from
    memory and two runs at the same difficulty are two samples rather than the
    same items twice.
    """
    if difficulty not in DIFFICULTY:
        raise ValueError(f"difficulty must be one of {sorted(DIFFICULTY)}")
    if track == "at-scale" and difficulty > AT_SCALE_MAX_DIFFICULTY:
        raise ValueError(
            f"at-scale takes difficulty 1-{AT_SCALE_MAX_DIFFICULTY}: scale is the "
            "variable on that track, and crossing it with structure produces an "
            "item that is about neither"
        )
    depth, per_level, diamonds, declarations, n_instances, pairs, conflicts, empties = DIFFICULTY[
        difficulty
    ]
    if track == "at-scale":
        n_instances *= AT_SCALE

    rng = random.Random(seed)
    tag = f"{seed:x}"[-4:]
    root = f"k{tag}root"
    levels: list[list[str]] = [[root]]
    counter = 0
    for _ in range(depth - 1):
        level = []
        for _ in range(per_level):
            level.append(f"k{tag}{counter:03d}")
            counter += 1
        levels.append(level)

    edges: set[tuple[str, str]] = set()
    for upper, lower in zip(levels, levels[1:], strict=False):
        for name in lower:
            edges.add((name, rng.choice(upper)))

    # Diamonds, spliced in rather than hoped for: a class two levels down gets a
    # second parent that shares its grandparent, so the grandparent is reached by
    # two routes. A random second edge usually makes a class reachable twice from
    # the *root*, which every class already is.
    # **`sorted`, not `edges`.** A set of strings iterates in an order Python
    # randomizes per process, so a dict built from one — and a list derived from
    # that dict — hands `rng.choice` a different sequence every time the same
    # seed is drawn. The truth stayed put and the *fixture* moved, which is the
    # one shape a manifest cannot survive: `calibrate.load` regenerates a slate
    # and checks its fingerprint, so an item pinned in one process was refused in
    # the next. Found 2026-08-30, on the first slate anything had ever run from.
    parent_of = {child: parent for child, parent in sorted(edges)}
    for _ in range(diamonds):
        candidates = [name for level in levels[2:] for name in level if name in parent_of]
        if not candidates:
            break
        child = rng.choice(candidates)
        grandparent = parent_of.get(parent_of[child])
        siblings = sorted(
            name
            for name, parent in parent_of.items()
            if parent == grandparent and name != parent_of[child]
        )
        if siblings:
            edges.add((child, rng.choice(siblings)))

    # Mixins: extra roots attached under the top, crossing the spine. They are
    # what makes "the ancestors of X" a set rather than a path.
    for index in range(2):
        edges.add((f"k{tag}mix{index}", root))

    subclass = tuple(sorted(edges))
    classes = tuple(sorted({name for pair in subclass for name in pair} | {root}))

    # The spine: one class per level, each below the last. Declarations of the
    # asked-about property live only here, so an instance's applicable
    # declarations are a *chain* and "the most specific wins" has exactly one
    # answer — under multiple inheritance that is a property of where the
    # declarations are, not of the closure. A test asserts it on every seed.
    spine = [root]
    for level in levels[1:]:
        below = [name for name in level if spine[-1] in {p for c, p in subclass if c == name}]
        if not below:
            break
        spine.append(rng.choice(below))
    spine = spine[:declarations]

    leaves = [
        name
        for name in classes
        if name not in {parent for _, parent in subclass}
        and not name.endswith(("root", "mix0", "mix1"))
    ]
    mixins = tuple(f"k{tag}mix{index}" for index in range(2))
    abstract = {root, *mixins}
    rare = leaves[-1]
    # At most half the leaves, so the deepest populated classes are still leaves
    # and the closure has somewhere to go. Two at minimum: a one-row answer is
    # close enough to guessable that `validate` refuses it.
    empty = set(rng.sample(leaves[:-1], min(empties, max(2, (len(leaves) - 1) // 2))))

    # One spine class whose value is **always overridden**: every class under it
    # that is not also under the next spine class is barred from holding an
    # instance, so nothing can take its value. Without this every declared value
    # was somebody's effective value on every seed, and `dead-declarations` — the
    # question the pinned slate cannot ask — had an empty answer.
    barred: set[str] = set()
    if len(spine) >= 3:
        overridden = spine[len(spine) // 2]
        barred = _descendants(subclass, overridden) - _descendants(
            subclass, spine[len(spine) // 2 + 1]
        )

    declares = tuple((name, ASKED, f"v{tag}{index}") for index, name in enumerate(spine))
    # Declarations on classes nothing is beneath. Dead by a second route, and
    # safe for the uniqueness above precisely because they never apply.
    declares += tuple(
        (name, ASKED, f"x{tag}{index}") for index, name in enumerate(sorted(empty)[:2])
    )
    # The distractor, on classes that are *unrelated* to each other, so an arm
    # that ignores the property column answers with something that is not even a
    # chain.
    unrelated = [name for name in classes if name not in spine and name != root]
    declares += tuple(
        (name, DISTRACTOR, f"t{tag}{index}")
        for index, name in enumerate(rng.sample(unrelated, min(3, len(unrelated))))
    )

    forbidden = empty | abstract | barred | {rare}
    populated = [name for name in classes if name not in forbidden]

    # Disjoint pairs whose subtrees do not intersect. A pair that shared a
    # descendant would make every instance of that descendant inconsistent by
    # construction, which is a contradiction in the *ontology* rather than a
    # mislabelled instance — a different finding, and not the one asked about.
    # Both sides must have somewhere an instance can actually sit, or the pair
    # is a constraint nothing can violate: at d1 every crafted conflict was
    # silently skipped, and `inconsistent-instances` answered nothing.
    candidates = [
        name
        for name in classes
        if name not in abstract and _descendants(subclass, name) & set(populated)
    ]
    possible = [
        (left, right)
        for index, left in enumerate(candidates)
        for right in candidates[index + 1 :]
        if not _descendants(subclass, left) & _descendants(subclass, right)
    ]
    rng.shuffle(possible)
    # Greedy on *distinct* classes: taking the first few in any fixed order gave
    # three pairs sharing one left-hand class, which is one constraint wearing
    # three hats — an arm that finds it finds all of them.
    disjoint: list[tuple[str, str]] = []
    used: set[str] = set()
    for left, right in possible:
        if len(disjoint) >= pairs:
            break
        if left not in used and right not in used:
            disjoint.append((left, right))
            used |= {left, right}
    disjoint.extend(possible[: pairs - len(disjoint)])
    disjoint = sorted(set(disjoint))[:pairs]

    instances = tuple(f"o{tag}{index:05d}" for index in range(n_instances))
    memberships: set[tuple[str, str]] = set()
    # A fixed handful in the rare class, whatever the population: it is what an
    # `at-scale` question is scoped to, and a share of 12,600 instances is an
    # answer nobody can write.
    rare_members = instances[:RARE_INSTANCES]
    for index, instance in enumerate(rare_members):
        memberships.add((instance, rare))
        if index % 3 == 0:
            memberships.add((instance, mixins[0]))

    crafted = instances[RARE_INSTANCES : RARE_INSTANCES + min(conflicts, MAX_CONFLICTS)]
    for index, instance in enumerate(crafted):
        left, right = disjoint[index % len(disjoint)] if disjoint else (populated[0], populated[0])
        under_left = sorted(_descendants(subclass, left) - forbidden)
        under_right = sorted(_descendants(subclass, right) - forbidden)
        # Falling back to the pair's own class would put an instance in a class
        # reserved as empty, or in the rare class, or under the barred subtree —
        # and each of those is a guarantee another question rests on. A conflict
        # that cannot be sited without breaking one is skipped.
        if under_left and under_right:
            memberships.add((instance, rng.choice(under_left)))
            memberships.add((instance, rng.choice(under_right)))
        else:
            memberships.add((instance, rng.choice(populated)))

    for instance in instances[RARE_INSTANCES + len(crafted) :]:
        memberships.add((instance, rng.choice(populated)))
        # Honest second classes, so two classes on one instance is not itself the
        # signal that something is wrong.
        if rng.random() < 0.2:
            memberships.add((instance, rng.choice(mixins)))

    return Ontology(
        classes=classes,
        subclass=subclass,
        declares=tuple(sorted(declares)),
        disjoint=tuple(sorted(disjoint)),
        instance_of=tuple(sorted(memberships)),
        instances=instances,
        rare_class=rare,
        narrowing_class=mixins[0],
    )


def to_fixture(ontology: Ontology) -> Fixture:
    """The five CSVs a workspace gets. One place, so a generated hierarchy and
    the pinned one are laid out identically and no cell is decided by layout."""
    return Fixture(
        files={
            "class.csv": "class\n" + "".join(f"{name}\n" for name in ontology.classes),
            "subclass.csv": "class,superclass\n"
            + "".join(f"{c},{s}\n" for c, s in ontology.subclass),
            "instance.csv": "instance,class\n"
            + "".join(f"{i},{c}\n" for i, c in ontology.instance_of),
            "declares.csv": "class,property,value\n"
            + "".join(f"{c},{p},{v}\n" for c, p, v in ontology.declares),
            "disjoint.csv": "class,other_class\n"
            + "".join(f"{a},{b}\n" for a, b in ontology.disjoint),
        },
        schemas={
            "class": ("class",),
            "subclass": ("class", "superclass"),
            "instance": ("instance", "class"),
            "declares": ("class", "property", "value"),
            "disjoint": ("class", "other_class"),
        },
    )


def build() -> Fixture:
    return to_fixture(PINNED)
