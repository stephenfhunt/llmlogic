"""A slate assembled by construction, and the ways it must not look like one that
was measured.

`calibrate` and `ladder` write the same manifest format, and that is the risk the
`selection` field exists for. A ladder's every entry carries `trials: 0` and
`correct: 0`, which in a calibrated manifest would mean *the subject answered
none of these* — the most damning number the format can hold — and here means
nothing ran at all. A reader that cannot tell those apart is a reader that will
retire the engine on a slate nobody screened.
"""

from __future__ import annotations

import json

import pytest

from harness import calibrate
from harness.cell import HAIKU_4_5, OPUS_5

SEED = 20260830
PACKS = ["access_control", "ontology"]
RUNGS = [1, 2, 3]


def build(questions=None, packs=None, difficulties=None):
    candidates = calibrate.pool(
        [SEED], packs=packs or PACKS, difficulties=difficulties or RUNGS, tracks=["in-context"]
    )
    if questions is not None:
        candidates = [c for c in candidates if calibrate.question(c) in set(questions)]
    spec = calibrate.spec(
        [SEED],
        packs or PACKS,
        difficulties or RUNGS,
        ["in-context"],
        trials=0,
        strength=HAIKU_4_5,
        arm=None,
    )
    return candidates, calibrate.ladder(candidates, spec)


class TestTheQuestionName:
    def test_it_strips_the_seed_tag_and_the_rung(self):
        for candidate in calibrate.pool([SEED], packs=["access_control"], difficulties=[4]):
            assert not calibrate.question(candidate).startswith("g")
            assert "-d4-" not in calibrate.question(candidate)

    def test_the_same_question_has_one_name_at_every_rung(self):
        """The whole point of the axis: a trajectory is one question, walked. If
        the name moved with the rung there would be no trajectory to walk."""
        names = {
            rung: {
                calibrate.question(c)
                for c in calibrate.pool([SEED], packs=["access_control"], difficulties=[rung])
            }
            for rung in RUNGS
        }
        assert len(set(map(frozenset, names.values()))) == 1


class TestWhatALadderKeeps:
    def test_it_keeps_every_candidate(self):
        candidates, payload = build()
        assert len(payload["kept"]) == len(candidates)
        assert payload["rejected"] == []

    def test_it_is_ordered_rung_ascending(self):
        """`load` preserves entry order and `run --limit N` stages a sitting off
        it, so the first rung is the gate. Ordered pack-major — which is how the
        pool comes out — the gate would be one pack at every difficulty."""
        _, payload = build()
        rungs = [entry["difficulty"] for entry in payload["kept"]]
        assert rungs == sorted(rungs)

    def test_a_filtered_ladder_holds_the_question_at_every_rung(self):
        _, payload = build(questions=["who-can-read"])
        assert [entry["difficulty"] for entry in payload["kept"]] == RUNGS

    def test_nothing_carries_a_measurement(self):
        _, payload = build()
        for entry in payload["kept"]:
            assert entry["trials"] == 0
            assert entry["correct"] == 0
            assert "not screened" in entry["reason"]


class TestTheManifestSaysItWasNotSelected:
    def test_selection_is_none_and_there_is_no_band(self):
        _, payload = build()
        assert payload["selection"] == calibrate.SELECTION_NONE
        assert payload["bands"] is None

    def test_the_pool_records_no_arm_and_no_trials(self):
        """Recording `prose` and three here would say a screen ran."""
        _, payload = build()
        assert payload["pool"]["arm"] is None
        assert payload["pool"]["trials"] == 0

    def test_it_still_names_the_subject_it_is_for(self):
        """There was no calibrating subject, but `run --slate` refuses a grid
        that does not hold the one the manifest names, and binding the ladder to
        Haiku is strictly safer than leaving it open."""
        _, payload = build()
        assert payload["pool"]["model"] == HAIKU_4_5.model
        assert calibrate.subject_moved(payload, (HAIKU_4_5,)) is None
        assert calibrate.subject_moved(payload, (OPUS_5,)) is not None


class TestReadingItBack:
    def test_it_round_trips_into_the_slate_it_names(self, tmp_path):
        candidates, payload = build(questions=["who-can-read", "instances-of"])
        path = calibrate.write(tmp_path / "ladder.json", payload)
        tasks = calibrate.load(path)
        assert [task.key for task in tasks] == [c.task.key for c in _ordered(candidates)]
        assert [task.difficulty for task in tasks] == sorted(t.difficulty for t in tasks)

    def test_a_moved_fixture_is_refused_like_any_other_slate(self, tmp_path):
        _, payload = build(questions=["who-can-read"])
        payload["kept"][0]["fingerprint"] = "0" * 16
        path = calibrate.write(tmp_path / "ladder.json", payload)
        with pytest.raises(calibrate.ManifestError, match="moved under this slate"):
            calibrate.load(path)

    def test_an_older_manifest_version_is_refused(self, tmp_path):
        _, payload = build()
        payload["version"] = 2
        path = calibrate.write(tmp_path / "ladder.json", payload)
        with pytest.raises(calibrate.ManifestError, match="manifest version"):
            calibrate.read(path)

    def test_a_manifest_that_cannot_say_how_it_chose_is_refused(self, tmp_path):
        """The v2 format could only mean *screened*, so a v3 payload without the
        field is one whose zeros are unreadable — not one to guess about."""
        _, payload = build()
        del payload["selection"]
        path = calibrate.write(tmp_path / "ladder.json", payload)
        with pytest.raises(calibrate.ManifestError, match="how its items were chosen"):
            calibrate.read(path)


class TestACalibratedManifestStillSaysWhatItIs:
    def test_select_writes_selection_band(self):
        candidates = calibrate.pool([SEED], packs=["access_control"], difficulties=[3])
        outcomes = {c.key: calibrate.Outcome(1, 3) for c in candidates}
        selection = calibrate.select(candidates, outcomes)
        spec = calibrate.spec([SEED], ["access_control"], [3], ["in-context"], trials=3)
        payload = calibrate.manifest(selection, spec)
        assert payload["selection"] == calibrate.SELECTION_BAND
        assert payload["pool"]["arm"] == calibrate.ARM


def _ordered(candidates):
    return sorted(candidates, key=lambda c: (c.difficulty, c.pack, c.task.id))


class TestTheCommand:
    """`harness ladder` end to end. It runs no cells at all, so *all* of it is
    the offline path — there is no paid half to leave untested."""

    @pytest.fixture
    def offline(self, tmp_path, monkeypatch):
        from harness import cli

        monkeypatch.setattr(cli, "SLATES_ROOT", tmp_path / "slates")
        return tmp_path

    def _main(self, *args):
        from harness.cli import main

        return main(["ladder", *args])

    def test_it_writes_a_ladder_the_grid_can_load(self, offline):
        out = offline / "ladder.json"
        assert (
            self._main(
                "--domain",
                "access_control",
                "--seed",
                str(SEED),
                "--difficulty",
                "1",
                "--difficulty",
                "2",
                "--question",
                "who-can-read",
                "--strength",
                "haiku-4.5",
                "--out",
                str(out),
            )
            == 0
        )
        payload = json.loads(out.read_text())
        assert payload["selection"] == calibrate.SELECTION_NONE
        assert [entry["difficulty"] for entry in payload["kept"]] == [1, 2]
        assert [task.difficulty for task in calibrate.load(out)] == [1, 2]

    def test_a_question_that_matches_nothing_is_refused(self, offline):
        """Silently dropping it writes a ladder missing one trajectory of eight,
        and a rung a subject never saw reads exactly like one it failed."""
        out = offline / "ladder.json"
        assert (
            self._main(
                "--domain",
                "access_control",
                "--seed",
                str(SEED),
                "--difficulty",
                "1",
                "--question",
                "who-can-read",
                "--question",
                "who-can-reed",
                "--out",
                str(out),
            )
            == 1
        )
        assert not out.exists()

    def test_an_unknown_strength_is_refused(self, offline):
        assert self._main("--seed", str(SEED), "--strength", "haiku-9") == 1

    def test_it_defaults_its_path_into_the_slates_directory(self, offline):
        assert self._main("--domain", "controls", "--seed", str(SEED), "--difficulty", "1") == 0
        assert (offline / "slates" / f"ladder-{SEED}.json").exists()
