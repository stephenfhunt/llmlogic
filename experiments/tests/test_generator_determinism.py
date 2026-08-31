"""A generated item must hash the same in every process, not just in this one.

`calibrate.load` regenerates a slate from ``(pack, seed, difficulty, track)`` and
refuses an item whose `resume.fingerprint` moved — that check is what makes a
manifest a reproduction rather than a hope. It has one unstated premise: that a
generator handed the same seed twice produces the same bytes twice.

`ontology` did not. Its diamond splice built a dict out of a **set of strings**
and handed the resulting list to `rng.choice`, and Python randomizes that
iteration order per process. The truth stayed put and the fixture moved
underneath it, so a ladder pinned in one process was refused by the run that
tried to use it — found 2026-08-30, on the first slate anything had ever been
asked to run from.

Two processes, two fixed hash seeds, because that is the only way to see it: a
single process is self-consistent by construction, which is exactly why the whole
existing suite passed while this was broken.
"""

from __future__ import annotations

import subprocess
import sys

import pytest

from harness import domains

PACKS = domains.generators()
DIFFICULTIES = (1, 2, 3, 4, 5)

#: Run under two fixed values rather than `random` twice. A flaky test that fails
#: on one seed in eight is a test that gets re-run until it passes; these two are
#: known to disagree on the `ontology` bug and will disagree deterministically on
#: the next one of its shape.
HASH_SEEDS = ("0", "1")

_PROGRAM = """
import json, sys
from harness import domains, resume

out = {}
for pack in %(packs)r:
    for difficulty in %(difficulties)r:
        for task in domains.generate(pack, %(seed)d, difficulty, "in-context"):
            out[task.key] = resume.fingerprint(task)
json.dump(out, sys.stdout)
"""


def _fingerprints(hash_seed: str, seed: int) -> dict[str, str]:
    program = _PROGRAM % {"packs": PACKS, "difficulties": DIFFICULTIES, "seed": seed}
    result = subprocess.run(
        [sys.executable, "-c", program],
        capture_output=True,
        text=True,
        check=True,
        env={"PYTHONHASHSEED": hash_seed, "PATH": "/usr/bin:/bin"},
    )
    return __import__("json").loads(result.stdout)


@pytest.mark.parametrize("seed", [20260830, 7919])
def test_the_same_seed_generates_the_same_items_in_a_different_process(seed):
    first = _fingerprints(HASH_SEEDS[0], seed)
    second = _fingerprints(HASH_SEEDS[1], seed)
    assert first, "the sweep generated nothing, so it asserts nothing"
    moved = sorted(key for key in first if first[key] != second.get(key))
    assert not moved, (
        f"{len(moved)} item(s) hash differently under a different PYTHONHASHSEED: "
        f"{', '.join(moved[:5])}. A manifest pins a fingerprint and regenerates "
        "the item to check it, so these cannot be run from a slate at all — and "
        "nothing else in the suite can see it, because one process agrees with "
        "itself."
    )
    assert first.keys() == second.keys()
