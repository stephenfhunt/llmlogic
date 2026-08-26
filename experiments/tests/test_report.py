"""How a run is read.

The report is where a run turns into a claim, so its arithmetic is load-bearing
in the same way the grader's is. Two things here are pinned because they were
wrong first:

- the pairing key. Keying on the task alone pooled opus and haiku as if they were
  two trials of one subject, which reported **24** paired observations for a grid
  that had 48 and averaged away the strength difference the grid exists to see.
- a run recorded before `engine_use` existed. Reading its missing classification
  as `answered-from: 0` would be a fabricated number, not a missing one.
"""

from __future__ import annotations

import json
from pathlib import Path

from harness.report import _mean_f1, _paired_units, render


def _record(**overrides) -> dict:
    base = {
        "run_id": "r",
        "cell_id": "c",
        "domain": "d",
        "task_id": "t1",
        "question_class": "recursion",
        "engine_expected_to_help": True,
        "track": "in-context",
        "arm": "prose",
        "strength": "opus-5",
        "model": "claude-opus-5",
        "ablated": None,
        "verdict": "correct",
        "missing": 0,
        "extra": 0,
        "truth_size": 4,
        "answer_raw": "",
        "first_program": None,
        "first_program_turn": None,
        "signals": {
            "engine_use": "none",
            "wrote_program": False,
            "ran_engine": False,
            "engine_calls": 0,
            "searches_after_engine": 0,
            "searches_total": 0,
            "rounds": 0,
        },
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_read_tokens": 0,
        "cache_creation_tokens": 0,
        "cost_usd": 0.0,
        "wall_seconds": 0.0,
        "turns": 1,
        "denials": [],
        "error": None,
        "recorded_at": "2026-08-25T00:00:00+00:00",
    }
    base.update(overrides)
    base.setdefault("cell_id", "c")
    return base


class TestPairing:
    def test_a_task_at_two_strengths_is_two_paired_observations(self):
        """The bug: pooling them reported half the observations the grid had, and
        averaged the two subjects whose difference is the point of running both."""
        records = [
            _record(arm="prose", task_id="t1", strength="opus-5", verdict="correct"),
            _record(arm="prose", task_id="t1", strength="haiku-4.5", verdict="wrong"),
        ]
        units = _paired_units(records, "prose")
        assert len(units) == 2
        assert set(units.values()) == {True, False}

    def test_repeat_trials_of_one_cell_do_collapse(self):
        """Trials are repeats of the *same* subject on the *same* task, so they
        are one observation — unlike two strengths."""
        records = [
            _record(arm="prose", cell_id="c1", verdict="correct"),
            _record(arm="prose", cell_id="c2", verdict="correct"),
            _record(arm="prose", cell_id="c3", verdict="wrong"),
        ]
        units = _paired_units(records, "prose")
        assert len(units) == 1
        assert next(iter(units.values())) is True  # 2 of 3

    def test_a_tie_across_trials_counts_as_incorrect(self):
        """The conservative direction: this harness's standing risk is flattering
        the engine, so an even split is not a win."""
        records = [
            _record(arm="prose", cell_id="c1", verdict="correct"),
            _record(arm="prose", cell_id="c2", verdict="wrong"),
        ]
        assert next(iter(_paired_units(records, "prose").values())) is False

    def test_the_other_arm_is_not_counted(self):
        records = [
            _record(arm="prose", task_id="t1"),
            _record(arm="engine", task_id="t1"),
        ]
        assert len(_paired_units(records, "prose")) == 1


class TestPartialCredit:
    def test_it_is_the_mean_over_items(self):
        records = [
            _record(verdict="correct", missing=0, extra=0, truth_size=4),
            _record(verdict="wrong", missing=4, extra=0, truth_size=4),
        ]
        assert _mean_f1(records) == "0.50"

    def test_a_run_without_truth_sizes_says_so_rather_than_guessing(self):
        """A run recorded before the field existed. Reconstructing the size from
        today's slate would grade it against an oracle it never saw — `scheduling`
        was repaired after the 2026-08-24 grid."""
        legacy = _record()
        del legacy["truth_size"]
        assert _mean_f1([legacy]) == "—"

    def test_one_missing_size_voids_the_column_rather_than_skewing_it(self):
        legacy = _record()
        del legacy["truth_size"]
        assert _mean_f1([_record(), legacy]) == "—"


class TestRender:
    def _run(self, tmp_path: Path, records: list[dict]) -> Path:
        run_dir = tmp_path / "run-x"
        run_dir.mkdir()
        (run_dir / "run.json").write_text("{}")
        with (run_dir / "records.jsonl").open("w") as handle:
            for index, record in enumerate(records):
                handle.write(json.dumps({**record, "cell_id": f"c{index}"}) + "\n")
        return run_dir

    def test_three_arms_render_three_comparisons(self, tmp_path):
        records = [
            _record(arm=arm, task_id=f"t{i}")
            for arm in ("prose", "engine", "engine-forced")
            for i in range(3)
        ]
        out = render(self._run(tmp_path, records))
        assert "engine-forced − prose" in out
        assert "engine − prose" in out
        assert "engine − engine-forced" in out

    def test_a_two_arm_run_renders_only_the_comparison_it_has(self, tmp_path):
        """A past run, or a `--arms` slice. It must not invent the missing arm."""
        records = [
            _record(arm=arm, task_id=f"t{i}") for arm in ("prose", "engine") for i in range(3)
        ]
        out = render(self._run(tmp_path, records))
        assert "engine − prose" in out
        # No invented comparison and no invented arm row. The explanatory prose
        # names all three arms whatever the run held, which is fine — it explains
        # the vocabulary, it does not report a number.
        assert "engine-forced − prose" not in out
        assert "| **engine-forced** |" not in out

    def test_the_two_tracks_get_their_own_tables(self, tmp_path):
        records = [_record(arm="prose", task_id="t1", track="in-context")]
        records += [_record(arm="prose", task_id="t2", track="at-scale")]
        out = render(self._run(tmp_path, records))
        assert "S1 — the measured slate (in-context)" in out
        assert "At scale" in out

    def test_a_track_with_no_cells_gets_no_table(self, tmp_path):
        records = [_record(arm="prose", task_id="t1", track="in-context")]
        assert "At scale" not in render(self._run(tmp_path, records))

    def test_an_unclassified_reach_is_reported_as_unknown_not_as_zero(self, tmp_path):
        signals = {
            "wrote_program": True,
            "ran_engine": True,
            "engine_calls": 1,
            "searches_after_engine": 0,
            "searches_total": 0,
            "rounds": 1,
        }
        records = [_record(arm="engine", task_id="t1", signals=signals)]
        out = render(self._run(tmp_path, records))
        assert "ran the engine: 1/1" in out
        assert "1/1 [" not in out  # no fabricated `answered-from` rate
