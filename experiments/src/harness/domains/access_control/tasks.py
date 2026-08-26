"""Four questions over the policy graph."""

from __future__ import annotations

from harness.domains.access_control import fixture, truth
from harness.task import Task

DOMAIN = "access_control"


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {"domain": DOMAIN, "fixture": fx}
    return [
        Task(
            id="who-can-read-r03",
            question=(
                "Which users can read resource r03? A user has a role if they are in a "
                "group that is granted it, and a role confers everything the roles it "
                "includes confer, transitively. A revocation removes a user's access to "
                "that resource entirely."
            ),
            truth=truth.users_who_can("read", "r03"),
            question_class="recursion",
            answer_shape=("user",),
            **common,
        ),
        Task(
            id="resources-for-u04",
            question=(
                "Which resources can user u04 access, by any action? Roles come from "
                "group membership and are transitively closed through role inclusion; "
                "revocations remove access to that resource."
            ),
            truth=truth.resources_reachable_by("u04"),
            question_class="recursion",
            answer_shape=("resource",),
            **common,
        ),
        Task(
            id="users-with-no-access",
            question=(
                "Which users have no access to any resource at all? Roles come from "
                "group membership and are transitively closed through role inclusion; "
                "revocations remove access to that resource."
            ),
            truth=truth.users_with_no_access(),
            question_class="negation",
            answer_shape=("user",),
            notes=(
                "Negation over a closed set — 'did I check everyone?' is where hand "
                "reasoning slips, and the answer may legitimately be empty."
            ),
            **common,
        ),
        Task(
            id="delete-without-read",
            question=(
                "Which user and resource pairs are such that the user can delete the "
                "resource but cannot read it? Roles come from group membership and are "
                "transitively closed through role inclusion; revocations remove access "
                "to that resource."
            ),
            truth=truth.can_delete_but_not_read(),
            question_class="negation",
            answer_shape=("user", "resource"),
            notes=(
                "The silent one: a dropped pair looks exactly like a pair that was never there."
            ),
            **common,
        ),
    ]


def _median_by(candidates: tuple[str, ...], sizes: dict[str, int]) -> str:
    """The candidate with the median non-empty answer size.

    ``sizes`` is a precomputed map rather than a callable: calling the oracle per
    candidate is quadratic and stopped an `at-scale` slate from building at all.

    Falls back to the first candidate only if *every* one answers nothing, which
    means the graph itself is degenerate — `validate` is what says so, and it says
    it about the item rather than silently here.
    """
    scored = sorted(((sizes.get(c, 0), c) for c in candidates), key=lambda pair: pair[0])
    non_empty = [c for count, c in scored if count > 0]
    return non_empty[len(non_empty) // 2] if non_empty else candidates[0]


def generated(seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh slate over a fresh policy graph.

    The four questions are the pinned ones re-asked of a new graph, plus a fifth
    the pinned slate cannot ask: *whose access needs the full closure*. That one
    exists because the four cannot distinguish a subject that closed one hop from
    one that closed none — both return a subset, and a subset is what nine of the
    2026-08-24 answers were.

    Nothing here is shipped without passing ``validate``: a generated item whose
    answer is empty, or is everyone, is an item a subject passes without looking.
    """
    policy = fixture.generate(seed, difficulty, track)
    fx = fixture.to_fixture(policy)
    tag = f"{seed:x}"[-4:]
    # Pick the *subject* of each question by its answer, not by its index. A
    # fixed index picked a resource nobody could read on 28 of 1000 generated
    # items, and an empty truth is one a subject passes by writing an empty file.
    # Taking the median non-trivial answer also keeps the item off both extremes
    # without tuning the fixture to it.
    subject_resource = _median_by(policy.resources, truth.readers_per_resource(policy))
    subject_user = _median_by(policy.users, truth.resources_per_user(policy))
    common = {
        "domain": DOMAIN,
        "fixture": fx,
        "track": track,
        "difficulty": difficulty,
    }
    depth = truth.deepest_informative_hop(policy)
    return [
        # The two broad questions are **narrowed on the `at-scale` track**, not
        # dropped. Unscoped over a 400x universe they answer with thousands of
        # rows, and an item like that measures whether a subject can emit a list
        # rather than whether it can compute one. The fact base stays huge and
        # the answer gets small — which is the combination the track is about.
        Task(
            id=f"g{tag}-d{difficulty}-who-can-read",
            question=(
                f"Which users can read resource {subject_resource}? A user has a role "
                "if they are in a group that is granted it, and a role confers "
                "everything the roles it includes confer, transitively. A revocation "
                "removes a user's access to that resource entirely."
            )
            if track == "in-context"
            else (
                f"Which users can read resource {subject_resource} but cannot write "
                "it? A user has a role if they are in a group that is granted it, and "
                "a role confers everything the roles it includes confer, transitively. "
                "A revocation removes a user's access to that resource entirely."
            ),
            truth=(
                truth.users_who_can("read", subject_resource, policy)
                if track == "in-context"
                else truth.users_who_can_but_cannot("read", "write", subject_resource, policy)
            ),
            question_class="recursion" if track == "in-context" else "negation",
            answer_shape=("user",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-resources-for-user",
            question=(
                f"Which resources can user {subject_user} access, by any action? Roles "
                "come from group membership and are transitively closed through role "
                "inclusion; revocations remove access to that resource."
            )
            if track == "in-context"
            else (
                f"Which resources can user {subject_user} delete? Roles come from group "
                "membership and are transitively closed through role inclusion; "
                "revocations remove access to that resource."
            ),
            truth=(
                truth.resources_reachable_by(subject_user, policy)
                if track == "in-context"
                else truth.resources_with_action_for(subject_user, "delete", policy)
            ),
            question_class="recursion",
            answer_shape=("resource",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-no-access",
            question=(
                "Which users have no access to any resource at all, by any action? "
                "Roles come from group membership and are transitively closed through "
                "role inclusion; a revocation removes access to that one resource."
            ),
            truth=truth.users_with_no_access(policy),
            question_class="negation",
            answer_shape=("user",),
            **common,
        ),
        # Scoped to one resource on the `at-scale` track. Unscoped it is a
        # cross-product: at 400x the universe it derived 307,539 rows, and an
        # answer file nobody can write is not a harder question, it is a
        # different one. `in-context` keeps it unscoped, where the large answer
        # set is exactly the silent-subset regime the slate wants.
        Task(
            id=f"g{tag}-d{difficulty}-delete-without-read",
            question=(
                "Which user and resource pairs are ones where the user can delete the "
                "resource but cannot read it? Roles come from group membership and are "
                "transitively closed through role inclusion; a revocation removes every "
                "action on that resource."
            )
            if track == "in-context"
            else (
                f"Which users can delete resource {subject_resource} but cannot read "
                "it? Roles come from group membership and are transitively closed "
                "through role inclusion; a revocation removes every action on that "
                "resource."
            ),
            truth=(
                truth.can_delete_but_not_read(policy)
                if track == "in-context"
                else truth.can_delete_but_not_read_on(subject_resource, policy)
            ),
            question_class="negation",
            answer_shape=("user", "resource") if track == "in-context" else ("user",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-needs-the-full-closure",
            question=(
                "Which users hold at least one role that is only reachable by following "
                f"role inclusion {depth} or more times from a role their group is "
                "granted directly? Count a directly granted role as zero steps."
            ),
            truth=truth.users_reachable_only_through(depth, policy),
            question_class="recursion",
            answer_shape=("user",),
            **common,
        ),
    ]


class Degenerate(Exception):
    """A generated item a subject could pass without doing the work."""


def validate(task: Task) -> None:
    """Refuse an item that carries no information.

    Three ways a generated item is worthless, all of which the pinned slate
    checks for by hand in `tests/test_controls_hold.py` and none of which a
    generator gets for free:

    - an **empty** truth is passed by writing an empty file without looking;
    - a truth that is the **whole universe** is passed by copying a column;
    - a truth of **one row** over a large universe is close enough to guessable,
      and carries almost nothing about whether rows were dropped.

    Raised rather than filtered, because a generator that silently drops a third
    of its items is a generator whose difficulty setting no longer means what it
    says.
    """
    rows = task.truth.rows
    if not rows:
        raise Degenerate(f"{task.id}: empty truth — doing nothing scores correct")
    if len(task.answer_shape) == 1:
        universe = {
            field.strip()
            for name, contents in task.fixture.files.items()
            if isinstance(contents, str) and name.endswith(".csv")
            for line in contents.splitlines()[1:]
            for field in line.split(",")
        }
        answered = {row[0] for row in rows}
        if universe and answered == universe:
            raise Degenerate(f"{task.id}: the answer is everything — copying a column passes")
    if len(rows) == 1 and len(task.fixture.files) > 1:
        raise Degenerate(f"{task.id}: a single-row answer is close to guessable")
