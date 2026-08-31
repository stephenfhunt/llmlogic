"""Ground truth, in plain Python. Control 1: never the engine being measured.

Two counterfactual oracles of the same boring shape — **change one fact and
re-derive**, over `access_control.truth`'s breadth-first closure — and one that
reads the chain itself. The counterfactuals are deliberately the slow spelling:
an oracle for a question about derivations must be obviously right by inspection,
because nothing else checks it. The fast, analytic spelling — count the grants
that would confer the permission, without deleting anything — is what the tests
use, as the independent formulation.

**One pass, not one per resource.** Each candidate fact is removed or added once,
the whole index is re-derived once, and every resource reads its answer out of
that single pass. The obvious spelling — re-derive per (resource, candidate) —
is the one that made `access_control` unable to build an `at-scale` slate, and
these questions pick their subject by comparing every resource's answer.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import replace
from functools import lru_cache

from harness.domains.access_control import truth as access
from harness.domains.provenance.fixture import ACTION, PINNED, Policy
from harness.task import Answer


def _held(policy: Policy) -> dict[str, set[tuple[str, str]]]:
    """Every user's ``(resource, action)`` permissions, revocations applied."""
    return {user: access.permissions(user, policy) for user in policy.users}


@lru_cache(maxsize=8)
def _critical(action: str, policy: Policy) -> dict[str, set[tuple[str, str, str]]]:
    """resource -> the ``(user, group, role)`` triples whose grant that user's
    access to it depends on.

    A grant is *critical* for a user and a resource when removing that one
    ``grant(group, role)`` row takes the permission away — the user has no second
    route to it. Criticality is a property of the derivation, not of the answer
    set: two users can both hold a permission and only one of them depend on a
    given grant for it.
    """
    before = _held(policy)
    found: dict[str, set[tuple[str, str, str]]] = defaultdict(set)
    for group, role in policy.grants:
        without = replace(policy, grants=tuple(g for g in policy.grants if g != (group, role)))
        after = _held(without)
        for user in policy.users:
            for resource, act in before[user] - after[user]:
                if act == action:
                    found[resource].add((user, group, role))
    return dict(found)


@lru_cache(maxsize=8)
def _repairs(action: str, policy: Policy) -> dict[str, set[tuple[str, str]]]:
    """resource -> the ``(user, group)`` memberships that would grant the access.

    The `?whynot` question, asked of the fact base rather than of a program: the
    user cannot do this, and adding *one* membership is what would change that.
    A group the user is already in is not a repair, and neither is one that
    grants the roles but leaves a revocation standing — which is the case a
    subject reasoning from the role hierarchy alone gets wrong.
    """
    before = _held(policy)
    groups = sorted({group for _, group in policy.members})
    found: dict[str, set[tuple[str, str]]] = defaultdict(set)
    for group in groups:
        for user in policy.users:
            if (user, group) in policy.members:
                continue
            joined = replace(policy, members=policy.members + ((user, group),))
            for resource, act in access.permissions(user, joined) - before[user]:
                if act == action:
                    found[resource].add((user, group))
    return dict(found)


@lru_cache(maxsize=8)
def _justifications(action: str, policy: Policy) -> dict[str, set[tuple[str, str]]]:
    """resource -> the ``(user, role)`` pairs on some chain that confers it.

    A role is on a chain when it is reachable from a role the user holds directly
    and the role that permits the pair is reachable from it — the roles a proof
    tree would name between the grant and the permission, both ends included.
    Roles the user holds but that lead nowhere near this resource are not on it,
    which is what makes this a question about the derivation rather than about
    the closure.
    """
    includes: dict[str, set[str]] = defaultdict(set)
    for holder, included in policy.includes:
        includes[holder].add(included)
    direct = {
        user: {
            role
            for group in {g for member, g in policy.members if member == user}
            for granted, role in policy.grants
            if granted == group
        }
        for user in policy.users
    }
    held = _held(policy)
    found: dict[str, set[tuple[str, str]]] = defaultdict(set)
    for resource in policy.resources:
        permitting = {
            role for role, res, act in policy.permits if res == resource and act == action
        }
        if not permitting:
            continue
        for user in policy.users:
            if (resource, action) not in held[user]:
                continue
            for role in _roles_between(direct[user], permitting, includes):
                found[resource].add((user, role))
    return dict(found)


def _roles_between(
    start: set[str], permitting: set[str], includes: dict[str, set[str]]
) -> set[str]:
    """Every role reachable from ``start`` that can itself reach ``permitting``.

    Two reachability walks, which is the question's own wording: *reachable from
    a role the user holds, and a role permitting the action is reachable from
    it*. The alternative spelling — a depth-first walk keeping its trail, so a
    role is on a chain only when one walk passes through it without repeating —
    agrees with this everywhere the fixture can go and disagrees on a **cycle**:
    with ``a -> b -> a`` and ``a`` itself permitting, this form calls ``b`` a
    justification and the trailed walk does not. The role hierarchy these
    questions are asked about is a chain built by `access_control.fixture._chain`
    and has no cycles, so the case is unreachable rather than decided quietly —
    and the wording, not the walk, is what the subject is graded against
    (`tests/test_provenance_truth.py` pins both halves of this).
    """
    forward = _reachable_from(start, includes)
    return {role for role in forward if _reachable_from({role}, includes) & permitting}


def _reachable_from(start: set[str], includes: dict[str, set[str]]) -> set[str]:
    """The roles reachable through inclusion, the given ones included."""
    seen = set(start)
    queue = list(start)
    while queue:
        role = queue.pop()
        for included in includes.get(role, ()):
            if included not in seen:
                seen.add(included)
                queue.append(included)
    return seen


def critical_grants(resource: str, action: str = ACTION, policy: Policy = PINNED) -> Answer:
    return Answer.of(*_critical(action, policy).get(resource, set()))


def single_membership_repairs(
    resource: str, action: str = ACTION, policy: Policy = PINNED
) -> Answer:
    return Answer.of(*_repairs(action, policy).get(resource, set()))


def justifying_roles(resource: str, action: str = ACTION, policy: Policy = PINNED) -> Answer:
    return Answer.of(*_justifications(action, policy).get(resource, set()))


def critical_grants_per_resource(action: str = ACTION, policy: Policy = PINNED) -> dict[str, int]:
    """How many critical grants each resource has, for picking a subject."""
    return {resource: len(rows) for resource, rows in _critical(action, policy).items()}


def repairs_per_resource(action: str = ACTION, policy: Policy = PINNED) -> dict[str, int]:
    return {resource: len(rows) for resource, rows in _repairs(action, policy).items()}


def justifications_per_resource(action: str = ACTION, policy: Policy = PINNED) -> dict[str, int]:
    return {resource: len(rows) for resource, rows in _justifications(action, policy).items()}
