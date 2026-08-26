"""Ground truth, in plain Python. Control 1: never the datalog engine.

Dates are `datetime.date` here and days are subtracted, not counted by hand —
the point of the domain is that the arms have to do the same thing, and an oracle
that formatted dates as strings and compared them would be answering a different
question from the one asked.

Every function takes the ``Ledger`` it is asked about, defaulting to the pinned
one. That is what lets the oracle be checked against a second formulation on a
*generated* ledger: an oracle that can only run against the single ledger it was
written for can only ever be checked against that ledger's own answers, which is
how a fixture comes to be tuned to its truth.
"""

from __future__ import annotations

from collections import Counter, defaultdict
from datetime import date
from functools import lru_cache

from harness.domains.imports.fixture import PINNED, Ledger
from harness.task import Answer

#: The pinned ledger's thresholds, under the names the four pinned questions
#: quote. A generated ledger carries its own.
LATE_AFTER_DAYS = PINNED.late_after_days
REGION_THRESHOLD = PINNED.region_threshold
QUIET_FROM = PINNED.quiet_from
QUIET_TO = PINNED.quiet_to


@lru_cache(maxsize=8)
def _index(ledger: Ledger):
    """``(region of customer, shipped date by order)``, built once.

    ``Ledger`` is frozen with tuple fields, so it hashes; a different ledger is
    a different cache key. The obvious spelling — scan `SHIPMENT_ROWS` inside a
    loop over orders — is fine on 54 orders and quadratic at `at-scale` sizes.
    """
    region_of = dict(ledger.customers)
    shipped: dict[str, date | None] = {}
    for _, order, on in ledger.shipments:
        shipped.setdefault(order, on)
    return region_of, shipped


def region_of(customer: str, ledger: Ledger = PINNED) -> str:
    return _index(ledger)[0][customer]


def shipped_on(order_id: str, ledger: Ledger = PINNED):
    return _index(ledger)[1].get(order_id)


def late_shipments(days: int | None = None, ledger: Ledger = PINNED) -> Answer:
    """Orders shipped more than ``days`` after they were ordered.

    An order with no shipment, or a shipment with no date, is not late — it is
    unknown, and the two are different answers.
    """
    limit = ledger.late_after_days if days is None else days
    shipped = _index(ledger)[1]
    late = []
    for order_id, _, ordered_on, _ in ledger.orders:
        on = shipped.get(order_id)
        if on is not None and (on - ordered_on).days > limit:
            late.append(order_id)
    return Answer.of(*late)


def late_shipments_for(customer: str, ledger: Ledger = PINNED) -> Answer:
    """The same, scoped to one customer — the `at-scale` shape, where the
    unscoped answer is thousands of order ids."""
    shipped = _index(ledger)[1]
    return Answer.of(
        *[
            order_id
            for order_id, who, ordered_on, _ in ledger.orders
            if who == customer
            and (on := shipped.get(order_id)) is not None
            and (on - ordered_on).days > ledger.late_after_days
        ]
    )


def region_totals(ledger: Ledger = PINNED) -> dict[str, int]:
    region = _index(ledger)[0]
    totals: dict[str, int] = dict.fromkeys(ledger.regions, 0)
    for _, customer, _, amount in ledger.orders:
        if amount is None:
            continue  # a missing amount is not a zero
        totals[region[customer]] = totals.get(region[customer], 0) + amount
    return totals


def regions_over(threshold: int | None = None, ledger: Ledger = PINNED) -> Answer:
    limit = ledger.region_threshold if threshold is None else threshold
    return Answer.of(*[region for region, total in region_totals(ledger).items() if total > limit])


def customers_with_no_orders_between(
    start: date | None = None, end: date | None = None, ledger: Ledger = PINNED
) -> Answer:
    first = ledger.quiet_from if start is None else start
    last = ledger.quiet_to if end is None else end
    ordered = {
        customer for _, customer, ordered_on, _ in ledger.orders if first <= ordered_on <= last
    }
    return Answer.of(*[name for name, _ in ledger.customers if name not in ordered])


def orders_per_region_month(ledger: Ledger = PINNED) -> dict[str, Counter]:
    region = _index(ledger)[0]
    counts: dict[str, Counter] = {}
    for _, customer, ordered_on, _ in ledger.orders:
        month = ordered_on.replace(day=1)
        counts.setdefault(region[customer], Counter())[month] += 1
    return counts


def busiest_month_per_region(ledger: Ledger = PINNED) -> Answer:
    """The month each region ordered most in, named by its first day.

    Named by the first day rather than as ``YYYY-MM`` so there is one spelling
    for both arms to produce and none for the grader to normalize.
    """
    rows = []
    for region, months in orders_per_region_month(ledger).items():
        ranked = sorted(months.items(), key=lambda item: (-item[1], item[0]))
        if len(ranked) > 1 and ranked[0][1] == ranked[1][1]:
            raise AssertionError(
                f"{region}'s busiest month is a tie — the question has no single "
                "answer, and the fixture has to change, not the grader"
            )
        rows.append((region, ranked[0][0].isoformat()))
    return Answer.of(*rows)


def months_spanned(ledger: Ledger = PINNED) -> list[date]:
    return sorted({ordered_on.replace(day=1) for _, _, ordered_on, _ in ledger.orders})


def customers_ordering_every_month(ledger: Ledger = PINNED) -> Answer:
    """Customers with at least one order in **every** month the ledger spans.

    The question the pinned slate cannot ask. Every wrong answer to the four
    pinned questions is a **subset** of the right one, so their shape cannot say
    which mistake was made. A universal quantifier is different in kind: an arm
    that misses one of a customer's orders drops that customer, and one that
    misses a *month* adds customers who never qualified — so the answer moves in
    both directions, and its shape says which.
    """
    every = set(months_spanned(ledger))
    seen: dict[str, set[date]] = defaultdict(set)
    for _, customer, ordered_on, _ in ledger.orders:
        seen[customer].add(ordered_on.replace(day=1))
    return Answer.of(*[name for name, _ in ledger.customers if seen[name] == every])


def orders_per_customer(ledger: Ledger = PINNED) -> dict[str, int]:
    """How many orders each customer placed — so a question's subject is picked
    by its answer rather than by its index."""
    counts: dict[str, int] = {name: 0 for name, _ in ledger.customers}
    for _, customer, _, _ in ledger.orders:
        counts[customer] += 1
    return counts
