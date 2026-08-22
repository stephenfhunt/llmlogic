"""Three tables, generated from a fixed seed: customers, orders, shipments.

Sized to stay well inside the smaller strength's context — the prose arm has to
hold the rows in its head, and a fixture that defeats it on context alone would
measure the context window.
"""

from __future__ import annotations

import io
import json
import random
from datetime import date, timedelta

from harness.task import Fixture

#: Chosen by search, not by taste: the generator is run and the fixture kept only
#: if every region's busiest month is a *strict* maximum. A tie would make one
#: task unanswerable as asked, and the tie is invisible in the data.
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


CUSTOMER_ROWS, ORDER_ROWS, SHIPMENT_ROWS = _build()


def _order_csv() -> str:
    rows = "".join(
        f"{order_id},{customer},{ordered_on.isoformat()},{'' if amount is None else amount}\n"
        for order_id, customer, ordered_on, amount in ORDER_ROWS
    )
    return "id,customer,ordered_on,amount\n" + rows


def _order_parquet() -> bytes:
    """The same rows, in Parquet. Written with real column types, which is the
    only reason the copy is worth carrying: a date column arrives typed."""
    import pyarrow
    import pyarrow.parquet as pq

    table = pyarrow.table(
        {
            "id": pyarrow.array([row[0] for row in ORDER_ROWS], pyarrow.string()),
            "customer": pyarrow.array([row[1] for row in ORDER_ROWS], pyarrow.string()),
            "ordered_on": pyarrow.array([row[2] for row in ORDER_ROWS], pyarrow.date32()),
            "amount": pyarrow.array([row[3] for row in ORDER_ROWS], pyarrow.int64()),
        }
    )
    sink = io.BytesIO()
    pq.write_table(table, sink)
    return sink.getvalue()


def _shipment_jsonl() -> str:
    return "".join(
        json.dumps(
            {
                "id": shipment_id,
                "order": order_id,
                "shipped_on": None if shipped_on is None else shipped_on.isoformat(),
            }
        )
        + "\n"
        for shipment_id, order_id, shipped_on in SHIPMENT_ROWS
    )


def build() -> Fixture:
    customer_csv = "id,region\n" + "".join(f"{name},{region}\n" for name, region in CUSTOMER_ROWS)
    return Fixture(
        files={
            "customer.csv": customer_csv,
            "order.csv": _order_csv(),
            "order.parquet": _order_parquet(),
            "shipment.jsonl": _shipment_jsonl(),
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
