"""Three questions about the derivation, over one policy graph.

The subject resource is picked **by its answer**, as everywhere else in the
slate, and with one extra filter: a resource whose answer is a single row is not
a candidate. `generate.validate` would reject such an item anyway — a one-row
answer over a large universe is close to guessable — and a derivation question
degenerates into a lookup long before it degenerates into a guess.
"""

from __future__ import annotations

from harness.domains.provenance import fixture, truth
from harness.generate import Degenerate, pick_by_median
from harness.task import Task

DOMAIN = "provenance"

#: The smallest answer a question here may have. Two, because the questions are
#: about *which* fact among several, and a single-row answer names no alternative
#: to have picked between.
MIN_ROWS = 2

#: How the three questions are worded, minus their subject. Written once and
#: shared by the pinned and generated slates: two spellings of one question is
#: how a generated item comes to measure something the pinned one does not.
CLOSURE = (
    "Roles come from group membership and are transitively closed through role "
    "inclusion; a revocation removes every action on that resource for that user."
)


def _subject(candidates: tuple[str, ...], sizes: dict[str, int], question: str) -> str:
    """The resource this question is asked about: the median of those it can be
    asked about at all.

    Median rather than maximum, for `pick_by_median`'s own reason — the extreme
    is where a fixture gets tuned to its question — and over the *qualifying*
    resources rather than all of them, so the median cannot land on a resource
    whose answer is one row.
    """
    qualifying = {name: count for name, count in sizes.items() if count >= MIN_ROWS}
    if not qualifying:
        raise Degenerate(
            f"{DOMAIN}: no resource has {MIN_ROWS} or more rows for {question} — "
            "this graph cannot ask the question"
        )
    return pick_by_median(tuple(sorted(qualifying)), qualifying)


def _questions(pol, tag: str = "", difficulty: int = 0, track: str = "in-context") -> list[Task]:
    """The three tasks over one policy graph, pinned or generated."""
    fx = fixture.to_fixture(pol)
    action = fixture.ACTION
    prefix = f"g{tag}-d{difficulty}-" if tag else ""
    common = {
        "domain": DOMAIN,
        "fixture": fx,
        "question_class": "provenance",
        "track": track,
        "difficulty": difficulty,
    }
    resources = tuple(pol.resources)

    critical = _subject(
        resources, truth.critical_grants_per_resource(action, pol), "critical-grant"
    )
    repair = _subject(resources, truth.repairs_per_resource(action, pol), "minimal-repair")
    path = _subject(resources, truth.justifications_per_resource(action, pol), "access-path")

    return [
        Task(
            id=f"{prefix}critical-grant",
            question=(
                f"Which user, group and role triples are ones where that group's grant "
                f"of that role is the only thing giving that user {action} access to "
                f"{critical}? Equivalently: deleting that one row of grant.csv would "
                f"take that user's {action} access to {critical} away, and every other "
                f"user keeps whatever they had. {CLOSURE}"
            ),
            truth=truth.critical_grants(critical, action, pol),
            answer_shape=("user", "group", "role"),
            notes=(
                "The `?why` question asked of the fact base: a user can hold a "
                "permission by two routes, and then no single grant is critical for "
                "them. The extension does not say which case a row is in."
            ),
            **common,
        ),
        Task(
            id=f"{prefix}minimal-repair",
            question=(
                f"Which user and group pairs are ones where adding that user to that "
                f"group would give them {action} access to {repair}, which they do not "
                f"have now? The user must not already be in the group. {CLOSURE}"
            ),
            truth=truth.single_membership_repairs(repair, action, pol),
            answer_shape=("user", "group"),
            notes=(
                "The `?whynot` question. A group conferring the right roles is not a "
                "repair for a user whose revocation stands, which is the row a subject "
                "reasoning from the hierarchy alone adds."
            ),
            **common,
        ),
        Task(
            id=f"{prefix}access-path",
            question=(
                f"For each user who can {action} {path}, which roles lie on a chain "
                f"that confers it? A role is on such a chain when the user holds it "
                f"directly through a grant, or it is reachable from a directly held "
                f"role through role inclusion, and a role permitting {action} on "
                f"{path} is reachable from it — that permitting role included. Roles "
                f"the user holds that lead nowhere near {path} are not on a chain. "
                f"Answer with user and role pairs. {CLOSURE}"
            ),
            truth=truth.justifying_roles(path, action, pol),
            answer_shape=("user", "role"),
            notes=(
                "The proof tree, flattened. Every other pack asks for the extension; "
                "this asks which roles a derivation of it would name."
            ),
            **common,
        ),
    ]


def tasks() -> list[Task]:
    return _questions(fixture.PINNED)


def generated(seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh graph, the same three questions."""
    pol = fixture.generate(seed, difficulty, track)
    return _questions(pol, tag=f"{seed:x}"[-4:], difficulty=difficulty, track=track)


def check(task: Task) -> None:
    """This pack's own invariants, read off the files the subject is given.

    Each question has one: a critical grant must be a grant, a repair must be a
    membership that is *not* there, and a role on a chain must be one the user
    can actually reach. All three are the ways a plausible-looking oracle would
    be wrong while still agreeing with itself.
    """
    suffix = task.id.split("-", 2)[2] if task.id.startswith("g") else task.id
    rows = task.truth.rows

    if suffix == "critical-grant":
        grants = _rows(task, "grant.csv")
        for _user, group, role in rows:
            if (group, role) not in grants:
                raise Degenerate(f"{task.id}: {group}/{role} is critical but is not a grant")
    elif suffix == "minimal-repair":
        members = _rows(task, "member.csv")
        for user, group in rows:
            if (user, group) in members:
                raise Degenerate(f"{task.id}: {user} is already in {group} — not a repair")
    elif suffix == "access-path":
        roles = {role for role, _, _ in _triples(task, "permit.csv")}
        included = {b for _, b in _rows(task, "includes.csv")}
        known = roles | included | {role for _, role in _rows(task, "grant.csv")}
        for _user, role in rows:
            if role not in known:
                raise Degenerate(f"{task.id}: {role} is on a chain but appears in no relation")


def _rows(task: Task, filename: str) -> set[tuple[str, str]]:
    return {
        (a, b)
        for a, b in (
            line.split(",") for line in task.fixture.text(filename).splitlines()[1:] if line.strip()
        )
    }


def _triples(task: Task, filename: str) -> list[tuple[str, str, str]]:
    return [
        (a, b, c)
        for a, b, c in (
            line.split(",") for line in task.fixture.text(filename).splitlines()[1:] if line.strip()
        )
    ]
