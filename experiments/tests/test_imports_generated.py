"""The generated ledgers: do the formats agree, and is a blank still a blank?

Hand-checking 28 tasks proved the oracle matched the *author's* reading, and it
could not prove there was only one (`decisions.md` 2026-08-24). Generation
multiplies that by the number of items, so the checks are mechanical here.

The second formulation is deliberately **the emitted files** — `order.csv` and
`shipment.jsonl`, parsed as text — rather than a second pass over the same
tuples. This pack ships one table in two formats on purpose, and the failure it
invites is that the copies drift; and a blank cell is the only way a subject ever
learns an amount is missing.
"""

from __future__ import annotations

import json
from datetime import date

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.domains.imports import fixture, truth
from harness.domains.imports.tasks import check, generated
from harness.generate import Degenerate, validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = sorted(fixture.DIFFICULTY)


def _orders_from_csv(fx) -> list[tuple[str, str, date, int | None]]:
    rows = []
    for line in fx.text("order.csv").splitlines()[1:]:
        if not line.strip():
            continue
        order_id, customer, ordered_on, amount = line.split(",")
        rows.append(
            (
                order_id,
                customer,
                date.fromisoformat(ordered_on),
                None if amount == "" else int(amount),
            )
        )
    return rows


def _shipments_from_jsonl(fx) -> dict[str, date | None]:
    shipped: dict[str, date | None] = {}
    for line in fx.text("shipment.jsonl").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        shipped.setdefault(
            row["order"],
            None if row["shipped_on"] is None else date.fromisoformat(row["shipped_on"]),
        )
    return shipped


class TestOracle:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_late_agrees_with_the_emitted_files(self, seed):
        """Date arithmetic across a join, recomputed from the text the workspace
        gets. An order with no shipment, or a shipment with no date, is not late
        — it is unknown, and the two are different answers."""
        ledger = fixture.generate(seed, 3)
        fx = fixture.to_fixture(ledger)
        shipped = _shipments_from_jsonl(fx)
        expected = {
            order_id
            for order_id, _, ordered_on, _ in _orders_from_csv(fx)
            if (on := shipped.get(order_id)) is not None
            and (on - ordered_on).days > ledger.late_after_days
        }
        assert {row[0] for row in truth.late_shipments(ledger=ledger).rows} == expected

    @pytest.mark.parametrize("seed", SEEDS)
    def test_region_totals_agree_with_the_emitted_csv(self, seed):
        """And a missing amount is skipped, not read as a zero — which for a
        *sum* is the same number, and for the count in `busiest-month` is not."""
        ledger = fixture.generate(seed, 3)
        fx = fixture.to_fixture(ledger)
        region_of = dict(ledger.customers)
        totals: dict[str, int] = {}
        for _, customer, _, amount in _orders_from_csv(fx):
            if amount is not None:
                totals[region_of[customer]] = totals.get(region_of[customer], 0) + amount
        assert truth.region_totals(ledger) == {
            region: totals.get(region, 0) for region in ledger.regions
        }

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_parquet_copy_carries_the_same_rows(self, seed):
        """The redundant copy exists so no cell is decided by which formats an
        arm can open. Two spellings of one table is also two chances to drift."""
        pyarrow = pytest.importorskip("pyarrow")
        import pyarrow.parquet as pq

        fx = fixture.to_fixture(fixture.generate(seed, 2))
        import io

        table = pq.read_table(io.BytesIO(fx.files["order.parquet"]))
        from_parquet = [
            (row["id"], row["customer"], row["ordered_on"], row["amount"])
            for row in table.to_pylist()
        ]
        assert from_parquet == _orders_from_csv(fx)
        assert pyarrow.types.is_date32(table.schema.field("ordered_on").type)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_every_region_has_a_strict_busiest_month(self, seed):
        """The pinned fixture bought this with a seed search. `_break_ties`
        replaces the search with a repair, and the oracle still raises on a tie
        — which is what makes it a repair rather than a hope."""
        for difficulty in DIFFICULTIES:
            ledger = fixture.generate(seed, difficulty)
            rows = truth.busiest_month_per_region(ledger).rows
            assert len({region for region, _ in rows}) == len(rows)
            assert len(rows) == len(ledger.regions)


class TestGeneratedItems:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_generated_item_is_worth_asking(self, seed, difficulty):
        rejected = 0
        for task in generated(seed, difficulty):
            try:
                validate(task)
                check(task)
            except Degenerate:
                rejected += 1
        assert rejected == 0

    @pytest.mark.parametrize("seed", SEEDS[:6])
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_the_gaps_the_questions_turn_on_are_actually_there(self, seed, difficulty):
        """Every question that sums says what happens to a missing amount, and
        every question about lateness says what happens to an order that never
        shipped. A ledger with no gaps makes both of those sentences free."""
        ledger = fixture.generate(seed, difficulty)
        assert any(amount is None for _, _, _, amount in ledger.orders)
        assert any(on is None for _, _, on in ledger.shipments)
        assert len({order for _, order, _ in ledger.shipments}) < len(ledger.orders)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_ordering_every_month_is_not_everybody(self, seed):
        """Ordinary customers have a season. Without one, twenty-five orders
        over five months covered them all and the universal quantifier asked
        nothing — 864 of 864 customers at `at-scale`."""
        for track, difficulty in (("in-context", 3), ("at-scale", 1)):
            ledger = fixture.generate(seed, difficulty, track)
            answered = truth.customers_ordering_every_month(ledger).rows
            assert 0 < len(answered) < len(ledger.customers) / 2

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_difficulty_widens_the_gaps_and_the_grouping(self, seed):
        """The knobs this pack's difficulty actually turns: more absent values,
        and more months for the busiest-month question to separate."""
        shallow = fixture.generate(seed, 1)
        deep = fixture.generate(seed, 5)
        blanks = lambda ledger: sum(1 for _, _, _, amount in ledger.orders if amount is None)  # noqa: E731
        assert blanks(deep) > blanks(shallow)
        assert len(truth.months_spanned(deep)) > len(truth.months_spanned(shallow))

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_ledger(self, seed):
        assert fixture.generate(seed, 3) == fixture.generate(seed, 3)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        one = {name for name, _ in fixture.generate(seed, 3).customers}
        other = {name for name, _ in fixture.generate(seed + 1, 3).customers}
        assert not (one & other)

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_an_in_context_fixture_stays_inside_the_budget(self, difficulty):
        task = generated(20260825, difficulty)[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET
        assert task.track == "in-context"

    def test_an_at_scale_fixture_actually_exceeds_it(self):
        task = generated(20260825, 1, "at-scale")[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 > FIXTURE_TOKEN_BUDGET
        assert task.track == "at-scale"

    def test_at_scale_answers_stay_small(self):
        """Only `late-shipments` is narrowed, because only it needed to be:
        unscoped over 21,600 orders it answered with 10,795 order ids."""
        for task in generated(20260825, 1, "at-scale"):
            assert len(task.truth.rows) <= 20, f"{task.id} answers {len(task.truth.rows)} rows"

    def test_at_scale_refuses_a_difficulty_it_cannot_carry(self):
        with pytest.raises(ValueError, match="at-scale takes difficulty"):
            fixture.generate(20260825, 5, "at-scale")

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)

    def test_the_generated_slate_asks_something_the_pinned_one_cannot(self):
        ids = {task.id for task in generated(20260825, 3)}
        assert any(task_id.endswith("ordered-every-month") for task_id in ids)


class TestCheck:
    def test_a_region_with_two_busiest_months_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        task = next(t for t in generated(20260825, 3) if t.id.endswith("busiest-month-per-region"))
        region = sorted(task.truth.rows)[0][0]
        doubled = Answer.of(*task.truth.rows, (region, "2026-12-01"))
        with pytest.raises(Degenerate, match="two busiest months"):
            check(replace(task, truth=doubled))
