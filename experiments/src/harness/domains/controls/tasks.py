"""The four negative controls."""

from __future__ import annotations

from harness.domains.controls import fixture, truth
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
