"""The four negative controls."""

from __future__ import annotations

from harness.domains.controls import fixture, truth
from harness.generate import Degenerate
from harness.task import Task

DOMAIN = "controls"


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {
        "domain": DOMAIN,
        "fixture": fx,
        "engine_expected_to_help": False,
    }
    return [
        Task(
            id="department-of",
            question="Which department is carol in?",
            truth=truth.department_of("carol"),
            question_class="single-hop",
            answer_shape=("department",),
            notes=(
                "One lookup in six rows. A model that needs an engine for this "
                "is not the model we ship to."
            ),
            **common,
        ),
        Task(
            id="customer-of-order",
            question="Which customer placed order o3?",
            truth=truth.customer_of("o3"),
            question_class="single-hop",
            answer_shape=("customer",),
            **common,
        ),
        Task(
            id="orders-above-100",
            question="Which orders have an amount greater than 100?",
            truth=truth.orders_above(),
            question_class="one-step",
            answer_shape=("order_id",),
            notes=(
                "Six comparisons. The engine's arithmetic is exact, but so is "
                "the model's at this size."
            ),
            **common,
        ),
        Task(
            id="engineering-headcount",
            question="How many employees are in the engineering department?",
            truth=truth.headcount("engineering"),
            question_class="one-step",
            answer_shape=("count",),
            **common,
        ),
    ]


def generated(seed: int, difficulty: int = 1, track: str = "in-context") -> list[Task]:
    """A fresh set of the same four negative controls.

    **Four, not five.** Every other pack's generator adds a question the pinned
    slate cannot ask, because every wrong answer to its four is a subset of the
    right one and their shape cannot say which mistake was made. Here there is no
    mistake to distinguish: these are questions the subject gets right in its
    head, and the answer that matters is the *rate*, not the shape.

    A generated slate needs controls of matching provenance, or a calibrated
    slate has no null to read against.
    """
    books = fixture.generate(seed, difficulty, track)
    fx = fixture.to_fixture(books)
    tag = f"{seed:x}"[-4:]
    common = {
        "domain": DOMAIN,
        "fixture": fx,
        "engine_expected_to_help": False,
        "track": track,
        "difficulty": difficulty,
    }
    return [
        Task(
            id=f"g{tag}-d{difficulty}-department-of",
            question=f"Which department is {books.subject_employee} in?",
            truth=truth.department_of(books.subject_employee, books),
            question_class="single-hop",
            answer_shape=("department",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-customer-of-order",
            question=f"Which customer placed order {books.subject_order}?",
            truth=truth.customer_of(books.subject_order, books),
            question_class="single-hop",
            answer_shape=("customer",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-orders-above",
            question=(f"Which orders have an amount greater than {books.high_order_threshold}?"),
            truth=truth.orders_above(books=books),
            question_class="one-step",
            answer_shape=("order_id",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-headcount",
            question=(f"How many employees are in the {books.subject_department} department?"),
            truth=truth.headcount(books.subject_department, books),
            question_class="one-step",
            answer_shape=("count",),
            **common,
        ),
    ]


#: The largest a control's fact base may get. A model that needs an engine to
#: read twenty rows is not the model we ship to; a model reading a thousand is
#: doing retrieval, which is a different question from the one the null answers.
MAX_ROWS = 48


def check(task: Task) -> None:
    """This pack's own invariants: that these are still *controls*.

    The failure mode is drift in one direction only — a control that quietly
    grows into a measured item. If the engine arm wins here, the harness is
    measuring something other than reasoning, and that reading is only available
    while the questions stay ones a model answers in its head.
    """
    if task.engine_expected_to_help:
        raise Degenerate(f"{task.id}: a control that expects the engine to help is not a control")
    if task.question_class not in ("single-hop", "one-step"):
        raise Degenerate(f"{task.id}: {task.question_class} is not a control's question class")
    rows = sum(
        len(contents.splitlines()) - 1
        for name, contents in task.fixture.files.items()
        if isinstance(contents, str) and name.endswith(".csv")
    )
    if rows > MAX_ROWS:
        raise Degenerate(
            f"{task.id}: {rows} rows — past {MAX_ROWS} this stops being a question a "
            "model answers in its head and starts being one about retrieval"
        )
