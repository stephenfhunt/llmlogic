"""The local seam of the CLI: `run`, `calibrate` and `resume` against a server.

None of this needs a model server either. `--dry-run` skips preflight — there is
nothing to hold a window to when nothing is served — and everything else fakes
`local.preflight`, which is what lets the plumbing for an overnight pass be
exercised without spending a night on it.

The rule these tests carry is the one the fourth silent misconfiguration bought
(`notes/a-local-subject.md`): a local sitting is defined by its **window** as
much as by its model, and an instrument that cannot rebuild the window it
measured under refuses rather than guessing.
"""

import json

import pytest

from harness import calibrate, cli, domains, local, resume
from harness.cli import main
from harness.record import RecordStore


@pytest.fixture
def offline(tmp_path, monkeypatch):
    """Results, slates and workspaces all under the test's own directory."""
    monkeypatch.setattr(cli, "RESULTS_ROOT", tmp_path / "results")
    monkeypatch.setattr(cli, "SLATES_ROOT", tmp_path / "slates")
    monkeypatch.setattr(cli, "WORKSPACE_ROOT", tmp_path / "workspaces")
    return tmp_path


@pytest.fixture
def served(monkeypatch):
    """A server that preflight is happy with, without one existing."""
    monkeypatch.setattr(local, "preflight", lambda *a, **k: [])


_LOCAL_STRENGTH = local.strengths(
    ["qwen3:14b"], ["structured"], "http://127.0.0.1:11434/v1", "none", 16384
)[0]


def latest_run(root):
    return sorted((root / "results").iterdir())[-1]


def meta_of(run_dir):
    return json.loads((run_dir / "run.json").read_text())


class TestACalibrationPassCanBeLocal:
    def test_it_runs_offline_and_records_the_subject_that_selected(self, offline):
        exit_code = main(
            [
                "calibrate",
                "--dry-run",
                "--domain",
                "controls",
                "--difficulty",
                "3",
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "16384",
            ]
        )
        assert exit_code == 0

        run_dir = latest_run(offline)
        meta = meta_of(run_dir)
        assert meta["local"]["models"] == ["qwen3:14b"]
        assert meta["local"]["protocols"] == ["structured"]
        assert meta["local"]["context_tokens"] == 16384
        assert meta["local"]["max_output_tokens"] == local.DEFAULT_MAX_OUTPUT_TOKENS
        assert meta["strengths"] == ["qwen3-14b-structured"]

    def test_the_manifest_says_which_subject_calibrated_it(self, offline):
        """`hypotheses.md` precondition 4: a slate is calibrated *for one
        subject*, so a manifest that cannot name it cannot be checked against
        the grid that runs it."""
        main(
            [
                "calibrate",
                "--dry-run",
                "--domain",
                "controls",
                "--difficulty",
                "3",
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "16384",
            ]
        )
        pool = json.loads((latest_run(offline) / "slate.json").read_text())["pool"]
        assert pool["strength"] == "qwen3-14b-structured"
        assert pool["model"] == "qwen3:14b"
        assert pool["local"]["protocol"] == "structured"
        assert pool["local"]["context_tokens"] == 16384

    def test_an_api_pass_still_records_the_strength_it_ran(self, offline):
        main(["calibrate", "--dry-run", "--domain", "controls", "--difficulty", "3"])
        pool = json.loads((latest_run(offline) / "slate.json").read_text())["pool"]
        assert pool["strength"] == calibrate.STRENGTH.name
        assert pool["local"] is None

    def test_two_protocols_are_two_subjects_and_are_refused(self, offline, capsys):
        """`tally` counts by task key across the run, so a two-strength pass
        would put six trials of one item under one rate — two subjects inside a
        band that means something for only one."""
        exit_code = main(
            [
                "calibrate",
                "--dry-run",
                "--domain",
                "controls",
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--protocol",
                "native",
            ]
        )
        assert exit_code == 1
        assert "one strength" in capsys.readouterr().err
        assert not (offline / "results").exists()

    def test_a_server_that_cannot_carry_it_refuses_before_any_cell(
        self, offline, monkeypatch, capsys
    ):
        monkeypatch.setattr(local, "preflight", lambda *a, **k: ["served at 4096, run wants 16384"])
        exit_code = main(
            [
                "calibrate",
                "--domain",
                "controls",
                "--local-model",
                "qwen3:14b",
                "--yes",
            ]
        )
        assert exit_code == 1
        assert "served at 4096" in capsys.readouterr().err
        assert not (offline / "results").exists()


class TestALocalRunCanBeResumed:
    """The 432-cell sweep finished, so this path had never been walked. An
    overnight pass is exactly where it would have been."""

    def _stopped(self, offline, window=16384, max_output_tokens=local.DEFAULT_MAX_OUTPUT_TOKENS):
        """A local run that recorded its metadata and then stopped, owing
        everything. Written directly rather than run, because a `--dry-run` is
        deliberately not resumable and a real one needs a server."""
        tasks = domains.load("controls")
        store = RecordStore(offline / "results", "run-local-test")
        store.write_metadata(
            dry_run=False,
            domains=["controls"],
            tasks=len(tasks),
            cells=len(tasks),
            strengths=["qwen3-14b-structured"],
            arms=["prose"],
            repeats=1,
            fingerprints=resume.fingerprints(tasks),
            ablate=None,
            max_turns=30,
            max_budget_usd=2.0,
            local={
                "endpoint": "http://127.0.0.1:11434/v1",
                "models": ["qwen3:14b"],
                "protocols": ["structured"],
                "reasoning_effort": "none",
                "max_cell_seconds": 240,
                "context_tokens": window,
                "max_output_tokens": max_output_tokens,
            },
            slate=None,
        )
        return store.dir

    def test_it_rebuilds_the_strengths_the_run_measured_under(
        self, offline, monkeypatch, served, capsys
    ):
        run_dir = self._stopped(offline)
        exit_code = main(["run", "--resume", str(run_dir)])
        assert exit_code == 2  # priced, and waiting for --yes
        out = capsys.readouterr().err
        assert "4 of 4 cells outstanding" in out
        assert "No API cost." in out

    def test_the_subject_it_resumes_with_is_the_local_one(self, offline, monkeypatch, served):
        run_dir = self._stopped(offline)
        chosen = []
        monkeypatch.setattr(
            cli, "run_grid", lambda cells, subject, *a, **k: chosen.append(subject) or _empty()
        )
        main(["run", "--resume", str(run_dir), "--yes"])
        assert isinstance(chosen[0], local.LocalSubject)

    def test_a_thinking_run_resumes_under_its_own_output_cap(self, offline, monkeypatch, served):
        """The cap bounds *reasoning plus answer*. A resume that fell back to
        the 2,048 default would truncate the second half's thoughts where the
        first half's completed — the same shape as the window this block
        already records, and the reason it is recorded beside it."""
        run_dir = self._stopped(offline, max_output_tokens=local.THINKING_MAX_OUTPUT_TOKENS)
        chosen = []
        monkeypatch.setattr(
            cli, "run_grid", lambda cells, subject, *a, **k: chosen.append(subject) or _empty()
        )
        main(["run", "--resume", str(run_dir), "--yes"])
        assert chosen[0].max_output_tokens == local.THINKING_MAX_OUTPUT_TOKENS

    def test_a_run_that_recorded_no_output_cap_falls_back_to_the_default(
        self, offline, monkeypatch, served
    ):
        """Runs recorded before the cap was a field resume at the default it
        was actually measured under, rather than being refused."""
        run_dir = self._stopped(offline)
        meta = meta_of(run_dir)
        del meta["local"]["max_output_tokens"]
        (run_dir / "run.json").write_text(json.dumps(meta))
        chosen = []
        monkeypatch.setattr(
            cli, "run_grid", lambda cells, subject, *a, **k: chosen.append(subject) or _empty()
        )
        main(["run", "--resume", str(run_dir), "--yes"])
        assert chosen[0].max_output_tokens == local.DEFAULT_MAX_OUTPUT_TOKENS

    def test_a_local_calibration_pass_resumes_from_its_pool(self, offline, served, capsys):
        """The pass a night is spent on is the one that most needs this: it
        rebuilds its slate from the recorded pool *and* its subject from the
        recorded local block, and neither existed for a local run before."""
        spec = calibrate.spec(
            [20260826], ["controls"], [3], ["in-context"], 3, _LOCAL_STRENGTH, 4096
        )
        candidates = calibrate.from_spec(spec)
        store = RecordStore(offline / "results", "cal-local-test")
        store.write_metadata(
            dry_run=False,
            domains=["controls"],
            tasks=len(candidates),
            cells=len(candidates) * 3,
            strengths=[_LOCAL_STRENGTH.name],
            arms=["prose"],
            repeats=3,
            fingerprints=resume.fingerprints([c.task for c in candidates]),
            ablate=None,
            max_turns=30,
            max_budget_usd=2.0,
            local={
                "endpoint": "http://127.0.0.1:11434/v1",
                "models": ["qwen3:14b"],
                "protocols": ["structured"],
                "reasoning_effort": "none",
                "max_cell_seconds": 240,
                "context_tokens": 16384,
            },
            calibration=spec,
        )

        assert main(["run", "--resume", str(store.dir)]) == 2
        out = capsys.readouterr().err
        assert f"{len(candidates) * 3} of {len(candidates) * 3} cells outstanding" in out
        assert "No API cost." in out

    def test_a_run_that_recorded_no_window_is_refused(self, offline, monkeypatch, served, capsys):
        run_dir = self._stopped(offline)
        meta = meta_of(run_dir)
        del meta["local"]["context_tokens"]
        (run_dir / "run.json").write_text(json.dumps(meta))

        exit_code = main(["run", "--resume", str(run_dir), "--yes"])
        assert exit_code == 1
        assert "no served context window" in capsys.readouterr().err

    def test_a_server_that_moved_under_the_run_is_refused(self, offline, monkeypatch, capsys):
        run_dir = self._stopped(offline)
        monkeypatch.setattr(local, "preflight", lambda *a, **k: ["served at 4096, run wants 16384"])
        exit_code = main(["run", "--resume", str(run_dir), "--yes"])
        assert exit_code == 1
        assert "served at 4096" in capsys.readouterr().err


def _empty():
    from harness.runner import RunResult

    return RunResult(run_id="run-test", records=[])
