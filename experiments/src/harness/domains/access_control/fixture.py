"""A policy graph, generated from a seed.

Generated rather than hand-written so the shape is not quietly tuned to be easy.
There are two entry points and they answer different questions:

``build()`` returns the **pinned** graph — one fixed seed, sized to stay well
inside the smaller strength's context, because on the ``in-context`` track the
prose arm has to hold the fact base in its head and a fixture that defeats it on
size alone would measure the context window (`decisions.md` 2026-08-25).

``generate(seed, difficulty, track)`` returns a fresh one. Difficulty is
**structure, not size** on the ``in-context`` track: a deeper role hierarchy, more
levels for a closure to slip a generation in, and planted near-misses — users one
hop or one revocation away from qualifying. The ``at-scale`` track is the same
generator with the universe enlarged past the prose arm's window, and it is
reported as a separate claim.
"""

from __future__ import annotations

import random
from dataclasses import dataclass

from harness.task import Fixture

SEED = 20260821

#: How deep the role hierarchy goes at each difficulty, and how many entities the
#: universe holds. Depth is the knob that matters: a two-level closure is a join,
#: and a six-level one is where prose reasoning drops a generation.
#:
#: Each row is ``(users, resources, roles, depth, near_misses)``.
DIFFICULTY: dict[int, tuple[int, int, int, int, int]] = {
    1: (12, 10, 8, 2, 0),  # the pinned shape's neighbourhood
    2: (18, 14, 10, 3, 2),
    3: (26, 18, 14, 4, 4),
    4: (36, 24, 18, 5, 6),
    5: (48, 30, 22, 6, 8),
}

#: The ``at-scale`` multiplier on the universe.
#:
#: Sized so the fixture **actually exceeds** `cell.FIXTURE_TOKEN_BUDGET`, which is
#: the entire point of the track — a first pass at 60 produced a 20k-token file
#: that both arms could still read, which is an `in-context` item wearing the
#: wrong label. A test pins that it stays over.
#:
#: The ceiling is what the engine measurably does:
#: `../datalog/notes/performance-baseline.md` imports 27,957 facts in 0.32s and
#: closes a 5,248-edge graph to 173k tuples in 11.6s. And what it measurably does
#: *not*: self-joining a large derived relation there did not finish, so the
#: closure stays anchored on the small `includes` relation.
AT_SCALE = 400

#: The highest difficulty the `at-scale` track accepts.
#:
#: Difficulty and scale multiply, and past this the item stops being about either
#: one: d5 at 400x is 74,681 facts, takes 41s to build an oracle for, and its
#: narrowed questions answer nothing at all. Scale is the variable on this track,
#: so the structural knob is held near the bottom of its range rather than
#: crossed with it.
AT_SCALE_MAX_DIFFICULTY = 3


@dataclass(frozen=True)
class Policy:
    """One policy graph, and the universe it is closed over.

    A value rather than module globals, so the oracle can be handed a *generated*
    graph and checked against a second formulation on it. With globals the oracle
    can only ever be checked against the one graph it was written for, which is
    how a fixture comes to be tuned to its own truth.
    """

    users: tuple[str, ...]
    resources: tuple[str, ...]
    members: tuple[tuple[str, str], ...]
    grants: tuple[tuple[str, str], ...]
    includes: tuple[tuple[str, str], ...]
    permits: tuple[tuple[str, str, str], ...]
    revoked: tuple[tuple[str, str], ...]


ACTIONS = ("read", "write", "delete")

#: A group granted nothing, with exactly one member. The "who has no access at
#: all?" question needs a non-empty answer, or a subject that writes an empty file
#: without looking scores correct. Kept out of the ordinary groups so nothing else
#: is assigned to it and no grant is generated for it.
UNGRANTED_GROUP = "quarantine"

GROUPS = ("eng", "sre", "support", "finance", "contractors", "leads")
ROLES = ("admin", "deployer", "reader", "writer", "auditor", "billing", "oncall", "guest")
USERS = tuple(f"u{i:02d}" for i in range(1, 13))
RESOURCES = tuple(f"r{i:02d}" for i in range(1, 11))
ISOLATED_USER = "u12"

#: The pinned hierarchy: including a role confers everything that role confers.
#: Three levels deep, so the closure is not a single hop.
INCLUDES: tuple[tuple[str, str], ...] = (
    ("admin", "deployer"),
    ("admin", "auditor"),
    ("deployer", "writer"),
    ("writer", "reader"),
    ("oncall", "deployer"),
    ("billing", "reader"),
)


def _build() -> Policy:
    rng = random.Random(SEED)
    assignable = [user for user in USERS if user != ISOLATED_USER]
    members = sorted(
        {(user, rng.choice(GROUPS)) for user in assignable}
        | {(rng.choice(assignable), rng.choice(GROUPS)) for _ in range(6)}
        | {(ISOLATED_USER, UNGRANTED_GROUP)}
    )
    grants = sorted(
        {(group, rng.choice(ROLES)) for group in GROUPS}
        | {(rng.choice(GROUPS), rng.choice(ROLES)) for _ in range(4)}
    )
    # Every resource gets one permit per action, so no question about a resource
    # can be answered correctly by a subject that never looked. A task whose truth
    # is empty is one an agent passes by doing nothing.
    permits = sorted(
        {(rng.choice(ROLES), resource, action) for resource in RESOURCES for action in ACTIONS}
        | {(rng.choice(ROLES), rng.choice(RESOURCES), rng.choice(ACTIONS)) for _ in range(8)}
    )
    revoked = sorted({(rng.choice(USERS), rng.choice(RESOURCES)) for _ in range(4)})
    return Policy(
        users=USERS,
        resources=RESOURCES,
        members=tuple(members),
        grants=tuple(grants),
        includes=INCLUDES,
        permits=tuple(permits),
        revoked=tuple(revoked),
    )


PINNED = _build()

#: The pinned graph's relations, kept as names because the pinned tasks and the
#: reference corpus are written against them.
MEMBERS, GRANTS, PERMITS, REVOKED = (
    PINNED.members,
    PINNED.grants,
    PINNED.permits,
    PINNED.revoked,
)


def _chain(roles: list[str], depth: int, rng: random.Random) -> tuple[tuple[str, str], ...]:
    """A role hierarchy ``depth`` levels deep, with cross-links.

    A pure chain is a path and a subject can walk it by eye. The cross-links make
    it a DAG with more than one route to the same role, which is where a closure
    that stops one hop early stops being visible in the answer's shape.
    """
    edges: set[tuple[str, str]] = set()
    levels: list[list[str]] = []
    remaining = list(roles)
    per_level = max(2, len(roles) // max(1, depth))
    while remaining and len(levels) < depth:
        levels.append(remaining[:per_level])
        remaining = remaining[per_level:]
    if remaining:
        levels[-1].extend(remaining)
    for upper, lower in zip(levels, levels[1:], strict=False):
        for role in upper:
            edges.add((role, rng.choice(lower)))
        # One cross-link per level, so at least one role is reachable two ways.
        edges.add((rng.choice(upper), rng.choice(lower)))
    return tuple(sorted(edges))


def _top_roles(includes: tuple[tuple[str, str], ...], roles: list[str]) -> list[str]:
    """Roles nothing includes — the entry points to the hierarchy.

    Granting one of these is what makes a question need the closure at all.
    """
    included = {lower for _, lower in includes}
    return [role for role in roles if role not in included] or list(roles)


def generate(seed: int, difficulty: int = 3, track: str = "in-context") -> Policy:
    """A fresh policy graph.

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
    n_users, n_resources, n_roles, depth, near_misses = DIFFICULTY[difficulty]
    if track == "at-scale":
        n_users *= AT_SCALE
        n_resources *= AT_SCALE

    rng = random.Random(seed)
    tag = f"{seed:x}"[-4:]
    users = tuple(f"u{tag}{i:04d}" for i in range(n_users))
    resources = tuple(f"r{tag}{i:04d}" for i in range(n_resources))
    roles = [f"role{tag}{i:02d}" for i in range(n_roles)]
    groups = tuple(f"g{tag}{i:02d}" for i in range(max(4, n_roles // 2)))

    includes = _chain(roles, depth, rng)

    # Users in nothing but the ungranted group, so "who has no access?" has an
    # answer that is neither empty nor findable by listing everyone. More than
    # one on purpose: a single-row answer is close enough to guessable that it
    # carries almost nothing, and `validate` rejects it.
    # A fixed handful, not a proportion. Proportional put 1,920 users in it at
    # `at-scale`, and an answer with 1,920 rows measures whether the subject can
    # transcribe a list, not whether it can find one.
    isolated = users[-min(6, max(2, n_users // 10)) :]
    assignable = users[: -len(isolated)]
    members = {(user, UNGRANTED_GROUP) for user in isolated}
    members |= {(user, rng.choice(groups)) for user in assignable}
    members |= {(rng.choice(assignable), rng.choice(groups)) for _ in range(n_users // 3)}

    # Grants land on the *top* of the hierarchy at least as often as anywhere
    # else. A grant straight onto a leaf role gives a user their whole permission
    # set in zero hops, and a graph where that is the common case cannot ask
    # anything the closure is needed for.
    top = _top_roles(includes, roles)
    grants = {(group, rng.choice(top if rng.random() < 0.6 else roles)) for group in groups}
    grants |= {(rng.choice(groups), rng.choice(roles)) for _ in range(n_roles // 2)}

    permits = {
        (rng.choice(roles), resource, action) for resource in resources for action in ACTIONS
    }
    permits |= {
        (rng.choice(roles), rng.choice(resources), rng.choice(ACTIONS)) for _ in range(n_resources)
    }

    # The near-misses: a revocation on a user who *would* otherwise qualify. This
    # is the distractor that matters, because it is invisible to anyone who
    # computed the closure and stopped there.
    revoked = {
        (rng.choice(users[:-1]), rng.choice(resources)) for _ in range(near_misses + n_users // 8)
    }

    return Policy(
        users=users,
        resources=resources,
        members=tuple(sorted(members)),
        grants=tuple(sorted(grants)),
        includes=includes,
        permits=tuple(sorted(permits)),
        revoked=tuple(sorted(revoked)),
    )


def to_fixture(policy: Policy) -> Fixture:
    """The five CSVs a workspace gets. One place, so a generated graph and the
    pinned one are laid out identically and no cell is decided by its layout."""
    return Fixture(
        files={
            "member.csv": "user,group\n" + "".join(f"{u},{g}\n" for u, g in policy.members),
            "grant.csv": "group,role\n" + "".join(f"{g},{r}\n" for g, r in policy.grants),
            "includes.csv": "role,included_role\n"
            + "".join(f"{a},{b}\n" for a, b in policy.includes),
            "permit.csv": "role,resource,action\n"
            + "".join(f"{r},{res},{a}\n" for r, res, a in policy.permits),
            "revoked.csv": "user,resource\n" + "".join(f"{u},{r}\n" for u, r in policy.revoked),
        },
        schemas={
            "member": ("user", "group"),
            "grant": ("group", "role"),
            "includes": ("role", "included_role"),
            "permit": ("role", "resource", "action"),
            "revoked": ("user", "resource"),
        },
    )


def build() -> Fixture:
    return to_fixture(PINNED)
