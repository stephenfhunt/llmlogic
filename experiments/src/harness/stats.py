"""Intervals and paired tests, so a delta is a measurement rather than a number.

The first grid reported *engine 41/48, prose 41/48, delta +0* and stopped there.
A delta with no interval is not evidence of no effect — with n=48 per arm, the
95% interval on that difference spans most of the plausible range, and nothing in
the report said so.

Two things are needed, and they are different:

- an **interval on a rate**, which says how precisely one arm was measured;
- a **paired test on the difference**, which uses the fact that both arms answered
  the *same* tasks. Comparing two independent proportions throws that pairing away
  and is markedly less powerful — most of the variance between arms is variance
  between items, and the pairing cancels it.

Pure stdlib on purpose: Wilson and an exact binomial are a few lines each, and a
dependency is a decision (``AGENTS.md``).
"""

from __future__ import annotations

import math
import random
from dataclasses import dataclass

#: Two-sided 95%. One number, one place — every interval this module prints uses
#: it, so a report never mixes coverage levels without saying so.
Z95 = 1.959963984540054


@dataclass(frozen=True)
class Interval:
    low: float
    high: float

    def __str__(self) -> str:
        return f"[{self.low:.0%}, {self.high:.0%}]"


def wilson(successes: int, trials: int, z: float = Z95) -> Interval:
    """A confidence interval for a rate.

    Wilson rather than the textbook normal approximation, which is wrong in
    exactly the region this harness lives in: at 8/8 the normal interval is
    [100%, 100%], claiming certainty from eight observations. Wilson gives a
    finite interval at the boundaries and stays inside [0, 1] everywhere.
    """
    if trials <= 0:
        return Interval(0.0, 1.0)
    p = successes / trials
    z2 = z * z
    denom = 1 + z2 / trials
    centre = (p + z2 / (2 * trials)) / denom
    half = z * math.sqrt(p * (1 - p) / trials + z2 / (4 * trials * trials)) / denom
    return Interval(max(0.0, centre - half), min(1.0, centre + half))


@dataclass(frozen=True)
class Paired:
    """One arm against another over the tasks both answered.

    ``b`` and ``c`` are the discordant pairs — the only ones that carry
    information about a difference. ``both`` and ``neither`` are reported because
    they are what a ceiling looks like: a slate where ``both`` is nearly
    everything cannot show a delta whatever the arms do.
    """

    both: int
    b: int  # first arm correct, second wrong
    c: int  # first arm wrong, second correct
    neither: int
    p_value: float

    @property
    def n(self) -> int:
        return self.both + self.b + self.c + self.neither

    @property
    def discordant(self) -> int:
        return self.b + self.c

    @property
    def delta(self) -> float:
        """First arm's rate minus the second's. Equals ``(b - c) / n``."""
        return (self.b - self.c) / self.n if self.n else 0.0


def mcnemar(first: dict[str, bool], second: dict[str, bool]) -> Paired:
    """McNemar's **exact** test over the keys the two arms share.

    Exact rather than the chi-square approximation: the discordant count is what
    the test runs on, and on a slate this size it is routinely under 10, where the
    approximation is not trustworthy. Under the null the b discordant pairs are a
    fair coin, so the p-value is a two-sided binomial tail.

    Keys present in only one mapping are dropped — an unpaired observation cannot
    contribute to a paired test, and silently treating a missing cell as wrong
    would let a crashed cell read as evidence.
    """
    shared = sorted(set(first) & set(second))
    both = b = c = neither = 0
    for key in shared:
        x, y = first[key], second[key]
        if x and y:
            both += 1
        elif x and not y:
            b += 1
        elif y and not x:
            c += 1
        else:
            neither += 1
    return Paired(both, b, c, neither, binomial_two_sided(b, b + c))


def binomial_two_sided(successes: int, trials: int, p: float = 0.5) -> float:
    """Two-sided exact binomial p-value.

    The two-sided value is the total probability of every outcome no more likely
    than the observed one — the standard definition, and the one that stays
    correct when the distribution is asymmetric. At ``p=0.5`` it reduces to
    doubling the smaller tail, but the general form is written so a future
    non-symmetric use does not silently get the wrong answer.
    """
    if trials <= 0:
        return 1.0
    observed = _pmf(successes, trials, p)
    # Floating point: an outcome that is mathematically equal to the observed one
    # can compute a hair larger and be excluded, which drops a whole term.
    tolerance = observed * (1 + 1e-9)
    total = sum(pmf for k in range(trials + 1) if (pmf := _pmf(k, trials, p)) <= tolerance)
    return min(1.0, total)


def _pmf(k: int, n: int, p: float) -> float:
    return math.comb(n, k) * (p**k) * ((1 - p) ** (n - k))


def bootstrap_delta(
    first: dict[str, bool],
    second: dict[str, bool],
    *,
    resamples: int = 10_000,
    seed: int = 20260825,
    z: float = Z95,  # noqa: ARG001 — percentile method; kept for signature symmetry
) -> Interval:
    """A percentile interval for the paired delta, resampling **tasks**.

    Resampling tasks rather than cells is the whole point: the pairing is the
    design, so a resample takes both arms' results for a task together or not at
    all. Seeded, because a report that changes between two renderings of the same
    run is a report nobody can cite.
    """
    shared = sorted(set(first) & set(second))
    if not shared:
        return Interval(0.0, 0.0)
    rng = random.Random(seed)
    n = len(shared)
    deltas = []
    for _ in range(resamples):
        picked = [shared[rng.randrange(n)] for _ in range(n)]
        deltas.append(sum(first[k] - second[k] for k in picked) / n)
    deltas.sort()
    lo = deltas[int(0.025 * resamples)]
    hi = deltas[min(resamples - 1, int(0.975 * resamples))]
    return Interval(lo, hi)


def f1(missing: int, extra: int, truth_size: int) -> float:
    """Per-item F1 over the answer set, from what a ``Grade`` already records.

    Binary set-equality throws away the difference between an answer that dropped
    one row of forty and one that returned nothing. Both are ``WRONG``; they are
    not the same failure, and on a slate with large answer sets the distinction is
    most of the signal.

    Derived rather than stored, so it is recoverable from every run already in
    ``results/`` without reopening an append-only record.
    """
    if truth_size == 0:
        # The empty answer is a real answer for a whole question class. Getting it
        # exactly right is 1.0; adding anything to it is 0.0.
        return 1.0 if extra == 0 else 0.0
    true_positives = truth_size - missing
    if true_positives <= 0:
        return 0.0
    precision = true_positives / (true_positives + extra)
    recall = true_positives / truth_size
    return 2 * precision * recall / (precision + recall)


def required_items(baseline: float, effect: float, *, power: float = 0.80, z: float = Z95) -> int:
    """Paired items needed to detect ``effect`` at 95% / ``power``.

    Approximate by construction — it uses a normal approximation to McNemar and
    assumes the discordant pairs split as the effect implies. Its job is not
    precision; it is to say *before* a grid is paid for whether the grid could
    possibly have detected what it is looking for. The 2026-08-24 run could not,
    and nothing said so.
    """
    if effect <= 0:
        raise ValueError("effect must be positive")
    upper = min(1.0, baseline + effect)
    # Discordant pairs under the alternative: at least the effect itself, plus the
    # symmetric pairs the baseline's own error rate contributes.
    psi = effect + 2 * min(baseline, 1 - upper)
    psi = max(psi, effect)
    z_power = _z_for(power)
    n = ((z * math.sqrt(psi) + z_power * math.sqrt(psi - effect**2)) / effect) ** 2
    return math.ceil(n)


def _z_for(power: float) -> float:
    """One-sided normal quantile, by bisection. Avoids a scipy dependency for one
    number that is needed once per invocation of a reporting command."""
    lo, hi = 0.0, 10.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if _normal_cdf(mid) < power:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def _normal_cdf(x: float) -> float:
    return 0.5 * (1 + math.erf(x / math.sqrt(2)))
