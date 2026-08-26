"""Ground truth, in plain Python. Control 1: never the engine being measured.

The closure is a breadth-first search, written the boring way on purpose — this
code has to be obviously right by inspection, because nothing else checks it.

Every function takes the ``Policy`` it is asked about, defaulting to the pinned
one. That is what lets the oracle be checked against a second formulation on
*generated* graphs: an oracle that can only run against the single graph it was
written for can only ever be checked against that graph's own answers, which is
how a fixture comes to be tuned to its truth.

**Shape, not cleverness, is what makes it fast enough.** The obvious spelling —
scan `permits` inside a loop over users — is fine on twelve users and quadratic
at `at-scale` sizes, where picking a question's subject by its answer size runs
it once per candidate and the slate stops building. Users *share role sets*:
thousands of them over a dozen groups have a few dozen distinct sets between
them, so the closure and the permissions it implies are computed per role set and
looked up per user. Only the revocation is genuinely per-user.
"""

from __future__ import annotations

from collections import defaultdict, deque
from functools import lru_cache

from harness.domains.access_control.fixture import PINNED, Policy
from harness.task import Answer


def closure(roles: set[str], includes: tuple[tuple[str, str], ...] = PINNED.includes) -> set[str]:
    """Every role reachable by following ``includes``, the given roles included.

    ``includes`` is a parameter so the closure can be checked against an
    independent formulation on generated graphs — this is the one piece of real
    algorithm in the file, and the piece a hand-written oracle can get wrong.
    """
    seen = set(roles)
    queue = deque(roles)
    while queue:
        role = queue.popleft()
        for holder, included in includes:
            if holder == role and included not in seen:
                seen.add(included)
                queue.append(included)
    return seen


@lru_cache(maxsize=8)
def _direct_roles(policy: Policy) -> dict[str, frozenset[str]]:
    """Each user's directly granted roles, before any closure."""
    by_group: dict[str, set[str]] = defaultdict(set)
    for group, role in policy.grants:
        by_group[group].add(role)
    groups_of: dict[str, set[str]] = defaultdict(set)
    for member, group in policy.members:
        groups_of[member].add(group)
    return {
        user: frozenset({role for group in groups_of[user] for role in by_group[group]})
        for user in policy.users
    }


@lru_cache(maxsize=8)
def _index(policy: Policy) -> dict[str, set[tuple[str, str]]]:
    """Every user's ``(resource, action)`` permissions, revocations applied.

    ``Policy`` is frozen with tuple fields, so it hashes; a different graph is a
    different cache key.
    """
    by_role: dict[str, set[tuple[str, str]]] = defaultdict(set)
    for role, resource, action in policy.permits:
        by_role[role].add((resource, action))
    revoked: dict[str, set[str]] = defaultdict(set)
    for holder, resource in policy.revoked:
        revoked[holder].add(resource)

    direct = _direct_roles(policy)
    granted: dict[frozenset[str], set[tuple[str, str]]] = {}
    for roles in set(direct.values()):
        held: set[tuple[str, str]] = set()
        for role in closure(set(roles), policy.includes):
            held |= by_role[role]
        granted[roles] = held

    result: dict[str, set[tuple[str, str]]] = {}
    for user in policy.users:
        held = granted[direct[user]]
        blocked = revoked[user]
        # Most users on a large graph have no revocation, and the filtered copy
        # is the whole cost. The shared set is never mutated.
        result[user] = {pair for pair in held if pair[0] not in blocked} if blocked else held
    return result


def effective_roles(user: str, policy: Policy = PINNED) -> set[str]:
    groups = {group for member, group in policy.members if member == user}
    direct = {role for group, role in policy.grants if group in groups}
    return closure(direct, policy.includes)


def permissions(user: str, policy: Policy = PINNED) -> set[tuple[str, str]]:
    """``(resource, action)`` pairs the user actually has, revocations applied."""
    return _index(policy)[user]


def users_who_can(action: str, resource: str, policy: Policy = PINNED) -> Answer:
    index = _index(policy)
    return Answer.of(*[user for user in policy.users if (resource, action) in index[user]])


def resources_reachable_by(user: str, policy: Policy = PINNED) -> Answer:
    return Answer.of(*{resource for resource, _ in permissions(user, policy)})


def users_with_no_access(policy: Policy = PINNED) -> Answer:
    index = _index(policy)
    return Answer.of(*[user for user in policy.users if not index[user]])


def can_delete_but_not_read(policy: Policy = PINNED) -> Answer:
    """The subtle one: a permission the user holds without the one it implies.

    Nothing in an answer to this question announces a dropped row, which is why
    it is here.
    """
    index = _index(policy)
    found = []
    for user in policy.users:
        held = index[user]
        for resource, action in held:
            if action == "delete" and (resource, "read") not in held:
                found.append((user, resource))
    return Answer.of(*found)


def can_delete_but_not_read_on(resource: str, policy: Policy = PINNED) -> Answer:
    """The same subtlety scoped to one resource, so the answer is bounded by the
    user count rather than by their product with the resources."""
    index = _index(policy)
    return Answer.of(
        *[
            user
            for user in policy.users
            if (resource, "delete") in index[user] and (resource, "read") not in index[user]
        ]
    )


def readers_per_resource(policy: Policy = PINNED) -> dict[str, int]:
    """How many users can read each resource, in one pass over the index.

    Asking `users_who_can` per candidate is the obvious spelling and is quadratic:
    on an `at-scale` graph that is 4,000 resources times 4,800 users, and picking
    a question's subject that way stopped the slate building.
    """
    counts: dict[str, int] = dict.fromkeys(policy.resources, 0)
    for held in _index(policy).values():
        for resource, action in held:
            if action == "read":
                counts[resource] = counts.get(resource, 0) + 1
    return counts


def resources_per_user(policy: Policy = PINNED) -> dict[str, int]:
    """How many distinct resources each user can reach, by any action."""
    return {user: len({resource for resource, _ in held}) for user, held in _index(policy).items()}


def users_reachable_only_through(depth: int, policy: Policy = PINNED) -> Answer:
    """Users whose access needs at least ``depth`` hops through role inclusion.

    A question with no shallow answer: it is *false* for anyone whose permission
    comes from a directly granted role, so an arm that closes one hop and stops
    returns a superset, and one that never closes at all returns nothing. The
    shape of the wrong answer says which mistake was made — which the four pinned
    questions cannot, because every wrong answer to those is just a subset.

    Keyed on the role set, for the reason ``_index`` is.
    """
    direct = _direct_roles(policy)
    verdict: dict[frozenset[str], bool] = {}
    for roles in set(direct.values()):
        if not roles:
            verdict[roles] = False
            continue
        # Roles reachable in fewer than `depth` hops, the direct ones counting as
        # zero. Anything beyond this needed the full walk.
        near = set(roles)
        frontier = set(roles)
        for _ in range(depth - 1):
            frontier = {included for holder, included in policy.includes if holder in frontier}
            frontier -= near
            near |= frontier
        verdict[roles] = bool(closure(set(roles), policy.includes) - near)
    return Answer.of(*[user for user in policy.users if verdict[direct[user]]])


@lru_cache(maxsize=8)
def deepest_informative_hop(policy: Policy = PINNED, ceiling: int = 8) -> int:
    """The largest hop count at which ``users_reachable_only_through`` still says
    something, or 0 if the graph cannot support the question at all.

    Fixing the hop count to the hierarchy's nominal depth produced an empty answer
    on every generated graph: a grant lands somewhere in the middle, so the walk
    left from *that* role is shorter than the chain. Asking the graph what it can
    support is honest — the question text states the number — and it keeps the
    item at the hardest depth the fixture actually has.
    """
    for depth in range(ceiling, 1, -1):
        answered = users_reachable_only_through(depth, policy).rows
        if answered and len(answered) < len(policy.users):
            return depth
    return 0


def users_who_can_but_cannot(
    action: str, denied: str, resource: str, policy: Policy = PINNED
) -> Answer:
    """Users holding one action on a resource and not another on the same one.

    The `at-scale` shape of the reachability question. Asked unscoped over a
    400x universe, *who can read this?* answers with 2,487 rows — which measures
    whether a subject can emit a list, not whether it can compute one. Narrowing
    it with a negation keeps the fact base large and the answer small, which is
    the combination the track is actually about.
    """
    index = _index(policy)
    return Answer.of(
        *[
            user
            for user in policy.users
            if (resource, action) in index[user] and (resource, denied) not in index[user]
        ]
    )


def resources_with_action_for(user: str, action: str, policy: Policy = PINNED) -> Answer:
    """One user's resources under one action, rather than under any."""
    return Answer.of(*{resource for resource, held in permissions(user, policy) if held == action})
