"""Four questions over the three tables."""

from __future__ import annotations

from harness.domains.imports import fixture, truth
from harness.generate import Degenerate
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


def generated(seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh slate over a fresh ledger.

    The four pinned questions re-asked, plus a fifth the pinned slate cannot
    ask: *which customers ordered in every month the ledger spans*. A universal
    quantifier is different in kind from the four — an arm that misses one of a
    customer's orders drops that customer, and one that misses a month adds
    customers who never qualified, so the answer moves in both directions and
    its shape says which mistake was made.

    On the `at-scale` track only `late-shipments` is narrowed: unscoped over
    21,600 orders it answers with 10,795 order ids, which measures
    transcription. The other four group by region or by a capped handful of
    customers, so they stay writable at any size — which is the combination the
    track is about.
    """
    ledger = fixture.generate(seed, difficulty, track)
    fx = fixture.to_fixture(ledger)
    tag = f"{seed:x}"[-4:]
    missing = "Some orders have no amount recorded; those orders are not counted."
    common = {"domain": DOMAIN, "fixture": fx, "track": track, "difficulty": difficulty}
    return [
        Task(
            id=f"g{tag}-d{difficulty}-late-shipments",
            question=(
                f"Which orders shipped more than {ledger.late_after_days} days after "
                "they were ordered? `shipment.jsonl` links a shipment to its order. "
                "An order that never shipped, or whose shipment has no date, is not "
                "late."
                if track == "in-context"
                else (
                    f"Which of customer {ledger.rare_customer}'s orders shipped more "
                    f"than {ledger.late_after_days} days after they were ordered? "
                    "`shipment.jsonl` links a shipment to its order. An order that "
                    "never shipped, or whose shipment has no date, is not late."
                )
            ),
            truth=(
                truth.late_shipments(ledger=ledger)
                if track == "in-context"
                else truth.late_shipments_for(ledger.rare_customer, ledger)
            ),
            question_class="temporal",
            answer_shape=("order",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-regions-over-threshold",
            question=(
                f"Which regions have a total order amount above {ledger.region_threshold}? "
                "A customer's region is in `customer.csv`, and each order belongs to a "
                f"customer. {missing}"
            ),
            truth=truth.regions_over(ledger=ledger),
            question_class="aggregation",
            answer_shape=("region",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-customers-with-no-recent-orders",
            question=(
                "Which customers placed no order between "
                f"{ledger.quiet_from.isoformat()} and {ledger.quiet_to.isoformat()} "
                "inclusive? List every such customer, including any that never "
                "ordered at all."
            ),
            truth=truth.customers_with_no_orders_between(ledger=ledger),
            question_class="negation",
            answer_shape=("customer",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-busiest-month-per-region",
            question=(
                "For each region, which month did its customers place the most orders "
                "in? Count every order, whether or not it has an amount. Identify the "
                "month by its first day, as YYYY-MM-DD. Each region has a single "
                "busiest month."
            ),
            truth=truth.busiest_month_per_region(ledger),
            question_class="aggregation",
            answer_shape=("region", "month"),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-ordered-every-month",
            question=(
                "Consider every month in which any order was placed. Which customers "
                "placed at least one order in every one of those months? Count every "
                "order, whether or not it has an amount."
            ),
            truth=truth.customers_ordering_every_month(ledger),
            question_class="aggregation",
            answer_shape=("customer",),
            **common,
        ),
    ]


def check(task: Task) -> None:
    """This pack's own invariants, on top of the universal ones.

    Read off the **emitted** files, because that is what the subject sees, and
    because two of them carry the same rows in different formats: an oracle that
    agreed with the generator's tuples while `order.csv` said something else
    would be checking the wrong artefact.
    """
    rows = [
        line.split(",") for line in task.fixture.text("order.csv").splitlines()[1:] if line.strip()
    ]
    suffix = task.id.split("-", 2)[2] if task.id.startswith("g") else task.id

    if suffix == "busiest-month-per-region":
        # The pinned fixture bought a strict maximum with a seed search; the
        # generator repairs ties instead, and this is what says it worked. A tie
        # is invisible in the data and makes the question unanswerable as asked.
        seen: dict[str, str] = {}
        for region, month in task.truth.rows:
            if region in seen:
                raise Degenerate(
                    f"{task.id}: {region} has two busiest months — the question says one"
                )
            seen[region] = month
    if suffix == "late-shipments":
        shipped = {
            line.split('"order": "')[1].split('"')[0]
            for line in task.fixture.text("shipment.jsonl").splitlines()
            if line.strip()
        }
        if len(shipped) >= len(rows):
            raise Degenerate(
                f"{task.id}: every order shipped — *an order that never shipped is "
                "not late* is then a rule with nothing to apply it to"
            )
    if suffix == "customers-with-no-recent-orders":
        customers = {row[0] for row in rows}
        if {row[0] for row in task.truth.rows} & customers == customers:
            raise Degenerate(f"{task.id}: nobody ordered in the window at all")
