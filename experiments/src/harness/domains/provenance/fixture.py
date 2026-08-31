"""The policy graph these questions are asked about, borrowed rather than rebuilt.

`access_control` already generates a policy graph, and its `Policy` is a value the
oracle can be handed — which is the whole reason this pack can reuse it without
either pack tuning a fixture to its own truth. What is *not* borrowed is the
pinned graph: `access_control.fixture.PINNED` has a role hierarchy two levels
deep, and a derivation question over a two-level hierarchy answers with one role.
So this pack pins its own, deeper graph, at its own seed.

**`at-scale` is refused, with a reason.** The oracles here are counterfactual —
they re-derive the closure once per candidate fact — and at 400x the universe
that is a slate that does not build. Scale is a claim about the fact base; this
pack's claim is about the question, and crossing the two would produce an item
that is about neither (`access_control.fixture.AT_SCALE_MAX_DIFFICULTY` makes the
same argument one axis over).
"""

from __future__ import annotations

from harness.domains.access_control import fixture as policy
from harness.domains.access_control.fixture import DIFFICULTY, Policy
from harness.task import Fixture

#: This pack's own seed, so its pinned graph is not the graph four other pinned
#: tasks are asked about — two packs sharing one fixture would make a subject's
#: memory of the first a property of the second.
SEED = 20260831

#: The rung the pinned graph is drawn at. **Depth is why**: `DIFFICULTY[4]` is a
#: five-level role hierarchy, and `justifying_roles` has nothing to say about a
#: hierarchy a grant can cross in one hop.
PINNED_DIFFICULTY = 4

#: The action every question is asked about. One action rather than "any", so a
#: row of the answer names one thing that was derived, not a disjunction.
ACTION = "read"

PINNED: Policy = policy.generate(SEED, PINNED_DIFFICULTY, "in-context")


def generate(seed: int, difficulty: int = 3, track: str = "in-context") -> Policy:
    """A fresh graph, from `access_control`'s generator and nothing else."""
    if track == "at-scale":
        raise ValueError(
            "provenance is an in-context pack: its oracles re-derive the closure "
            "once per candidate fact, which does not survive a 400x universe, and "
            "an item crossing scale with a derivation question is about neither"
        )
    if difficulty not in DIFFICULTY:
        raise ValueError(f"difficulty must be one of {sorted(DIFFICULTY)}")
    return policy.generate(seed, difficulty, track)


def to_fixture(pol: Policy) -> Fixture:
    """The same five CSVs, so a cell cannot be decided by its layout."""
    return policy.to_fixture(pol)


def build() -> Fixture:
    return to_fixture(PINNED)
