"""The one piece of real algorithm in a ``truth.py``, checked independently.

Ground truth is what every verdict in the domain rests on, and nothing else
checks it. The closure gets a property test against a second formulation; the
rest gets concrete assertions about facts that must hold.
"""

from hypothesis import given
from hypothesis import strategies as st

from harness.domains.access_control import fixture, truth


def fixpoint_closure(roles, includes):
    """The same closure, computed by iterating to a fixpoint instead of by BFS.

    Deliberately a different algorithm: two spellings of one search would agree
    on the same bug.
    """
    seen = set(roles)
    changed = True
    while changed:
        changed = False
        for holder, included in includes:
            if holder in seen and included not in seen:
                seen.add(included)
                changed = True
    return seen


_ROLE = st.sampled_from(["a", "b", "c", "d", "e", "f"])


@given(
    roles=st.sets(_ROLE, max_size=4),
    includes=st.sets(st.tuples(_ROLE, _ROLE), max_size=12),
)
def test_closure_agrees_with_an_independent_formulation(roles, includes):
    assert truth.closure(roles, tuple(includes)) == fixpoint_closure(roles, includes)


@given(includes=st.sets(st.tuples(_ROLE, _ROLE), max_size=12))
def test_closure_is_idempotent(includes):
    edges = tuple(includes)
    once = truth.closure({"a"}, edges)
    assert truth.closure(once, edges) == once


@given(includes=st.sets(st.tuples(_ROLE, _ROLE), max_size=12))
def test_closure_contains_what_it_was_given(includes):
    assert {"a", "b"} <= truth.closure({"a", "b"}, tuple(includes))


def test_a_cycle_in_the_role_graph_terminates():
    # Not hypothetical: role hierarchies acquire cycles by accident, and a BFS
    # that revisits would hang the whole run rather than fail one cell.
    cyclic = (("a", "b"), ("b", "c"), ("c", "a"))
    assert truth.closure({"a"}, cyclic) == {"a", "b", "c"}


def test_role_inclusion_is_followed_transitively():
    # admin -> deployer -> writer -> reader, three hops. A single-hop reading of
    # the policy would miss `reader`, which is exactly the error the engine is
    # supposed to prevent.
    assert "reader" in truth.closure({"admin"})


def test_the_isolated_user_really_has_nothing():
    assert truth.effective_roles(fixture.ISOLATED_USER) == set()
    assert truth.permissions(fixture.ISOLATED_USER) == set()


def test_revocation_removes_access_that_a_role_would_otherwise_give():
    revoked_pairs = [(u, r) for u, r in fixture.REVOKED]
    assert revoked_pairs, "the fixture has no revocations; the negation is untested"
    for user, resource in revoked_pairs:
        assert all(res != resource for res, _ in truth.permissions(user))
