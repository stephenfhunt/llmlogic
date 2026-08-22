"""Four questions over the three tables."""

from __future__ import annotations

from harness.domains.imports import fixture, truth
from harness.task import Task

DOMAIN = "imports"

#: Said in every question that touches an amount. The rule has to be *stated*, or
#: the two arms are answering two different questions and the grader cannot tell
#: which — the point is whether they apply it, not whether they guess it.
MISSING = "Some orders have no amount recorded; those orders are not counted."


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {"domain": DOMAIN, "fixture": fx}
    return [
        Task(
            id="late-shipments",
            question=(
                f"Which orders shipped more than {truth.LATE_AFTER_DAYS} days after "
                "they were ordered? `shipment.jsonl` links a shipment to its order. "
                "An order that never shipped, or whose shipment has no date, is not "
                "late."
            ),
            truth=truth.late_shipments(),
            question_class="temporal",
            answer_shape=("order",),
            notes=(
                "Date arithmetic across a join. A run that compares the dates as "
                "text gets the same answer here and a different one at a year "
                "boundary, which is what makes it a plausible wrong method."
            ),
            **common,
        ),
        Task(
            id="regions-over-threshold",
            question=(
                f"Which regions have a total order amount above {truth.REGION_THRESHOLD}? "
                "A customer's region is in `customer.csv`, and each order belongs to a "
                f"customer. {MISSING}"
            ),
            truth=truth.regions_over(),
            question_class="aggregation",
            answer_shape=("region",),
            notes=(
                "The missing amounts decide this one: read as zeros they change no "
                "total, but read as anything else they change two."
            ),
            **common,
        ),
        Task(
            id="customers-with-no-recent-orders",
            question=(
                "Which customers placed no order between "
                f"{truth.QUIET_FROM.isoformat()} and {truth.QUIET_TO.isoformat()} "
                "inclusive? List every such customer, including any that never "
                "ordered at all."
            ),
            truth=truth.customers_with_no_orders_between(),
            question_class="negation",
            answer_shape=("customer",),
            notes=(
                "Negation inside a date window — 'did I check every customer, and "
                "every one of their orders?' is where a hand reading slips."
            ),
            **common,
        ),
        Task(
            id="busiest-month-per-region",
            question=(
                "For each region, which month did its customers place the most orders "
                "in? Count every order, whether or not it has an amount. Identify the "
                "month by its first day, as YYYY-MM-DD. Each region has a single "
                "busiest month."
            ),
            truth=truth.busiest_month_per_region(),
            question_class="aggregation",
            answer_shape=("region", "month"),
            notes=(
                "Grouping on a derived key — the month is not a column, it has to be "
                "computed from the date before anything can be counted per group."
            ),
            **common,
        ),
    ]
