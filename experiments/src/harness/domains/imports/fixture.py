"""Three tables: customers, orders, shipments.

Two entry points, answering different questions.

``build()`` returns the **pinned** ledger, sized to stay well inside the smaller
strength's context — the prose arm has to hold the rows in its head, and a
fixture that defeats it on context alone would measure the context window.

``generate(seed, difficulty, track)`` returns a fresh one. Difficulty is **how
much of the data is absent or unresolved** and how finely it has to be grouped,
not how many rows there are: more orders with no amount, more orders that never
shipped, more shipments with no date, and more months for the busiest-month
question to separate. Every one of those is a place where reading a blank as a
zero, or an unshipped order as on-time, gets a plausible wrong answer.
"""

from __future__ import annotations

import io
import json
import random
from collections import Counter
from dataclasses import dataclass
from datetime import date, timedelta

from harness.task import Fixture

#: Chosen by search, not by taste: the pinned fixture is kept only if every
#: region's busiest month is a *strict* maximum. A tie would make one task
#: unanswerable as asked, and the tie is invisible in the data. `generate`
#: replaces the search with a repair — see `_break_ties`.
SEED = 20260835

REGIONS = ("north", "south", "east", "west")
CUSTOMERS = tuple(f"c{i:02d}" for i in range(1, 13))

WINDOW_START = date(2026, 1, 1)
WINDOW_END = date(2026, 4, 30)

#: Orders with no amount at all — a blank CSV cell, a JSON `null`. Not noise:
#: every question that sums has to say what happens to them, and an arm that
#: reads the blank as a zero answers a different question silently.
MISSING_AMOUNTS = 5
#: Orders that were never shipped, so the join has rows on one side only.
UNSHIPPED = 8
#: Shipments whose date is missing — dispatched, not yet recorded.
UNDATED_SHIPMENTS = 3

ORDER_COUNT = 54


#: ``(customers, regions, orders, months, missing amounts, unshipped, undated)``.
#:
#: The knobs that matter are the last three and the months. A ledger where every
#: order has an amount and every shipment a date is a join and a sum; the gaps
#: are what make *"an order that never shipped is not late"* a rule an arm has to
#: apply rather than a sentence it can skip. More months means the busiest-month
#: question separates a finer grouping, where a single order decides it.
DIFFICULTY: dict[int, tuple[int, int, int, int, int, int, int]] = {
    1: (12, 4, 54, 4, 5, 8, 3),  # the pinned shape's neighbourhood
    2: (18, 4, 110, 5, 14, 18, 7),
    3: (28, 5, 190, 6, 30, 34, 14),
    4: (40, 6, 300, 8, 54, 60, 26),
    5: (56, 6, 460, 10, 92, 100, 44),
}

#: The ``at-scale`` multiplier on the **orders**, not the customers. Every
#: question is about orders grouped by something, so multiplying the customers
#: multiplies the group count and the answers with it; multiplying the orders
#: multiplies the fact base and leaves the answers the size of the region list.
#: Sized so the fixture exceeds `cell.FIXTURE_TOKEN_BUDGET`; a test pins it.
AT_SCALE = 400

#: The highest difficulty the ``at-scale`` track accepts.
AT_SCALE_MAX_DIFFICULTY = 2

#: Orders per customer at `at-scale`. Customers grow with the ledger so a
#: customer stays a readable slice, and so *"ordered in every month"* is still a
#: question about a handful of people rather than about thousands.
AT_SCALE_ORDERS_PER_CUSTOMER = 25

#: Fixed handfuls, not shares: what the `at-scale` questions are scoped to. A
#: share of 21,600 orders is an answer nobody can transcribe — the lesson
#: `access_control` records after a proportional isolated set produced 1,920
#: rows.
RARE_CUSTOMER_ORDERS = 11
MAX_QUIET_CUSTOMERS = 10
MAX_EVERY_MONTH_CUSTOMERS = 6


def _build():
    rng = random.Random(SEED)
    customers = tuple((name, rng.choice(REGIONS)) for name in CUSTOMERS)

    span = (WINDOW_END - WINDOW_START).days
    orders = []
    for index in range(1, ORDER_COUNT + 1):
        orders.append(
            (
                f"o{index:03d}",
                rng.choice(CUSTOMERS),
                WINDOW_START + timedelta(days=rng.randint(0, span)),
                rng.randint(20, 900),
            )
        )
    for index in rng.sample(range(len(orders)), MISSING_AMOUNTS):
        order_id, customer, ordered_on, _ = orders[index]
        orders[index] = (order_id, customer, ordered_on, None)

    shipped = sorted(rng.sample(range(len(orders)), len(orders) - UNSHIPPED))
    shipments = []
    for count, index in enumerate(shipped, start=1):
        _, _, ordered_on, _ = orders[index]
        shipments.append(
            (
                f"s{count:03d}",
                orders[index][0],
                ordered_on + timedelta(days=rng.randint(0, 14)),
            )
        )
    for index in rng.sample(range(len(shipments)), UNDATED_SHIPMENTS):
        shipment_id, order_id, _ = shipments[index]
        shipments[index] = (shipment_id, order_id, None)

    return customers, tuple(orders), tuple(shipments)


@dataclass(frozen=True)
class Ledger:
    """One ledger, and the thresholds its questions are asked against.

    A value rather than module globals, so the oracle can be handed a
    *generated* ledger and checked against a second formulation on it. With
    globals the oracle can only ever be checked against the one ledger it was
    written for, which is how a fixture comes to be tuned to its truth.

    An order's ``amount`` and a shipment's ``shipped_on`` may be ``None`` —
    absent, which is not the same as zero and not the same as on time.
    """

    customers: tuple[tuple[str, str], ...]
    orders: tuple[tuple[str, str, date, int | None], ...]
    shipments: tuple[tuple[str, str, date | None], ...]
    regions: tuple[str, ...]
    late_after_days: int = 10
    region_threshold: int = 5_000
    quiet_from: date = date(2026, 3, 1)
    quiet_to: date = date(2026, 4, 30)
    #: A customer with a fixed handful of orders, so an `at-scale` question can
    #: be scoped to one. Empty on the pinned ledger.
    rare_customer: str = ""

    def __hash__(self) -> int:
        """The generated hash, computed once — see `scheduling.fixture.Roster`,
        where an `at-scale` value rehashed its rows on every cached lookup."""
        cached = self.__dict__.get("_hash")
        if cached is None:
            cached = hash((self.customers, self.orders, self.shipments, self.regions))
            object.__setattr__(self, "_hash", cached)
        return cached


CUSTOMER_ROWS, ORDER_ROWS, SHIPMENT_ROWS = _build()

PINNED = Ledger(
    customers=CUSTOMER_ROWS,
    orders=ORDER_ROWS,
    shipments=SHIPMENT_ROWS,
    regions=REGIONS,
)


def _months(start: date, count: int) -> list[date]:
    """The first day of each of ``count`` consecutive months from ``start``."""
    months = []
    year, month = start.year, start.month
    for _ in range(count):
        months.append(date(year, month, 1))
        year, month = (year + 1, 1) if month == 12 else (year, month + 1)
    return months


def _in_month(month: date, rng: random.Random) -> date:
    """A day inside ``month``, never the 29th–31st, so no month is short of
    candidate days and nothing depends on February."""
    return month.replace(day=rng.randint(1, 28))


def _break_ties(orders: list, region_of: dict[str, str], rng: random.Random) -> list:
    """Move an order until every region's busiest month is a *strict* maximum.

    The pinned fixture bought this with a seed search — run the generator, keep
    the draw only if no region tied — which does not survive being asked for a
    thousand fixtures. The repair is deterministic and local: one order from the
    runner-up month moves into the leader's, which changes the count that was
    tied and nothing else about the shape.

    `truth.busiest_month_per_region` still raises on a tie. That guard is what
    makes this a repair rather than a hope.
    """
    for _ in range(len(orders)):
        counts: dict[str, Counter] = {}
        for _, customer, ordered_on, _ in orders:
            counts.setdefault(region_of[customer], Counter())[ordered_on.replace(day=1)] += 1
        tied = None
        for region, months in counts.items():
            ranked = months.most_common()
            if len(ranked) > 1 and ranked[0][1] == ranked[1][1]:
                tied = (region, ranked[0][0], ranked[1][0])
                break
        if tied is None:
            return orders
        region, leader, runner_up = tied
        for index, (order_id, customer, ordered_on, amount) in enumerate(orders):
            if region_of[customer] == region and ordered_on.replace(day=1) == runner_up:
                orders[index] = (order_id, customer, _in_month(leader, rng), amount)
                break
    return orders


def generate(seed: int, difficulty: int = 3, track: str = "in-context") -> Ledger:
    """A fresh ledger.

    Identifiers are regenerated per seed, so nothing here can be answered from
    memory and two runs at the same difficulty are two samples rather than the
    same items twice.

    Three groups are engineered rather than drawn, because a draw supplies them
    only by luck: the **quiet** customers who place nothing in the window the
    third question asks about, the customers who order in **every** month, and
    the **rare** customer an `at-scale` question is scoped to.
    """
    if difficulty not in DIFFICULTY:
        raise ValueError(f"difficulty must be one of {sorted(DIFFICULTY)}")
    if track == "at-scale" and difficulty > AT_SCALE_MAX_DIFFICULTY:
        raise ValueError(
            f"at-scale takes difficulty 1-{AT_SCALE_MAX_DIFFICULTY}: scale is the "
            "variable on that track, and crossing it with structure produces an "
            "item that is about neither"
        )
    n_customers, n_regions, n_orders, n_months, missing, unshipped, undated = DIFFICULTY[difficulty]
    if track == "at-scale":
        n_orders *= AT_SCALE
        n_customers = max(n_customers, n_orders // AT_SCALE_ORDERS_PER_CUSTOMER)

    rng = random.Random(seed)
    tag = f"{seed:x}"[-4:]
    regions = tuple(f"{tag}-region{index}" for index in range(n_regions))
    names = tuple(f"c{tag}{index:05d}" for index in range(n_customers))
    customers = tuple((name, regions[index % n_regions]) for index, name in enumerate(names))
    region_of = dict(customers)
    months = _months(WINDOW_START, n_months)

    # The quiet customers order only in the first half of the window; the window
    # the question asks about is the second half. A *fixed handful*, so the
    # answer stays writable at any scale.
    # Capped above *and* scaled below: the ceiling keeps an `at-scale` answer
    # writable, and the floor keeps the smallest ledger from spending its whole
    # customer list on the engineered groups.
    n_quiet = max(2, min(MAX_QUIET_CUSTOMERS, n_customers // 4))
    n_every = max(2, min(MAX_EVERY_MONTH_CUSTOMERS, n_customers // 6))
    quiet = list(names[:n_quiet])
    every_month = list(names[n_quiet : n_quiet + n_every])
    rare = names[n_quiet + n_every]
    quiet_from = months[len(months) // 2]
    quiet_to = months[-1].replace(day=28)

    orders: list[tuple[str, str, date, int | None]] = []

    def add(customer: str, month: date) -> None:
        orders.append(
            (
                f"o{tag}{len(orders):06d}",
                customer,
                _in_month(month, rng),
                rng.randint(20, 900),
            )
        )

    for customer in quiet:
        for month in months[: len(months) // 2]:
            add(customer, month)
    for customer in every_month:
        for month in months:
            add(customer, month)
    for _ in range(RARE_CUSTOMER_ORDERS):
        add(rare, rng.choice(months))
    # Everyone else. Every non-quiet customer gets at least one order inside the
    # asked-about window, so `customers-with-no-recent-orders` is the quiet
    # handful and not "whoever the draw happened to miss".
    ordinary = [name for name in names if name not in set(quiet) | set(every_month) | {rare}]
    # Each ordinary customer has a **season** — a proper subset of the months —
    # and orders only inside it. Without one, `customers-ordering-every-month`
    # answered with 864 of 864 customers at `at-scale`: twenty-five orders over
    # five months covers them all, and a universal quantifier that everything
    # satisfies asks nothing.
    season = {
        customer: rng.sample(months[len(months) // 2 :] + months, max(2, len(months) - 2))
        for customer in ordinary
    }
    for customer in ordinary:
        add(customer, rng.choice(months[len(months) // 2 :]))
    while len(orders) < n_orders:
        customer = rng.choice(ordinary or list(names))
        add(customer, rng.choice(season.get(customer, months)))

    orders = _break_ties(orders, region_of, rng)

    # A missing amount is not a zero, and it is drawn *after* the dates are
    # settled so it cannot move a month count.
    for index in rng.sample(range(len(orders)), min(missing, len(orders))):
        order_id, customer, ordered_on, _ = orders[index]
        orders[index] = (order_id, customer, ordered_on, None)

    # Shipments. Half the shipped orders are deliberately late, so the threshold
    # decides something at every difficulty rather than only when the draw is
    # kind.
    shipped = sorted(rng.sample(range(len(orders)), max(0, len(orders) - unshipped)))
    shipments = []
    for count, index in enumerate(shipped):
        _, _, ordered_on, _ = orders[index]
        delay = rng.randint(0, 9) if count % 2 else rng.randint(11, 30)
        shipments.append(
            (f"s{tag}{count:06d}", orders[index][0], ordered_on + timedelta(days=delay))
        )
    for index in rng.sample(range(len(shipments)), min(undated, len(shipments))):
        shipment_id, order_id, _ = shipments[index]
        shipments[index] = (shipment_id, order_id, None)

    # The region threshold is calibrated to what was drawn: fixed, it names every
    # region or none, and a criterion nobody is near decides nothing.
    totals: dict[str, int] = dict.fromkeys(regions, 0)
    for _, customer, _, amount in orders:
        if amount is not None:
            totals[region_of[customer]] += amount
    ranked = sorted(totals.values())
    threshold = ranked[max(0, len(ranked) // 2 - 1)]

    return Ledger(
        customers=customers,
        orders=tuple(orders),
        shipments=tuple(shipments),
        regions=regions,
        late_after_days=10,
        region_threshold=threshold,
        quiet_from=quiet_from,
        quiet_to=quiet_to,
        rare_customer=rare,
    )


def _order_csv(ledger: Ledger) -> str:
    rows = "".join(
        f"{order_id},{customer},{ordered_on.isoformat()},{'' if amount is None else amount}\n"
        for order_id, customer, ordered_on, amount in ledger.orders
    )
    return "id,customer,ordered_on,amount\n" + rows


def _order_parquet(ledger: Ledger) -> bytes:
    """The same rows, in Parquet. Written with real column types, which is the
    only reason the copy is worth carrying: a date column arrives typed."""
    import pyarrow
    import pyarrow.parquet as pq

    table = pyarrow.table(
        {
            "id": pyarrow.array([row[0] for row in ledger.orders], pyarrow.string()),
            "customer": pyarrow.array([row[1] for row in ledger.orders], pyarrow.string()),
            "ordered_on": pyarrow.array([row[2] for row in ledger.orders], pyarrow.date32()),
            "amount": pyarrow.array([row[3] for row in ledger.orders], pyarrow.int64()),
        }
    )
    sink = io.BytesIO()
    pq.write_table(table, sink)
    return sink.getvalue()


def _shipment_jsonl(ledger: Ledger) -> str:
    return "".join(
        json.dumps(
            {
                "id": shipment_id,
                "order": order_id,
                "shipped_on": None if shipped_on is None else shipped_on.isoformat(),
            }
        )
        + "\n"
        for shipment_id, order_id, shipped_on in ledger.shipments
    )


def to_fixture(ledger: Ledger) -> Fixture:
    """The four files a workspace gets. One place, so a generated ledger and the
    pinned one are laid out identically and no cell is decided by layout.

    `order` ships as CSV **and** as Parquet, the same rows in each. The copy is
    redundant on purpose: a table only one arm can open would decide cells on
    file format rather than on reasoning (`decisions.md` 2026-08-22).
    """
    customer_csv = "id,region\n" + "".join(
        f"{name},{region}\n" for name, region in ledger.customers
    )
    return Fixture(
        files={
            "customer.csv": customer_csv,
            "order.csv": _order_csv(ledger),
            "order.parquet": _order_parquet(ledger),
            "shipment.jsonl": _shipment_jsonl(ledger),
        },
        schemas={
            "customer": ("id", "region"),
            "order": ("id", "customer", "ordered_on", "amount"),
            "shipment": ("id", "order", "shipped_on"),
        },
        sources={
            "order": ("order.csv", "order.parquet"),
            "shipment": ("shipment.jsonl",),
        },
    )


def build() -> Fixture:
    return to_fixture(PINNED)
