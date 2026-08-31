"""The counterfactual oracles, each against a formulation that is not its own.

Ground truth is what every verdict rests on, and these three are the only oracles
in the slate that answer a question about a *derivation* rather than about an
extension. The check that matters is therefore not "does it run" but "does the
slow, obviously-right spelling agree with a spelling that shares none of its
steps" — removal-and-re-derive against counting the routes, and a trailed walk
against two reachability sets.
"""

from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st

from harness.domains.access_control import truth as access
from harness.domains.provenance import fixture, truth

SEEDS = [s * 7919 for s in range(1, 7)]
ACTION = fixture.ACTION


def _closure(roles, includes):
    return access.closure(set(roles), includes)


def _routes(policy, user, resource, action):
    """Every grant that would confer this permission on this user, counted.

    The independent formulation of criticality: a grant is critical when it is
    the *only* route, so counting routes answers the question without ever
    deleting a row and re-deriving. Revocation is applied last, exactly as the
    permission rule applies it.
    """
    if any(holder == user and res == resource for holder, res in policy.revoked):
        return set()
    groups = {group for member, group in policy.members if member == user}
    found = set()
    for group, role in policy.grants:
        if group not in groups:
            continue
        conferred = _closure({role}, policy.includes)
        if any(
            r in conferred and res == resource and act == action for r, res, act in policy.permits
        ):
            found.add((group, role))
    return found


def _reachable(role, includes):
    seen = {role}
    queue = [role]
    while queue:
        current = queue.pop()
        for holder, included in includes:
            if holder == current and included not in seen:
                seen.add(included)
                queue.append(included)
    return seen


class TestCriticalGrants:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_a_grant_is_critical_exactly_when_it_is_the_only_route(self, seed):
        policy = fixture.generate(seed, 3)
        for resource in policy.resources:
            expected = {
                (user, group, role)
                for user in policy.users
                for group, role in _routes(policy, user, resource, ACTION)
                if len(_routes(policy, user, resource, ACTION)) == 1
            }
            got = set(truth.critical_grants(resource, ACTION, policy).rows)
            assert got == expected

    def test_a_user_without_the_access_is_in_no_row(self):
        policy = fixture.PINNED
        resource = _subject(truth.critical_grants_per_resource(ACTION, policy))
        for user, _group, _role in truth.critical_grants(resource, ACTION, policy).rows:
            assert (resource, ACTION) in access.permissions(user, policy)


class TestRepairs:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_a_repair_is_exactly_a_group_whose_roles_reach_the_resource(self, seed):
        """Stated over the role hierarchy rather than by joining and re-deriving.

        The revocation is the half a subject reasoning from roles alone drops: a
        group conferring every needed role repairs nothing for a user whose access
        to that resource is revoked.
        """
        policy = fixture.generate(seed, 3)
        groups = sorted({group for _, group in policy.members})
        for resource in policy.resources:
            expected = set()
            for user in policy.users:
                if (resource, ACTION) in access.permissions(user, policy):
                    continue
                if any(holder == user and res == resource for holder, res in policy.revoked):
                    continue
                for group in groups:
                    if (user, group) in policy.members:
                        continue
                    conferred = _closure(
                        {role for granted, role in policy.grants if granted == group},
                        policy.includes,
                    )
                    if any(
                        r in conferred and res == resource and act == ACTION
                        for r, res, act in policy.permits
                    ):
                        expected.add((user, group))
            got = set(truth.single_membership_repairs(resource, ACTION, policy).rows)
            assert got == expected

    def test_nobody_is_repaired_into_a_group_they_are_already_in(self):
        policy = fixture.PINNED
        resource = _subject(truth.repairs_per_resource(ACTION, policy))
        for row in truth.single_membership_repairs(resource, ACTION, policy).rows:
            assert row not in set(policy.members)


class TestJustifyingRoles:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_the_walk_agrees_with_two_reachability_sets(self, seed):
        policy = fixture.generate(seed, 3)
        for resource in policy.resources:
            permitting = {
                role for role, res, act in policy.permits if res == resource and act == ACTION
            }
            expected = set()
            for user in policy.users:
                if (resource, ACTION) not in access.permissions(user, policy):
                    continue
                direct = {
                    role
                    for group in {g for member, g in policy.members if member == user}
                    for granted, role in policy.grants
                    if granted == group
                }
                for role in _closure(direct, policy.includes):
                    if _reachable(role, policy.includes) & permitting:
                        expected.add((user, role))
            got = set(truth.justifying_roles(resource, ACTION, policy).rows)
            assert got == expected

    def test_a_role_that_leads_nowhere_near_the_resource_is_not_on_a_chain(self):
        """The question that separates *the derivation* from *the closure*: a user
        holds roles that have nothing to do with this resource, and they are not
        an answer."""
        policy = fixture.PINNED
        resource = _subject(truth.justifications_per_resource(ACTION, policy))
        on_path = set(truth.justifying_roles(resource, ACTION, policy).rows)
        held = {
            (user, role) for user in policy.users for role in access.effective_roles(user, policy)
        }
        assert on_path < held


_ROLE = st.sampled_from(["a", "b", "c", "d", "e", "f"])
_ORDER = {name: index for index, name in enumerate("abcdef")}


def _trailed_walk(start, permitting, includes):
    """The other reading of *on a chain*: one walk that passes through the role
    without repeating one. Kept here rather than in the oracle — see
    `truth._roles_between` — because the two part company on cycles and the
    question's wording is what a subject is graded against."""
    on_path = set()

    def walk(role, trail):
        trail = trail + [role]
        if role in permitting:
            on_path.update(trail)
        for included in sorted(includes.get(role, ())):
            if included not in trail:
                walk(included, trail)

    for role in sorted(start):
        walk(role, [])
    return on_path


@given(
    start=st.sets(_ROLE, max_size=3),
    permitting=st.sets(_ROLE, max_size=3),
    edges=st.sets(st.tuples(_ROLE, _ROLE), max_size=10),
)
def test_the_two_readings_agree_on_a_hierarchy_without_cycles(start, permitting, edges):
    """Which is the shape `access_control.fixture._chain` builds, and therefore the
    only shape either reading is ever asked about."""
    acyclic = {(a, b) for a, b in edges if _ORDER[a] < _ORDER[b]}
    lookup: dict[str, set[str]] = {}
    for holder, included in acyclic:
        lookup.setdefault(holder, set()).add(included)
    assert truth._roles_between(set(start), set(permitting), lookup) == _trailed_walk(
        set(start), set(permitting), lookup
    )


def test_a_cycle_is_where_the_two_readings_part_company():
    """Pinned so the choice is recorded rather than discovered later. `a -> b -> a`
    with `a` permitting: the walk `a -> b -> a` justifies `b` under the question's
    wording, and no trail without a repeat does."""
    lookup = {"a": {"b"}, "b": {"a"}}
    assert truth._roles_between({"a"}, {"a"}, lookup) == {"a", "b"}
    assert _trailed_walk({"a"}, {"a"}, lookup) == {"a"}


def _subject(sizes: dict[str, int]) -> str:
    """The resource with the most rows — any qualifying one would do here; the
    largest is simply the least likely to make an assertion vacuous."""
    return max(sorted(sizes), key=lambda name: sizes[name])
