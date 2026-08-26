"""Selecting a slate from a generated pool.

The pass itself is the cheap part — one arm, one strength. What has to be right
is everything around it: that the pool is what it says it is, that a cell which
failed is not counted as a wrong answer, that the band keeps what
`decisions.md` says it keeps, and that a manifest reproduces the slate it names
rather than merely describing it.
"""

import json

import pytest

from harness import calibrate, domains, generate, resume
from harness.calibrate import Candidate, Outcome, PoolError

SEED = 20260826


def write_records(run_dir, rows):
    """A run directory holding exactly these records, in this order."""
    run_dir.mkdir(parents=True, exist_ok=True)
    with (run_dir / "records.jsonl").open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row) + "\n")
    return run_dir


def record(cell_id, domain, task_id, verdict, arm="prose", error=None):
    """The fields `tally` reads. Not a Record — a record as it is on disk, which
    is what a pass actually reads back and what a past run left behind."""
    return {
        "cell_id": cell_id,
        "domain": domain,
        "task_id": task_id,
        "verdict": verdict,
        "arm": arm,
        "error": error,
    }


class TestPool:
    def test_it_draws_every_pack_that_has_a_generator(self):
        drawn = calibrate.pool([SEED])
        assert sorted({candidate.pack for candidate in drawn}) == sorted(domains.generators())
        assert "static_analysis" not in {candidate.pack for candidate in drawn}

    def test_it_records_the_provenance_that_reproduces_each_item(self):
        """A `Task` knows its difficulty and track but not its seed, and a
        manifest that cannot name the seed cannot regenerate the item."""
        for candidate in calibrate.pool([SEED], difficulties=[2], tracks=["in-context"]):
            assert candidate.seed == SEED
            assert candidate.difficulty == 2 == candidate.task.difficulty
            assert candidate.track == "in-context" == candidate.task.track

    def test_the_same_spec_draws_the_same_pool(self):
        first = calibrate.pool([SEED], difficulties=[1, 3])
        second = calibrate.pool([SEED], difficulties=[1, 3])
        assert [c.key for c in first] == [c.key for c in second]
        assert [resume.fingerprint(c.task) for c in first] == [
            resume.fingerprint(c.task) for c in second
        ]

    def test_two_difficulties_are_two_items_not_one(self):
        drawn = calibrate.pool([SEED], packs=["controls"], difficulties=[1, 2])
        assert len(drawn) == 2 * len(domains.generate("controls", SEED, 1))
        assert len({candidate.key for candidate in drawn}) == len(drawn)

    def test_a_pack_with_no_generator_is_refused_with_its_reason(self):
        with pytest.raises(PoolError, match="nothing to seed"):
            calibrate.pool([SEED], packs=["static_analysis"])

    def test_two_seeds_that_collide_in_an_id_are_refused(self):
        """An id carries the seed as its low four hex digits, so 0x10000 and
        0x20000 produce the same key — the same cell id, and one item's trials
        tallied against another's."""
        with pytest.raises(PoolError, match="low four hex digits"):
            calibrate.pool([0x10000, 0x20000], packs=["controls"])

        with pytest.raises(PoolError) as caught:
            calibrate.pool([0x10000, 0x20000], packs=["controls"])
        assert "0x10000" in str(caught.value) and "0x20000" in str(caught.value)

    def test_a_degenerate_item_stops_the_pool_rather_than_shrinking_it(self, monkeypatch):
        """`generate.validate` raises by design: a generator that silently drops
        a third of its items is a generator whose difficulty no longer means what
        it says. Building a pool is offline, so refusing costs nothing."""
        real = domains.validate
        seen = []

        def refuse_the_third(name, task):
            seen.append(task.key)
            if len(seen) == 3:
                raise generate.Degenerate(f"{task.id}: empty truth")
            real(name, task)

        monkeypatch.setattr(domains, "validate", refuse_the_third)
        with pytest.raises(generate.Degenerate, match="empty truth"):
            calibrate.pool([SEED], packs=["controls"])


class TestTally:
    def test_it_counts_correct_over_trials_per_item(self, tmp_path):
        run_dir = write_records(
            tmp_path / "cal",
            [
                record("controls.a.prose.haiku-4.5", "controls", "a", "correct"),
                record("controls.a.prose.haiku-4.5#1", "controls", "a", "wrong"),
                record("controls.a.prose.haiku-4.5#2", "controls", "a", "correct"),
            ],
        )
        outcomes = calibrate.tally(run_dir)
        assert outcomes["controls/a"] == Outcome(correct=2, trials=3)
        assert outcomes["controls/a"].rate == pytest.approx(2 / 3)

    def test_a_failed_cell_is_not_a_trial(self, tmp_path):
        """An API error is a cell that did not run. Counting it as a miss makes
        an item look hard because the instrument broke — which is exactly the
        direction that would select a broken item onto the slate."""
        run_dir = write_records(
            tmp_path / "cal",
            [
                record("c.a.prose.haiku-4.5", "controls", "a", "correct"),
                record("c.a.prose.haiku-4.5#1", "controls", "a", "error"),
                record(
                    "c.a.prose.haiku-4.5#2",
                    "controls",
                    "a",
                    "no-answer",
                    error="ResultError: session limit",
                ),
            ],
        )
        assert calibrate.tally(run_dir)["controls/a"] == Outcome(correct=1, trials=1)

    def test_a_resumed_cell_counts_once(self, tmp_path):
        """Both attempts stay in `records.jsonl` — that file is the record of
        what happened. The tally is the reading of it, and the last attempt is
        the one that produced a verdict."""
        run_dir = write_records(
            tmp_path / "cal",
            [
                record("c.a.prose.haiku-4.5", "controls", "a", "error"),
                record("c.a.prose.haiku-4.5", "controls", "a", "correct"),
            ],
        )
        assert calibrate.tally(run_dir)["controls/a"] == Outcome(correct=1, trials=1)

    def test_another_arm_is_not_this_pass(self, tmp_path):
        run_dir = write_records(
            tmp_path / "cal",
            [
                record("c.a.prose.haiku-4.5", "controls", "a", "wrong"),
                record("c.a.engine.haiku-4.5", "controls", "a", "correct", arm="engine"),
            ],
        )
        assert calibrate.tally(run_dir)["controls/a"] == Outcome(correct=0, trials=1)

    def test_an_unmeasured_item_has_no_rate(self):
        """0.0 would read as *always wrong*, which on the at-scale floor band is
        a keep — an item selected on no evidence at all."""
        unmeasured = Outcome(correct=0, trials=0)
        with pytest.raises(ValueError, match="no trials"):
            _ = unmeasured.rate


def one(track, difficulty=3):
    """A single candidate on the given track, for the band tests."""
    pack = "controls"
    task = domains.generate(pack, SEED, difficulty)[0]
    return Candidate(pack=pack, seed=SEED, difficulty=difficulty, track=track, task=task)


class TestBand:
    @pytest.mark.parametrize(
        "correct,kept,reason",
        [
            (0, False, "too hard"),
            (1, True, "in band"),
            (2, True, "in band"),
            (3, False, "too easy"),
        ],
    )
    def test_the_four_rates_three_trials_can_produce(self, correct, kept, reason):
        """Three trials exist so the band can be expressed at all: at one trial
        the reachable rates are 0 and 1, and both are outside it."""
        candidate = one("in-context")
        selection = calibrate.select([candidate], {candidate.key: Outcome(correct, 3)})
        assert selection.judged[0].kept is kept
        assert reason in selection.judged[0].reason

    @pytest.mark.parametrize(
        "correct,kept",
        [(0, True), (1, True), (2, False), (3, False)],
    )
    def test_at_scale_is_a_floor_not_a_middle(self, correct, kept):
        """That track exceeds the prose arm's window on purpose, so ~0 is what it
        claims. An at-scale item prose answers well is a defective one."""
        candidate = one("at-scale")
        selection = calibrate.select([candidate], {candidate.key: Outcome(correct, 3)})
        assert selection.judged[0].kept is kept

    def test_an_at_scale_item_prose_answered_says_what_that_means(self):
        candidate = one("at-scale")
        selection = calibrate.select([candidate], {candidate.key: Outcome(3, 3)})
        assert "did not defeat" in selection.judged[0].reason

    def test_an_unmeasured_item_is_never_kept_on_either_track(self):
        for track in ("in-context", "at-scale"):
            candidate = one(track)
            selection = calibrate.select([candidate], {})
            assert selection.kept == []
            assert selection.judged[0].reason == "never measured"

    def test_every_candidate_comes_back_judged(self):
        """A pass that keeps nothing has to be readable — that is the finding."""
        drawn = calibrate.pool([SEED], packs=["controls"])
        selection = calibrate.select(drawn, {c.key: Outcome(3, 3) for c in drawn})
        assert selection.kept == []
        assert len(selection.rejected) == len(drawn)
        assert all("too easy" in judged.reason for judged in selection.rejected)


class TestManifest:
    def _written(self, tmp_path, correct_for):
        drawn = calibrate.pool([SEED], packs=["controls", "eligibility"])
        outcomes = {c.key: Outcome(correct_for(c), 3) for c in drawn}
        selection = calibrate.select(drawn, outcomes)
        spec = calibrate.spec([SEED], ["controls", "eligibility"], [3], ["in-context"], 3)
        path = calibrate.write(
            tmp_path / "slate.json", calibrate.manifest(selection, spec, run_id="cal-test")
        )
        return drawn, selection, path

    def test_it_round_trips_to_the_tasks_it_selected(self, tmp_path):
        _, selection, path = self._written(tmp_path, lambda c: 1)
        loaded = calibrate.load(path)
        assert [task.key for task in loaded] == [task.key for task in selection.tasks]
        for task, expected in zip(loaded, selection.tasks, strict=True):
            assert task.fixture.files == expected.fixture.files
            assert task.truth == expected.truth
            assert task.question == expected.question

    def test_it_records_the_rejected_items_and_their_rates(self, tmp_path):
        """A slate that says only what it kept cannot answer the question the
        next session asks of it: was the pool too easy, or did the pass not run?"""
        drawn, _, path = self._written(tmp_path, lambda c: 3 if c.pack == "controls" else 1)
        payload = calibrate.read(path)
        rejected = {entry["id"]: entry for entry in payload["rejected"]}
        assert rejected and all("too easy" in entry["reason"] for entry in rejected.values())
        assert len(payload["kept"]) + len(rejected) == len(drawn)
        assert all(entry["trials"] == 3 for entry in payload["kept"])

    def test_an_entry_carries_what_it_was_drawn_from(self, tmp_path):
        _, _, path = self._written(tmp_path, lambda c: 1)
        entry = calibrate.read(path)["kept"][0]
        assert entry["seed"] == SEED
        assert entry["difficulty"] == 3
        assert entry["track"] == "in-context"
        assert entry["fingerprint"]
        assert entry["question_class"]

    def test_a_manifest_from_another_version_is_refused(self, tmp_path):
        _, _, path = self._written(tmp_path, lambda c: 1)
        payload = json.loads(path.read_text())
        payload["version"] = calibrate.MANIFEST_VERSION + 1
        path.write_text(json.dumps(payload))
        with pytest.raises(calibrate.ManifestError, match="manifest version"):
            calibrate.load(path)

    def test_a_fixture_that_moved_under_the_slate_is_refused(self, tmp_path):
        """The manifest holds provenance and a fingerprint, not fixtures. The
        fingerprint is what turns *the generator is deterministic* from a hope
        into a check — `resume.fingerprint`'s 2026-08-24 lesson, pointed at the
        source tree instead of at a resumed run."""
        _, _, path = self._written(tmp_path, lambda c: 1)
        payload = json.loads(path.read_text())
        payload["kept"][0]["fingerprint"] = "0" * 16
        path.write_text(json.dumps(payload))
        with pytest.raises(calibrate.ManifestError, match="moved under this slate"):
            calibrate.load(path)

    def test_an_id_the_generator_no_longer_produces_is_refused(self, tmp_path):
        _, _, path = self._written(tmp_path, lambda c: 1)
        payload = json.loads(path.read_text())
        payload["kept"][0]["id"] = "a-question-nobody-generated"
        path.write_text(json.dumps(payload))
        with pytest.raises(calibrate.ManifestError, match="no longer produces"):
            calibrate.load(path)

    def test_a_manifest_that_selected_nothing_is_not_a_slate(self, tmp_path):
        _, _, path = self._written(tmp_path, lambda c: 3)
        with pytest.raises(calibrate.ManifestError, match="selected no items"):
            calibrate.load(path)

    def test_a_missing_manifest_says_so(self, tmp_path):
        with pytest.raises(calibrate.ManifestError, match="no manifest at"):
            calibrate.load(tmp_path / "absent.json")

    def test_the_digest_moves_when_the_slate_does(self, tmp_path):
        _, _, path = self._written(tmp_path, lambda c: 1)
        before = calibrate.digest(path)
        payload = json.loads(path.read_text())
        payload["kept"].pop()
        path.write_text(json.dumps(payload))
        assert calibrate.digest(path) != before
