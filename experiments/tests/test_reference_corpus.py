"""The reference corpus, against the engine and against the oracles.

Two things are checked per correct entry, from one engine run:

1. **It still answers what the domain's ``truth.py`` says.** This is what makes
   the corpus a *reference* rather than a snapshot — a program can be pinned and
   wrong, and pinning a wrong program is worse than having no corpus, because the
   pin then defends the error.
2. **It prints byte-for-byte what it printed before.** The harness measures an
   agent against an engine that moves under it, and two grid runs are the same
   measurement only if the engine answered the same way in between.

A malformed entry checks the mirror: the diagnostic is pinned, stdout is empty,
and the exit code is non-zero. Most of what a subject reads while getting its
program right is diagnostics, so a corpus of correct programs alone would pin the
half of the tool the subject sees least.

**A pin diff is not automatically a regression.** It is a change in the
instrument. Look at it, decide, and re-pin deliberately — never by regenerating
because the test went red.
"""

import tempfile
from pathlib import Path

import pytest

from harness import domains, reference
from harness.arms import DATALOG_BIN_DIR
from harness.corpus import SQLPARSE

pytestmark = pytest.mark.skipif(
    not (DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; `cargo build --release --offline` in datalog/",
)

#: ``task id -> the relation in the reference program that answers it``.
#:
#: Written out rather than derived. A task and a relation are named by different
#: people for different reasons, and a convention tying them together would turn
#: renaming a relation into silently unchecking a task.
ANSWERS: dict[str, dict[str, str]] = {
    "access_control": {
        "who-can-read-r03": "reader_r03",
        "resources-for-u04": "resource_u04",
        "users-with-no-access": "no_access",
        "delete-without-read": "delete_not_read",
    },
    "ontology": {
        "instances-of-sensor": "sensors",
        "power-profile": "profile",
        "inconsistent-instances": "inconsistent",
        "classes-with-no-instances": "empty_class",
    },
    "imports": {
        "late-shipments": "late",
        "regions-over-threshold": "big_region",
        "customers-with-no-recent-orders": "quiet",
        "busiest-month-per-region": "busiest",
    },
    "eligibility": {
        "eligible": "eligible",
        "blocked-by-exactly-one": "blocked_by_one",
        "undetermined": "undetermined",
        "households-over-dependants": "big_household",
    },
    "scheduling": {
        "double-booked": "double_booked",
        "unstaffable-shifts": "unstaffable",
        "forced-assignments": "forced",
        "rest-violations": "rest_violation",
    },
    "static_analysis": {
        "never-called-functions": "never_called",
        "modules-outside-any-import-cycle": "outside",
        "token-subclasses": "sub_token",
        "functions-called-from-several-modules": "widely_used",
    },
    "controls": {
        "department-of": "carol_department",
        "customer-of-order": "o3_customer",
        "orders-above-100": "large_order",
        "engineering-headcount": "engineering_headcount",
    },
}


def _execute(entry):
    fixture = None
    if entry.domain:
        tasks = domains.load_all([entry.domain])
        if not tasks:
            pytest.skip(f"{entry.domain} unavailable")
        fixture = tasks[0].fixture
    with tempfile.TemporaryDirectory() as scratch:
        workdir = Path(scratch)
        reference.materialize(entry, fixture, workdir)
        return reference.run(entry, workdir)


# ---- The corpus covers the slate ---------------------------------------------


def test_every_domain_has_a_reference_program():
    have = {entry.name for entry in reference.correct()}
    want = {task.domain for task in domains.load_all()}
    assert want <= have, f"no reference program for {sorted(want - have)}"


def test_every_task_has_a_relation_that_answers_it():
    # The mapping is hand-written, so this is what stops a task being added
    # without a reference answer and nobody noticing.
    for task in domains.load_all():
        mapped = ANSWERS.get(task.domain, {})
        assert task.id in mapped, f"{task.key} has no relation in ANSWERS"


def test_the_mapping_names_no_task_that_does_not_exist():
    real = {task.key for task in domains.load_all()}
    available = {task.domain for task in domains.load_all()}
    for domain, mapped in ANSWERS.items():
        if domain not in available:
            continue  # a pack whose corpus is not fetched
        for task_id in mapped:
            assert f"{domain}/{task_id}" in real, f"ANSWERS names {domain}/{task_id}, which is gone"


# ---- Correct programs ---------------------------------------------------------


@pytest.mark.parametrize("entry", reference.correct(), ids=lambda e: e.name)
def test_a_correct_program_answers_its_domain_and_prints_what_it_printed(entry):
    if entry.name == "static_analysis" and not SQLPARSE.present():
        pytest.skip(f"{SQLPARSE.slug} not fetched; run `harness corpus fetch`")

    result = _execute(entry)
    assert result.exit_code == 0, f"{entry.name} exited {result.exit_code}:\n{result.stderr}"

    # 1 — the answers are the oracle's, task by task.
    relations = reference.parse_facts(result.stdout)
    tasks = {task.id: task for task in domains.load_all([entry.domain])}
    for task_id, relation in ANSWERS[entry.domain].items():
        expected = tasks[task_id].truth.rows
        got = relations.get(relation, set())
        assert got == set(expected), (
            f"{entry.domain}/{task_id}: `{relation}` disagrees with truth.py — "
            f"missing {sorted(set(expected) - got)}, extra {sorted(got - set(expected))}"
        )

    # 2 — and it printed exactly what it printed last time.
    assert result.stdout == entry.stdout, f"{entry.name} stdout moved; see this file's docstring"
    assert result.stderr == entry.stderr, f"{entry.name} stderr moved; see this file's docstring"


def test_a_reference_program_answers_every_one_of_its_tasks():
    # A program that defines three of its four relations would pass the loop
    # above for the three and never be asked about the fourth.
    for entry in reference.correct():
        source = entry.source
        for relation in ANSWERS[entry.name].values():
            assert f"{relation}(" in source, f"{entry.name}.dl never defines `{relation}`"
            assert f"?- {relation}(" in source, f"{entry.name}.dl never queries `{relation}`"


# ---- Malformed programs -------------------------------------------------------


@pytest.mark.parametrize("entry", reference.malformed(), ids=lambda e: e.name)
def test_a_malformed_program_fails_with_the_diagnostic_it_had(entry):
    result = _execute(entry)
    assert result.exit_code != 0, f"{entry.name} was expected to fail and exited 0"
    assert result.stdout == "", (
        f"{entry.name} printed answers before failing:\n{result.stdout}\n"
        "A program that does not typecheck should derive nothing."
    )
    assert result.stderr == entry.stderr, f"{entry.name} stderr moved; see this file's docstring"


def test_the_malformed_half_is_not_empty():
    # The half that is easy to skip: a corpus of correct programs cannot pin what
    # the tool does when a run goes wrong, which is most of what a subject reads.
    assert len(reference.malformed()) >= 4
