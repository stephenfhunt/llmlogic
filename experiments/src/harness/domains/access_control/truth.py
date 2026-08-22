"""Ground truth, in plain Python. Control 1: never the engine being measured.

The closure is a breadth-first search, written the boring way on purpose — this
code has to be obviously right by inspection, because nothing else checks it.
"""

from __future__ import annotations

from collections import deque

from harness.domains.access_control.fixture import (
    GRANTS,
    INCLUDES,
    MEMBERS,
    PERMITS,
    REVOKED,
    USERS,
)
from harness.task import Answer


def closure(roles: set[str], includes: tuple[tuple[str, str], ...] = INCLUDES) -> set[str]:
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


def effective_roles(user: str) -> set[str]:
    groups = {group for member, group in MEMBERS if member == user}
    direct = {role for group, role in GRANTS if group in groups}
    return closure(direct)


def permissions(user: str) -> set[tuple[str, str]]:
    """``(resource, action)`` pairs the user actually has, revocations applied."""
    roles = effective_roles(user)
    revoked = {resource for holder, resource in REVOKED if holder == user}
    return {
        (resource, action)
        for role, resource, action in PERMITS
        if role in roles and resource not in revoked
    }


def users_who_can(action: str, resource: str) -> Answer:
    return Answer.of(*[user for user in USERS if (resource, action) in permissions(user)])


def resources_reachable_by(user: str) -> Answer:
    return Answer.of(*{resource for resource, _ in permissions(user)})


def users_with_no_access() -> Answer:
    return Answer.of(*[user for user in USERS if not permissions(user)])


def can_delete_but_not_read() -> Answer:
    """The subtle one: a permission the user holds without the one it implies.

    Nothing in an answer to this question announces a dropped row, which is why
    it is here.
    """
    found = []
    for user in USERS:
        held = permissions(user)
        for resource, action in held:
            if action == "delete" and (resource, "read") not in held:
                found.append((user, resource))
    return Answer.of(*found)
