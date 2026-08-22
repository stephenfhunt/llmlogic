"""Recognising engine use, and the first program.

Every case here is one the harness must not get wrong: control 4 records the
program written *before any feedback*, so a false positive replaces the
measurement rather than adding noise to it.

The first two cases are not hypothetical. They are what the first real cell
actually did.
"""

from harness.engine_use import (
    invokes_engine,
    program_from_call,
    program_in_command,
    uses_engine,
)


def test_listing_the_skill_directory_is_not_running_the_engine():
    # Observed on the first real cell: the subject ran this, and a substring test
    # recorded it as the first Datalog program the model wrote.
    command = "ls -la /ws/.claude/skills/datalog/"
    assert invokes_engine(command) is False
    assert program_in_command(command) is None


def test_reading_the_skill_file_is_not_running_the_engine():
    assert invokes_engine("cat /ws/.claude/skills/datalog/SKILL.md") is False
    assert invokes_engine("grep -r datalog /ws") is False


def test_running_the_binary_is_running_the_engine():
    assert invokes_engine("datalog facts.dl")
    assert invokes_engine("/ws/bin/datalog facts.dl")
    assert invokes_engine("cd /ws && datalog facts.dl")
    assert invokes_engine("cat facts.dl | datalog -")
    assert invokes_engine("RUST_LOG=info datalog facts.dl")


def test_an_inline_query_is_the_program():
    program = program_in_command("datalog facts.dl -q 'reaches(X, \"r03\")'")
    assert program == 'reaches(X, "r03")'


def test_a_piped_program_is_the_program():
    # The shape the first real cell used: no .dl file was ever written.
    command = "echo 'can(U) :- member(U, G), grant(G, R).' | datalog -"
    program = program_in_command(command)
    assert program is not None
    assert ":-" in program


def test_a_bare_file_run_carries_no_source_of_its_own():
    # The program is in the file; the file-write path is what records it.
    assert program_in_command("datalog facts.dl") is None


def test_a_written_dl_file_is_the_program():
    program = program_from_call("Write", {"file_path": "/ws/q.dl", "content": "p(X) :- q(X)."})
    assert program == "p(X) :- q(X)."


def test_writing_the_answer_is_not_writing_a_program():
    assert program_from_call("Write", {"file_path": "/ws/answer.txt", "content": "u03\n"}) is None


def test_reading_a_fixture_is_not_writing_a_program():
    assert program_from_call("Read", {"file_path": "/ws/member.csv"}) is None


def test_invoking_the_skill_counts_as_reaching_for_the_engine():
    # The strongest form of "reached for it", and what the first real cell did.
    assert uses_engine("Skill", {"command": "datalog"})
    assert uses_engine("Skill", {"command": "dataviz"}) is False


def test_a_python_script_is_not_engine_use():
    # The prose arm's honest alternative. Counting it would erase the comparison.
    assert uses_engine("Bash", {"command": "python3 solve.py"}) is False


def test_an_unbalanced_quote_does_not_crash_the_detector():
    # Heredocs and stray quotes are normal in agent transcripts; a parser error
    # here would lose the cell.
    assert invokes_engine("datalog <<'EOF'\np(X) :- q(X).\nEOF") is True
