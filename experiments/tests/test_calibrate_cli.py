"""`harness calibrate` end to end, offline.

The pass is the expensive part of this project's method, so the whole of it —
pool, cells, tally, band, manifest — has to run against the stub subject. A step
that can only be exercised by paying for it is a step that stops being exercised.
"""

import argparse
import json

import pytest

from harness import calibrate, cli
from harness.cli import main


@pytest.fixture
def offline(tmp_path, monkeypatch):
    """Results, slates and workspaces all under the test's own directory."""
    monkeypatch.setattr(cli, "RESULTS_ROOT", tmp_path / "results")
    monkeypatch.setattr(cli, "SLATES_ROOT", tmp_path / "slates")
    monkeypatch.setattr(cli, "WORKSPACE_ROOT", tmp_path / "workspaces")
    return tmp_path


def latest_run(root):
    return sorted((root / "results").iterdir())[-1]


class TestThePass:
    def test_it_runs_the_pool_and_pins_what_the_band_kept(self, offline):
        exit_code = main(["calibrate", "--dry-run", "--domain", "eligibility", "--difficulty", "3"])
        assert exit_code == 0

        run_dir = latest_run(offline)
        manifest = json.loads((run_dir / "slate.json").read_text())
        assert manifest["version"] == calibrate.MANIFEST_VERSION
        assert manifest["run_id"] == run_dir.name
        assert manifest["pool"]["arm"] == "prose"
        assert manifest["pool"]["trials"] == calibrate.TRIALS
        assert manifest["kept"]
        assert len(manifest["kept"]) + len(manifest["rejected"]) == 5

    def test_every_item_is_run_once_per_trial(self, offline):
        main(
            ["calibrate", "--dry-run", "--domain", "controls", "--difficulty", "3", "--trials", "3"]
        )
        run_dir = latest_run(offline)
        records = [
            json.loads(line) for line in (run_dir / "records.jsonl").read_text().splitlines()
        ]
        assert len(records) == 4 * 3
        assert {r["arm"] for r in records} == {"prose"}
        assert {r["strength"] for r in records} == {calibrate.STRENGTH.name}

    def test_a_dry_run_slate_stays_with_its_run(self, offline):
        """A stub's selection is evidence about the plumbing. `slates/` is where
        the real ones live, and committing a stub's next to them would make the
        two indistinguishable a month later."""
        main(["calibrate", "--dry-run", "--domain", "controls"])
        assert (latest_run(offline) / "slate.json").exists()
        assert not (offline / "slates").exists()

    def test_out_puts_the_manifest_where_it_is_told(self, offline):
        target = offline / "elsewhere" / "slate.json"
        main(["calibrate", "--dry-run", "--domain", "eligibility", "--out", str(target)])
        assert target.exists()

    def test_a_paid_pass_refuses_without_yes_and_leaves_no_trace(self, offline):
        """A refused pass is not a pass that happened, and `results/` records the
        ones that did."""
        assert main(["calibrate", "--domain", "controls"]) == 2
        assert not (offline / "results").exists()

    def test_a_pack_with_no_generator_is_refused_before_anything_runs(self, offline):
        assert main(["calibrate", "--dry-run", "--domain", "static_analysis"]) == 1
        assert not (offline / "results").exists()

    def test_an_unknown_strength_is_refused(self, offline):
        assert main(["calibrate", "--dry-run", "--strength", "gpt-9"]) == 1
        assert not (offline / "results").exists()

    def test_a_pass_that_keeps_nothing_says_so_and_writes_no_slate(self, offline, monkeypatch):
        """That is a finding about the pool, not a failure to report: the run's
        records stay, and only the slate is withheld."""
        monkeypatch.setattr(calibrate, "BAND", (1.5, 2.0))
        assert main(["calibrate", "--dry-run", "--domain", "controls"]) == 1
        run_dir = latest_run(offline)
        assert (run_dir / "records.jsonl").exists()
        assert not (run_dir / "slate.json").exists()


class TestReselection:
    def test_from_selects_again_without_running_anything(self, offline):
        main(["calibrate", "--dry-run", "--domain", "eligibility"])
        run_dir = latest_run(offline)
        before = (run_dir / "records.jsonl").read_text()

        target = offline / "again.json"
        assert main(["calibrate", "--from", str(run_dir), "--out", str(target)]) == 0

        assert (run_dir / "records.jsonl").read_text() == before
        assert (
            json.loads(target.read_text())["kept"]
            == json.loads((run_dir / "slate.json").read_text())["kept"]
        )

    def test_moving_the_band_reselects_from_the_same_evidence(self, offline, monkeypatch):
        """The band is a reading of the evidence, not part of collecting it —
        and re-running a stochastic subject to move it would quietly be a
        different pass."""
        main(["calibrate", "--dry-run", "--domain", "eligibility", "--domain", "controls"])
        run_dir = latest_run(offline)
        narrow = json.loads((run_dir / "slate.json").read_text())

        monkeypatch.setattr(calibrate, "BAND", (0.0, 1.0))
        target = offline / "everything.json"
        main(["calibrate", "--from", str(run_dir), "--out", str(target)])
        widened = json.loads(target.read_text())

        assert len(widened["kept"]) > len(narrow["kept"])
        assert widened["rejected"] == []

    def test_a_run_that_is_not_a_calibration_pass_is_refused(self, offline):
        main(["run", "--dry-run", "--domain", "controls"])
        run_dir = latest_run(offline)
        assert main(["calibrate", "--from", str(run_dir)]) == 1


class TestRunningTheSlate:
    def test_a_grid_runs_the_items_the_manifest_names(self, offline):
        main(["calibrate", "--dry-run", "--domain", "eligibility"])
        slate = latest_run(offline) / "slate.json"
        kept = {entry["id"] for entry in json.loads(slate.read_text())["kept"]}

        assert main(["run", "--dry-run", "--slate", str(slate)]) == 0

        run_dir = latest_run(offline)
        records = [
            json.loads(line) for line in (run_dir / "records.jsonl").read_text().splitlines()
        ]
        assert {r["task_id"] for r in records} == kept
        meta = json.loads((run_dir / "run.json").read_text())
        assert meta["slate"]["digest"] == calibrate.digest(slate)

    def test_a_slate_whose_items_moved_is_refused(self, offline):
        main(["calibrate", "--dry-run", "--domain", "eligibility"])
        slate = latest_run(offline) / "slate.json"
        payload = json.loads(slate.read_text())
        payload["kept"][0]["fingerprint"] = "0" * 16
        slate.write_text(json.dumps(payload))

        assert main(["run", "--dry-run", "--slate", str(slate)]) == 1

    def test_a_run_without_a_slate_still_runs_the_pinned_one(self, offline):
        assert main(["run", "--dry-run", "--domain", "controls"]) == 0
        meta = json.loads((latest_run(offline) / "run.json").read_text())
        assert meta["slate"] is None


class TestRebuildingASlate:
    """What `--resume` rebuilds a stopped run's grid from.

    Three provenances now, and a resume that picks the wrong one joins two
    different experiments under one run id — the failure `resume.moved` was
    written for, one level up.
    """

    def test_a_calibration_pass_rebuilds_its_pool(self, offline):
        main(["calibrate", "--dry-run", "--domain", "controls", "--difficulty", "2"])
        run_dir = latest_run(offline)
        meta = json.loads((run_dir / "run.json").read_text())

        rebuilt = cli._slate_of(run_dir, meta)

        assert [task.key for task in rebuilt] == [
            candidate.key for candidate in calibrate.from_spec(meta["calibration"])
        ]

    def test_a_calibrated_run_rebuilds_from_its_manifest(self, offline):
        main(["calibrate", "--dry-run", "--domain", "eligibility"])
        slate = latest_run(offline) / "slate.json"
        main(["run", "--dry-run", "--slate", str(slate)])
        run_dir = latest_run(offline)
        meta = json.loads((run_dir / "run.json").read_text())

        rebuilt = cli._slate_of(run_dir, meta)

        assert [task.key for task in rebuilt] == [task.key for task in calibrate.load(slate)]

    def test_a_manifest_edited_since_the_run_is_refused(self, offline):
        main(["calibrate", "--dry-run", "--domain", "eligibility"])
        slate = latest_run(offline) / "slate.json"
        main(["run", "--dry-run", "--slate", str(slate)])
        run_dir = latest_run(offline)
        meta = json.loads((run_dir / "run.json").read_text())

        payload = json.loads(slate.read_text())
        payload["kept"].pop()
        slate.write_text(json.dumps(payload))

        with pytest.raises(calibrate.ManifestError, match="not the slate that was measured"):
            cli._slate_of(run_dir, meta)

    def test_an_ordinary_run_rebuilds_the_pinned_slate(self, offline):
        main(["run", "--dry-run", "--domain", "controls"])
        run_dir = latest_run(offline)
        meta = json.loads((run_dir / "run.json").read_text())

        assert {task.domain for task in cli._slate_of(run_dir, meta)} == {"controls"}


def test_resuming_a_repeated_run_rebuilds_every_trial(offline):
    """`repeats` is part of the grid's shape. Rebuilding without it produced
    only trial 0, and every later trial in the record then read as a stranger —
    a resume that refused itself. Nothing owed a repeated run before now, because
    nothing had run one."""
    main(["run", "--dry-run", "--domain", "controls", "--repeats", "2"])
    run_dir = latest_run(offline)
    meta = json.loads((run_dir / "run.json").read_text())
    # `--resume` refuses a dry run by design, so the rebuild is exercised
    # directly; what broke was the shape of the grid, not the refusal.
    from harness.cell import STRENGTHS, grid

    tasks = cli._slate_of(run_dir, meta)
    cells = grid(tasks, STRENGTHS, tuple(meta["arms"]), meta.get("ablate"), meta.get("repeats", 1))

    assert len(cells) == meta["cells"]
    recorded = {
        json.loads(line)["cell_id"] for line in (run_dir / "records.jsonl").read_text().splitlines()
    }
    assert recorded - {cell.id for cell in cells} == set()


def test_the_resume_path_accepts_a_completed_repeated_run(offline, monkeypatch):
    """The end of the same defect: `cmd_resume` rebuilt the grid without
    `repeats` and then refused the run's own cells as strangers."""
    main(["run", "--dry-run", "--domain", "controls", "--repeats", "2"])
    run_dir = latest_run(offline)
    meta = json.loads((run_dir / "run.json").read_text())
    meta["dry_run"] = False  # `resume` refuses a dry run before it rebuilds anything
    (run_dir / "run.json").write_text(json.dumps(meta))

    args = argparse.Namespace(resume=str(run_dir), limit=None, budget=2.0, max_turns=30, yes=False)
    assert cli.cmd_resume(args) == 0
