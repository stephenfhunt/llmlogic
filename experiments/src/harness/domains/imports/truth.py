"""Ground truth, in plain Python. Control 1: never the datalog engine.

Dates are `datetime.date` here and days are subtracted, not counted by hand —
the point of the domain is that the arms have to do the same thing, and an oracle
that formatted dates as strings and compared them would be answering a different
question from the one asked.
"""

from __future__ import annotations

from collections import Counter
from datetime import date

from harness.domains.imports.fixture import (
    CUSTOMER_ROWS,
    ORDER_ROWS,
    SHIPMENT_ROWS,
)
from harness.task import Answer

#: More than this many days between ordering and shipping is late.
LATE_AFTER_DAYS = 10
#: A region clears this in total order value, missing amounts excluded.
REGION_THRESHOLD = 5_000
#: The quiet quarter the "no orders" question asks about.
QUIET_FROM = date(2026, 3, 1)
QUIET_TO = date(2026, 4, 30)


def region_of(customer: str) -> str:
    return next(region for name, region in CUSTOMER_ROWS if name == customer)


def shipped_on(order_id: str):
    for _, order, shipped in SHIPMENT_ROWS:
        if order == order_id:
            return shipped
    return None


def late_shipments(days: int = LATE_AFTER_DAYS) -> Answer:
    """Orders shipped more than ``days`` after they were ordered.

    An order with no shipment, or a shipment with no date, is not late — it is
    unknown, and the two are different answers.
    """
    late = []
    for order_id, _, ordered_on, _ in ORDER_ROWS:
        shipped = shipped_on(order_id)
        if shipped is not None and (shipped - ordered_on).days > days:
            late.append(order_id)
    return Answer.of(*late)


def region_totals() -> dict[str, int]:
    totals: dict[str, int] = {}
    for _, customer, _, amount in ORDER_ROWS:
        if amount is None:
            continue  # a missing amount is not a zero
        totals[region_of(customer)] = totals.get(region_of(customer), 0) + amount
    return totals


def regions_over(threshold: int = REGION_THRESHOLD) -> Answer:
    return Answer.of(*[region for region, total in region_totals().items() if total > threshold])


def customers_with_no_orders_between(start: date = QUIET_FROM, end: date = QUIET_TO) -> Answer:
    ordered = {customer for _, customer, ordered_on, _ in ORDER_ROWS if start <= ordered_on <= end}
    return Answer.of(*[name for name, _ in CUSTOMER_ROWS if name not in ordered])


def orders_per_region_month() -> dict[str, Counter]:
    counts: dict[str, Counter] = {}
    for _, customer, ordered_on, _ in ORDER_ROWS:
        month = ordered_on.replace(day=1)
        counts.setdefault(region_of(customer), Counter())[month] += 1
    return counts


def busiest_month_per_region() -> Answer:
    """The month each region ordered most in, named by its first day.

    Named by the first day rather than as ``YYYY-MM`` so there is one spelling
    for both arms to produce and none for the grader to normalize.
    """
    rows = []
    for region, months in orders_per_region_month().items():
        ranked = sorted(months.items(), key=lambda item: (-item[1], item[0]))
        if len(ranked) > 1 and ranked[0][1] == ranked[1][1]:
            raise AssertionError(
                f"{region}'s busiest month is a tie — the question has no single "
                "answer, and the fixture seed has to change, not the grader"
            )
        rows.append((region, ranked[0][0].isoformat()))
    return Answer.of(*rows)
