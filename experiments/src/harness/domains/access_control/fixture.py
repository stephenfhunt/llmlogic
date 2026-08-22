"""A policy graph, generated from a fixed seed.

Generated rather than hand-written so the shape is not quietly tuned to be easy,
and seeded so it is the same graph on every run. Sized to stay well inside the
smaller strength's context: the prose arm has to hold this in its head, and a
fixture that defeats it on context alone would measure the context window.
"""

from __future__ import annotations

import random

from harness.task import Fixture

SEED = 20260821

USERS = tuple(f"u{i:02d}" for i in range(1, 13))
GROUPS = ("eng", "sre", "support", "finance", "contractors", "leads")
#: A group granted nothing, with exactly one member. The "who has no access
#: at all?" question needs a non-empty answer, or a subject that writes an
#: empty file without looking scores correct. Kept out of GROUPS so nothing
#: else is assigned to it and no grant is generated for it.
UNGRANTED_GROUP = "quarantine"
ISOLATED_USER = "u12"
ROLES = ("admin", "deployer", "reader", "writer", "auditor", "billing", "oncall", "guest")
RESOURCES = tuple(f"r{i:02d}" for i in range(1, 11))
ACTIONS = ("read", "write", "delete")

#: A role hierarchy: including a role confers everything that role confers.
#: Three levels deep, so the closure is not a single hop.
INCLUDES: tuple[tuple[str, str], ...] = (
    ("admin", "deployer"),
    ("admin", "auditor"),
    ("deployer", "writer"),
    ("writer", "reader"),
    ("oncall", "deployer"),
    ("billing", "reader"),
)


def _build():
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
    return tuple(members), tuple(grants), tuple(permits), tuple(revoked)


MEMBERS, GRANTS, PERMITS, REVOKED = _build()


def build() -> Fixture:
    member_csv = "user,group\n" + "".join(f"{u},{g}\n" for u, g in MEMBERS)
    grant_csv = "group,role\n" + "".join(f"{g},{r}\n" for g, r in GRANTS)
    includes_csv = "role,included_role\n" + "".join(f"{a},{b}\n" for a, b in INCLUDES)
    permit_csv = "role,resource,action\n" + "".join(f"{r},{res},{a}\n" for r, res, a in PERMITS)
    revoked_csv = "user,resource\n" + "".join(f"{u},{r}\n" for u, r in REVOKED)
    return Fixture(
        files={
            "member.csv": member_csv,
            "grant.csv": grant_csv,
            "includes.csv": includes_csv,
            "permit.csv": permit_csv,
            "revoked.csv": revoked_csv,
        },
        schemas={
            "member": ("user", "group"),
            "grant": ("group", "role"),
            "includes": ("role", "included_role"),
            "permit": ("role", "resource", "action"),
            "revoked": ("user", "resource"),
        },
    )
