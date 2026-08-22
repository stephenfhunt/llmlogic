"""The answer contract. Every leniency here is a failure the run can no longer see."""

from hypothesis import given
from hypothesis import strategies as st

from harness.task import FIELD_SEPARATOR, Answer


def test_parses_one_row_per_line():
    answer = Answer.parse("alice|engineering\nbob|sales\n")
    assert answer is not None
    assert answer.rows == {("alice", "engineering"), ("bob", "sales")}


def test_empty_file_is_an_empty_answer_not_a_parse_failure():
    # "there are none" is the correct answer to a negation question, and the
    # prompt tells both arms to write an empty file for it.
    answer = Answer.parse("")
    assert answer is not None
    assert answer.rows == frozenset()
    assert len(answer) == 0


def test_whitespace_and_comments_only_is_still_an_empty_answer():
    answer = Answer.parse("\n  \n# nothing found\n")
    assert answer is not None
    assert answer.rows == frozenset()


def test_empty_field_is_a_parse_failure():
    assert Answer.parse("alice||engineering\n") is None


def test_fields_are_stripped():
    answer = Answer.parse("  alice |  engineering  \n")
    assert answer is not None
    assert answer.rows == {("alice", "engineering")}


def test_duplicate_rows_collapse():
    answer = Answer.parse("alice\nalice\nalice\n")
    assert answer is not None
    assert answer.rows == {("alice",)}


def test_order_is_not_significant():
    first = Answer.parse("a\nb\nc\n")
    second = Answer.parse("c\na\nb\n")
    assert first is not None and second is not None
    assert first.rows == second.rows


def test_arities_reports_mixed_shapes():
    answer = Answer.parse("alice|engineering\nbob\n")
    assert answer is not None
    assert answer.arities() == {1, 2}


_FIELD = st.text(
    alphabet=st.characters(min_codepoint=97, max_codepoint=122), min_size=1, max_size=8
)


@given(st.sets(st.tuples(_FIELD, _FIELD), min_size=0, max_size=20))
def test_format_then_parse_round_trips(rows):
    # The law: what the harness can write out, it can read back unchanged. If this
    # ever fails, a correct answer can be graded wrong for reasons of punctuation.
    text = "\n".join(FIELD_SEPARATOR.join(row) for row in sorted(rows))
    parsed = Answer.parse(text)
    assert parsed is not None
    assert parsed.rows == rows
