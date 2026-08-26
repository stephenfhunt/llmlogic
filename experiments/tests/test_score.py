"""Grading a program rather than an agent.

Three outcomes have to stay distinct, because they are three different things to
do next: the engine **rejected** it (repair from the diagnostic — the whole of
what S3 claims), it ran and derived the **wrong** rows (repair the logic), and it
was **correct**. Collapsing any two of them into a boolean throws away the signal.

The oracle is still the domain's plain-Python `truth.py`, never the engine
(control 1).
"""

from __future__ import annotations

import pytest

from harness import arms, domains
from harness.score import score

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(), reason="datalog binary not built"
)

PREAMBLE = """\
import "member.csv" as member.
import "grant.csv" as grant.
import "includes.csv" as includes.
import "permit.csv" as permit.
import "revoked.csv" as revoked.

has_role(U, R) :- member(user: U, group: G), grant(group: G, role: R).
has_role(U, S) :- has_role(U, R), includes(role: R, included_role: S).

can(U, Res, A) :- has_role(U, R),
                  permit(role: R, resource: Res, action: A),
                  not revoked(user: U, resource: Res).
"""


def _task(task_id: str):
    return next(t for t in domains.load_all(["access_control"]) if t.id == task_id)


def test_a_correct_program_scores_correct(tmp_path):
    task = _task("who-can-read-r03")
    result = score(task, PREAMBLE + '?- reader: can(U, "r03", "read").', tmp_path)
    assert result.accepted
    assert result.correct
    assert (result.missing, result.extra) == (0, 0)
    assert result.derived is not None and result.derived.rows == task.truth.rows


def test_a_rejected_program_is_not_a_wrong_answer(tmp_path):
    """Exit 2 and up is *did not answer*. It carries a diagnostic to act on,
    which a wrong answer does not."""
    result = score(_task("who-can-read-r03"), "p(X) :- q(Y).", tmp_path)
    assert not result.accepted
    assert result.exit_code >= 2
    assert result.derived is None
    assert "unsafe" in result.stderr


def test_a_program_that_runs_and_derives_the_wrong_rows(tmp_path):
    result = score(
        _task("who-can-read-r03"), PREAMBLE + '?- reader: can(U, "r03", "write").', tmp_path
    )
    assert result.accepted
    assert not result.correct
    assert result.missing > 0


def test_deriving_nothing_is_accepted_and_graded_not_rejected(tmp_path):
    """The engine exits 1 when every query ran and none answered. That is a
    *result* — negation over a closed set is a whole question class — so it must
    grade, not read as a rejection."""
    result = score(
        _task("who-can-read-r03"), PREAMBLE + '?- reader: can(U, "nope", "read").', tmp_path
    )
    assert result.accepted
    assert result.exit_code == 1
    assert not result.correct
    assert result.missing == len(_task("who-can-read-r03").truth.rows)


def test_a_named_relation_is_picked_out_of_several(tmp_path):
    """A program that publishes more than one relation is not ambiguous once the
    caller says which one it means."""
    program = PREAMBLE + ('?- reader: can(U, "r03", "read").\n?- writer: can(U, "r03", "write").\n')
    task = _task("who-can-read-r03")
    assert score(task, program, tmp_path, relation="reader").correct
    assert not score(task, program, tmp_path, relation="writer").correct


def test_several_relations_and_no_name_refuses_to_guess(tmp_path):
    """Guessing would pick the flattering one often enough to matter."""
    program = PREAMBLE + ('?- reader: can(U, "r03", "read").\n?- writer: can(U, "r03", "write").\n')
    result = score(_task("who-can-read-r03"), program, tmp_path)
    assert result.accepted
    assert result.derived is None
    assert "which relation" in result.stderr


def test_a_program_that_never_terminates_is_killed_by_us(tmp_path):
    """The engine has no timeout, by decision. This is the harness's bound, and
    without it a value-creating recursion costs a session rather than a minute."""
    program = "n(1).\nn(X) :- n(Y), X = Y + 1.\n?- big: n(X).\n"
    result = score(_task("who-can-read-r03"), program, tmp_path, timeout=3)
    assert result.timed_out
    assert not result.accepted
    assert "timed out" in result.stderr
