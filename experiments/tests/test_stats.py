"""The statistics, against values worked by hand.

A statistics module that is subtly wrong is worse than none: it puts a decimal
point on a mistake. Every test here pins a number computed independently of the
implementation — from the closed form, from a published Wilson interval, or by
enumerating the whole sample space.
"""

from __future__ import annotations

import math

import pytest

from harness.stats import (
    Interval,
    binomial_two_sided,
    bootstrap_delta,
    f1,
    mcnemar,
    required_items,
    wilson,
)


class TestWilson:
    def test_a_perfect_score_does_not_claim_certainty(self):
        """The reason it is Wilson. The normal approximation gives [100%, 100%]
        from eight observations, which is how a negative control's 8/8 came to
        read as a fact rather than as eight coin flips."""
        interval = wilson(8, 8)
        assert interval.high == pytest.approx(1.0, abs=1e-3)
        assert interval.low == pytest.approx(0.676, abs=0.002)

    def test_a_half_of_a_hundred(self):
        interval = wilson(50, 100)
        assert interval.low == pytest.approx(0.4038, abs=0.001)
        assert interval.high == pytest.approx(0.5962, abs=0.001)

    def test_the_grid_s_own_rate(self):
        """41/48 — the number the first grid reported for both arms, and the
        interval it never printed."""
        interval = wilson(41, 48)
        assert interval.low == pytest.approx(0.7283, abs=0.001)
        assert interval.high == pytest.approx(0.927, abs=0.002)

    def test_it_stays_inside_the_unit_interval(self):
        for successes, trials in [(0, 1), (0, 5), (3, 3), (1, 2), (0, 100), (100, 100)]:
            interval = wilson(successes, trials)
            assert 0.0 <= interval.low <= interval.high <= 1.0

    def test_no_trials_is_no_information(self):
        assert wilson(0, 0) == Interval(0.0, 1.0)

    def test_more_trials_narrow_it(self):
        wide = wilson(5, 10)
        narrow = wilson(50, 100)
        assert (narrow.high - narrow.low) < (wide.high - wide.low)


class TestBinomial:
    def test_no_trials_is_certain(self):
        assert binomial_two_sided(0, 0) == 1.0

    def test_five_of_five_by_hand(self):
        """Only k=0 and k=5 are as unlikely as the observation: 2/32."""
        assert binomial_two_sided(0, 5) == pytest.approx(0.0625)
        assert binomial_two_sided(5, 5) == pytest.approx(0.0625)

    def test_one_of_ten_by_hand(self):
        """k in {0, 1, 9, 10} = (1 + 10 + 10 + 1)/1024."""
        assert binomial_two_sided(1, 10) == pytest.approx(22 / 1024)

    def test_the_middle_is_never_evidence(self):
        assert binomial_two_sided(5, 10) == pytest.approx(1.0)

    def test_the_equal_tail_is_not_dropped_to_rounding(self):
        """The symmetric outcome is *mathematically* as likely as the observed
        one and can compute a hair larger. Without the tolerance the far tail
        vanishes and every p-value comes out half its true size."""
        for k in range(0, 21):
            assert binomial_two_sided(k, 20) == pytest.approx(binomial_two_sided(20 - k, 20))


class TestMcNemar:
    def test_a_ceiling_is_visible_in_the_cells(self):
        """Both arms right on everything: no discordant pairs, no evidence, and
        `both` says why. This is the shape of the 2026-08-24 slate."""
        arm = {f"t{i}": True for i in range(20)}
        result = mcnemar(arm, dict(arm))
        assert (result.both, result.b, result.c, result.neither) == (20, 0, 0, 0)
        assert result.discordant == 0
        assert result.p_value == 1.0
        assert result.delta == 0.0

    def test_only_discordant_pairs_carry_the_test(self):
        first = {"a": True, "b": True, "c": True, "d": False}
        second = {"a": True, "b": False, "c": False, "d": False}
        result = mcnemar(first, second)
        assert (result.both, result.b, result.c, result.neither) == (1, 2, 0, 1)
        assert result.p_value == pytest.approx(binomial_two_sided(2, 2))
        assert result.delta == pytest.approx(0.5)

    def test_delta_is_the_difference_in_rates(self):
        first = {"a": True, "b": True, "c": False}
        second = {"a": True, "b": False, "c": False}
        result = mcnemar(first, second)
        assert result.delta == pytest.approx(2 / 3 - 1 / 3)

    def test_an_unpaired_cell_is_dropped_not_counted_wrong(self):
        """A crashed cell must not read as evidence against its arm."""
        first = {"a": True, "b": True}
        second = {"a": False}
        result = mcnemar(first, second)
        assert result.n == 1
        assert (result.b, result.c) == (1, 0)

    def test_swapping_the_arms_swaps_b_and_c(self):
        first = {"a": True, "b": False, "c": True}
        second = {"a": False, "b": True, "c": True}
        forward = mcnemar(first, second)
        backward = mcnemar(second, first)
        assert (forward.b, forward.c) == (backward.c, backward.b)
        assert forward.p_value == pytest.approx(backward.p_value)
        assert forward.delta == pytest.approx(-backward.delta)


class TestBootstrap:
    def test_it_is_seeded(self):
        first = {f"t{i}": i % 3 != 0 for i in range(30)}
        second = {f"t{i}": i % 4 != 0 for i in range(30)}
        assert bootstrap_delta(first, second, resamples=500) == bootstrap_delta(
            first, second, resamples=500
        )

    def test_it_brackets_the_observed_delta(self):
        first = {f"t{i}": i % 3 != 0 for i in range(30)}
        second = {f"t{i}": i % 4 != 0 for i in range(30)}
        observed = mcnemar(first, second).delta
        interval = bootstrap_delta(first, second, resamples=2000)
        assert interval.low <= observed <= interval.high

    def test_two_identical_arms_have_a_zero_width_interval(self):
        arm = {f"t{i}": i % 2 == 0 for i in range(20)}
        interval = bootstrap_delta(arm, dict(arm), resamples=200)
        assert interval.low == 0.0 and interval.high == 0.0

    def test_nothing_shared_is_no_interval(self):
        assert bootstrap_delta({"a": True}, {"b": True}) == Interval(0.0, 0.0)


class TestF1:
    def test_an_exact_answer_scores_one(self):
        assert f1(missing=0, extra=0, truth_size=10) == 1.0

    def test_one_row_of_forty_dropped_is_not_the_same_as_nothing(self):
        """The distinction binary grading throws away, and the reason partial
        credit exists: both of these are `WRONG`."""
        near = f1(missing=1, extra=0, truth_size=40)
        nothing = f1(missing=40, extra=0, truth_size=40)
        assert near > 0.97
        assert nothing == 0.0

    def test_by_hand(self):
        assert f1(missing=1, extra=0, truth_size=10) == pytest.approx(2 * 0.9 / 1.9)
        assert f1(missing=0, extra=10, truth_size=10) == pytest.approx(2 / 3)

    def test_the_empty_answer_is_a_real_answer(self):
        """Negation over a closed set is a whole question class whose truth is
        empty. Getting it right must score 1.0, not 0.0."""
        assert f1(missing=0, extra=0, truth_size=0) == 1.0
        assert f1(missing=0, extra=3, truth_size=0) == 0.0

    def test_it_stays_in_range(self):
        for missing in range(0, 6):
            for extra in range(0, 6):
                assert 0.0 <= f1(missing, extra, 5) <= 1.0


class TestPower:
    def test_a_bigger_effect_needs_fewer_items(self):
        assert required_items(0.85, 0.20) < required_items(0.85, 0.10)

    def test_more_power_needs_more_items(self):
        assert required_items(0.85, 0.15, power=0.90) > required_items(0.85, 0.15, power=0.80)

    def test_the_first_grid_was_underpowered_for_a_ten_point_effect(self):
        """The finding this function exists to have surfaced in advance: 48 paired
        items cannot detect ten points on an 85% baseline."""
        assert required_items(0.85, 0.10) > 48

    def test_a_zero_effect_is_not_a_question(self):
        with pytest.raises(ValueError):
            required_items(0.85, 0.0)

    def test_it_returns_a_whole_number_of_items(self):
        n = required_items(0.5, 0.1)
        assert isinstance(n, int) and n == math.ceil(n)
