"""A slate is calibrated for one subject, and the grid has to hold that subject.

`calibrate.load` checks the *items* — fixture, question, truth — and said nothing
about **who** the band was drawn for. An item's difficulty is not a property of
the item alone (`hypotheses.md`, precondition 4), so a slate calibrated with
thinking on is not calibrated for the same model with `--reasoning-effort none`,
and that grid used to run without a word. Precondition 4 failing silently is this
project's standing failure mode; these tests are the check that it cannot.
"""

import json

import pytest

from harness import calibrate, cli, local
from harness.cell import HAIKU_4_5, OPUS_5
from harness.cli import main

ENDPOINT = "http://127.0.0.1:11434/v1"
SEED = 20260826


def qwen(protocol="structured", effort=None, window=24576, model="qwen3:14b"):
    """One locally-served strength, spelled the way `run` builds it."""
    return local.strengths([model], [protocol], ENDPOINT, effort, window)[0]


def manifest_for(strength, max_output_tokens=None, packs=("controls",)):
    """A manifest that kept something, recorded as selected by this subject."""
    drawn = calibrate.pool([SEED], packs=list(packs), difficulties=[1])
    selection = calibrate.select(drawn, {c.key: calibrate.Outcome(1, 3) for c in drawn})
    spec = calibrate.spec([SEED], list(packs), [1], ["in-context"], 3, strength, max_output_tokens)
    return calibrate.manifest(selection, spec, run_id="cal-test")


class TestTheSubjectAGridOffers:
    """`subject_moved` against one strength — the flag-by-flag comparison."""

    def test_the_same_subject_is_not_a_move(self):
        payload = manifest_for(qwen(), 4096)
        assert calibrate.subject_moved(payload, [qwen()], 4096) is None

    def test_thinking_turned_off_under_a_thinking_on_slate_is_refused(self):
        """The case the open question named, and the reason it is worth a night:
        `run --slate <thinking-on slate> --reasoning-effort none` was accepted."""
        payload = manifest_for(qwen(effort=None), 4096)
        moved = calibrate.subject_moved(payload, [qwen(effort="none")], 4096)
        assert moved is not None
        assert "reasoning_effort" in moved
        assert "precondition 4" in moved

    def test_a_narrower_window_is_refused_and_named(self):
        payload = manifest_for(qwen(window=24576), 4096)
        moved = calibrate.subject_moved(payload, [qwen(window=16384)], 4096)
        assert "context_tokens" in moved
        assert "24576" in moved and "16384" in moved

    def test_a_different_output_cap_is_refused_and_named(self):
        """The newest of the four flags, and the one `Strength` does not carry:
        it bounds reasoning plus answer, so 2048 and 4096 are two subjects."""
        payload = manifest_for(qwen(), 4096)
        moved = calibrate.subject_moved(payload, [qwen()], 2048)
        assert "max_output_tokens" in moved
        assert "4096" in moved and "2048" in moved

    def test_a_different_tool_protocol_is_refused_and_named(self):
        payload = manifest_for(qwen(protocol="structured"), 4096)
        moved = calibrate.subject_moved(payload, [qwen(protocol="native")], 4096)
        assert "protocol" in moved
        assert "structured" in moved and "native" in moved

    def test_a_different_model_is_refused_and_named(self):
        payload = manifest_for(qwen(model="qwen3:14b"), 4096)
        moved = calibrate.subject_moved(payload, [qwen(model="qwen3:8b")], 4096)
        assert "model" in moved
        assert "qwen3:14b" in moved and "qwen3:8b" in moved

    def test_the_same_model_served_from_another_url_is_the_same_subject(self):
        """`endpoint` is excluded on purpose (`calibrate.SUBJECT_FIELDS`). A
        moved port is about the machine, not about what the subject does, and
        refusing on it would teach the next session to ignore this check."""
        payload = manifest_for(qwen(), 4096)
        elsewhere = local.strengths(
            ["qwen3:14b"], ["structured"], "http://gpu:11434/v1", None, 24576
        )
        assert calibrate.subject_moved(payload, elsewhere, 4096) is None


class TestLocalAgainstApi:
    def test_a_local_slate_run_against_an_api_grid_is_refused(self):
        payload = manifest_for(qwen(), 4096)
        moved = calibrate.subject_moved(payload, [HAIKU_4_5], None)
        assert "served locally" in moved and "an API model" in moved

    def test_an_api_slate_run_against_a_local_grid_is_refused(self):
        payload = manifest_for(HAIKU_4_5)
        moved = calibrate.subject_moved(payload, [qwen()], 4096)
        assert "served locally" in moved and "an API model" in moved


class TestContainmentNotEquality:
    """A grid may cross strengths; what it may not do is drop the calibrator.

    `hypotheses.md` reads the primary endpoint at the weaker strength while the
    grid runs both, so demanding a single exact match would refuse the run the
    slate exists for.
    """

    def test_the_calibrator_among_several_strengths_is_accepted(self):
        payload = manifest_for(HAIKU_4_5)
        assert calibrate.subject_moved(payload, [OPUS_5, HAIKU_4_5], None) is None

    def test_a_crossing_without_the_calibrator_is_refused(self):
        payload = manifest_for(HAIKU_4_5)
        moved = calibrate.subject_moved(payload, [OPUS_5], None)
        assert "model" in moved and OPUS_5.model in moved

    def test_several_strengths_none_of_them_the_calibrator_lists_them(self):
        payload = manifest_for(qwen(effort=None), 4096)
        offered = local.strengths(["qwen3:14b"], ["native", "structured"], ENDPOINT, "none", 24576)
        moved = calibrate.subject_moved(payload, offered, 4096)
        assert "not among the 2 run" in moved
        assert all(strength.name in moved for strength in offered)


@pytest.fixture
def offline(tmp_path, monkeypatch):
    monkeypatch.setattr(cli, "RESULTS_ROOT", tmp_path / "results")
    monkeypatch.setattr(cli, "SLATES_ROOT", tmp_path / "slates")
    monkeypatch.setattr(cli, "WORKSPACE_ROOT", tmp_path / "workspaces")
    return tmp_path


def local_slate(root, *extra):
    """A dry-run calibration pass against the local subject, and its manifest."""
    assert (
        main(
            [
                "calibrate",
                "--dry-run",
                "--domain",
                "eligibility",
                "--difficulty",
                "1",
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "24576",
                "--max-output-tokens",
                "4096",
                *extra,
            ]
        )
        == 0
    )
    return sorted((root / "results").iterdir())[-1] / "slate.json"


class TestTheCommandRefusesBeforeAnyCellRuns:
    def test_the_grid_the_slate_was_calibrated_for_runs(self, offline):
        slate = local_slate(offline)
        before = len(list((offline / "results").iterdir()))
        exit_code = main(
            [
                "run",
                "--dry-run",
                "--slate",
                str(slate),
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "24576",
                "--max-output-tokens",
                "4096",
            ]
        )
        assert exit_code == 0
        assert len(list((offline / "results").iterdir())) == before + 1

    def test_the_same_grid_with_thinking_off_is_refused_and_records_nothing(self, offline, capsys):
        slate = local_slate(offline)
        before = sorted((offline / "results").iterdir())
        exit_code = main(
            [
                "run",
                "--dry-run",
                "--slate",
                str(slate),
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "24576",
                "--max-output-tokens",
                "4096",
                "--reasoning-effort",
                "none",
            ]
        )
        assert exit_code == 1
        assert "reasoning_effort" in capsys.readouterr().err
        assert sorted((offline / "results").iterdir()) == before

    def test_a_narrower_window_is_refused_by_the_command(self, offline, capsys):
        slate = local_slate(offline)
        exit_code = main(
            [
                "run",
                "--dry-run",
                "--slate",
                str(slate),
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "16384",
                "--max-output-tokens",
                "4096",
            ]
        )
        assert exit_code == 1
        assert "context_tokens" in capsys.readouterr().err

    def test_an_api_grid_still_runs_a_slate_its_own_strength_calibrated(self, offline):
        """The default path: the pinned strengths carry `calibrate.STRENGTH`, so
        the ordinary `run --slate` is untouched by the guard."""
        assert main(["calibrate", "--dry-run", "--domain", "eligibility"]) == 0
        slate = sorted((offline / "results").iterdir())[-1] / "slate.json"
        assert main(["run", "--dry-run", "--slate", str(slate)]) == 0

    def test_an_api_grid_narrowed_away_from_the_calibrator_is_refused(self, offline, capsys):
        assert main(["calibrate", "--dry-run", "--domain", "eligibility"]) == 0
        slate = sorted((offline / "results").iterdir())[-1] / "slate.json"
        exit_code = main(["run", "--dry-run", "--slate", str(slate), "--strength", OPUS_5.name])
        assert exit_code == 1
        assert "calibrated" in capsys.readouterr().err

    def test_a_resume_rebuilds_the_subject_it_would_be_checked_against(self, offline):
        """Which is why the guard is on `run --slate` and not on `--resume`.

        A resume takes its strengths from the run's own `run.json`, not from the
        flags given now, so the subject it rebuilds is the one the manifest was
        checked against when the run started. There is nothing left for the
        guard to disagree with — and this is the assertion that says so, rather
        than a comment claiming it.
        """
        slate = local_slate(offline)
        main(
            [
                "run",
                "--dry-run",
                "--slate",
                str(slate),
                "--local-model",
                "qwen3:14b",
                "--protocol",
                "structured",
                "--min-context",
                "24576",
                "--max-output-tokens",
                "4096",
            ]
        )
        run_dir = sorted((offline / "results").iterdir())[-1]
        meta = json.loads((run_dir / "run.json").read_text())

        rebuilt = cli._local_strengths_of(meta["local"], meta["strengths"])

        payload = json.loads(slate.read_text())
        cap = meta["local"]["max_output_tokens"]
        assert cap == payload["pool"]["local"]["max_output_tokens"]
        assert calibrate.subject_moved(payload, rebuilt, cap) is None
