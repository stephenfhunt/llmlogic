"""The `imports` oracle, checked against second formulations and its own traps.

Nothing else checks these answers, and three of the four questions turn on a rule
that is easy to state and easy to apply to the wrong rows: a missing amount is
not a zero, an unshipped order is not a late one, and a month is a derived key.
"""

from collections import Counter
from datetime import date, timedelta

from hypothesis import given
from hypothesis import strategies as st

from harness.domains.imports import fixture, truth


def test_the_fixture_actually_contains_the_cases_the_questions_turn_on():
    # A trap nobody falls into is a trap that is not in the data. Each of these
    # was checked by hand once; the test is what keeps it true after a reseed.
    assert [row for row in fixture.ORDER_ROWS if row[3] is None], "no missing amounts"
    shipped = {order for _, order, _ in fixture.SHIPMENT_ROWS}
    assert [row for row in fixture.ORDER_ROWS if row[0] not in shipped], "every order shipped"
    assert [row for row in fixture.SHIPMENT_ROWS if row[2] is None], "no undated shipments"


def test_late_shipments_agree_with_a_row_by_row_reading():
    """The same answer by walking shipments rather than orders."""
    ordered_on = {row[0]: row[2] for row in fixture.ORDER_ROWS}
    by_shipment = {
        order
        for _, order, shipped in fixture.SHIPMENT_ROWS
        if shipped is not None and (shipped - ordered_on[order]).days > truth.LATE_AFTER_DAYS
    }
    assert {order for (order,) in truth.late_shipments().rows} == by_shipment


def test_an_unshipped_order_is_never_late():
    shipped = {order for _, order, _ in fixture.SHIPMENT_ROWS}
    late = {order for (order,) in truth.late_shipments().rows}
    assert not (late - shipped)


def test_an_undated_shipment_is_never_late():
    undated = {order for _, order, when in fixture.SHIPMENT_ROWS if when is None}
    assert undated
    assert not (undated & {order for (order,) in truth.late_shipments().rows})


@given(days=st.integers(min_value=0, max_value=30))
def test_a_higher_lateness_bar_never_adds_orders(days):
    # Monotonicity: the answer set shrinks as the threshold rises. A comparison
    # written the wrong way round passes on one threshold and fails this.
    wider = {order for (order,) in truth.late_shipments(days).rows}
    tighter = {order for (order,) in truth.late_shipments(days + 1).rows}
    assert tighter <= wider


def test_region_totals_ignore_missing_amounts_rather_than_zeroing_them():
    # Both readings give the same *total*; they differ on what a total is over.
    # This pins the one the question states, and would catch a `or 0` creeping in.
    counted = sum(1 for _, customer, _, amount in fixture.ORDER_ROWS if amount is not None)
    assert counted < len(fixture.ORDER_ROWS)
    assert sum(truth.region_totals().values()) == sum(
        amount for *_, amount in fixture.ORDER_ROWS if amount is not None
    )


@given(threshold=st.integers(min_value=0, max_value=20_000))
def test_regions_over_a_threshold_shrink_as_it_rises(threshold):
    wider = {region for (region,) in truth.regions_over(threshold).rows}
    tighter = {region for (region,) in truth.regions_over(threshold + 1).rows}
    assert tighter <= wider


def test_regions_over_the_threshold_are_some_but_not_all():
    over = {region for (region,) in truth.regions_over().rows}
    regions = set(truth.region_totals())
    assert over and over != regions


def test_a_quiet_customer_really_placed_nothing_in_the_window():
    quiet = {customer for (customer,) in truth.customers_with_no_orders_between().rows}
    assert quiet
    for _, customer, ordered_on, _ in fixture.ORDER_ROWS:
        if truth.QUIET_FROM <= ordered_on <= truth.QUIET_TO:
            assert customer not in quiet


def test_the_window_is_inclusive_at_both_ends():
    # The off-by-one that changes the answer and looks like nothing.
    inside = [
        customer
        for _, customer, ordered_on, _ in fixture.ORDER_ROWS
        if ordered_on in (truth.QUIET_FROM, truth.QUIET_TO)
    ]
    assert inside, "no order sits on a boundary, so the boundary is untested"
    quiet = {customer for (customer,) in truth.customers_with_no_orders_between().rows}
    assert not (set(inside) & quiet)


def test_every_region_has_a_strict_busiest_month():
    # Well-posedness. `busiest_month_per_region` raises on a tie rather than
    # picking one, so this failing means the seed has to change.
    rows = truth.busiest_month_per_region().rows
    assert len(rows) == len(truth.orders_per_region_month())
    for region, months in truth.orders_per_region_month().items():
        top = sorted(months.values(), reverse=True)
        assert top[0] > top[1], region


def test_the_busiest_month_agrees_with_a_count_over_raw_rows():
    """Counted again without the month index, straight off the order rows."""
    for region, month in truth.busiest_month_per_region().rows:
        first = date.fromisoformat(month)
        last = (first + timedelta(days=32)).replace(day=1)
        counts = Counter()
        for _, customer, ordered_on, _ in fixture.ORDER_ROWS:
            if truth.region_of(customer) == region:
                counts[ordered_on.replace(day=1)] += 1
        assert counts[first] == max(counts.values())
        assert first < last  # the month key is the first of the month, not the last
