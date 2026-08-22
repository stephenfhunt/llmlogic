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
