"""Selecting the slate, instead of designing it.

An item the subject scores 0% or 100% on carries almost no information about
whether the engine helped: either both arms get it right or neither does, and the
difference the experiment exists to measure has no room to appear. The
informative band is the middle (``decisions.md`` 2026-08-25).

So the slate is **selected, not written**: generate a large pool, run one cheap
pass over it — the prose arm at the weakest strength — and keep the items whose
accuracy lands in the band. This is the check that would have caught the
2026-08-24 ceiling *before* 112 cells were paid for; opus was 20/20 in prose on
the live domains and nothing said so until afterwards.

What is kept is pinned to a **manifest**, and a measured grid runs from that.
Re-running the pass is not how a slate is reproduced — regenerating from
``(pack, seed, difficulty, track)`` and checking each fingerprint is, which is
why `load` refuses an item whose fixture moved under it.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

from harness import domains, report, resume
from harness.cell import HAIKU_4_5, Strength
from harness.record import RecordStore
from harness.task import Task

#: The manifest format. Bumped when a field changes meaning, so a manifest
#: written by an older harness is refused rather than half-understood.
MANIFEST_VERSION = 1

#: Trials per pool item. **One trial is 0 or 1**, so a band over the middle is
#: empty by construction and the whole pass selects nothing. At three the
#: reachable accuracies are {0, ⅓, ⅔, 1} and `BAND` keeps exactly ⅓ and ⅔.
#: Coarse on purpose: calibration is a screen run before the money, not the
#: measurement — that is the grid, with `--repeats` and `stats.mcnemar`.
TRIALS = 3

#: The informative band for an ``in-context`` item, from `decisions.md`
#: 2026-08-25. Read against a three-trial rate it means *not unanimous*: an item
#: the weak subject always gets right has no headroom for the engine to show in,
#: and one it never gets right cannot distinguish a helped subject from a lost
#: one either.
BAND = (0.2, 0.8)

#: The ceiling for an ``at-scale`` item — a floor band, not a middle one. That
#: track exceeds the prose arm's window on purpose, so prose scoring ~0 is what
#: the track *claims* and is the evidence it is doing its job. An at-scale item
#: prose answers well is a defective at-scale item: its fixture did not, in
#: fact, defeat the arm it was built to defeat. Only running the pass can say
#: which items those are, which is why the track is calibrated rather than
#: assumed.
AT_SCALE_CEILING = 1 / 3

#: Both bands are compared against a ratio of small integers, and ⅓ is not
#: exactly representable. Without this, whether ``1/3 <= 1/3`` decides an item
#: depends on how each side was computed.
TOLERANCE = 1e-9

#: A pool the size of one sitting. The pass is 29 items per
#: ``(seed, difficulty, track)`` combination and `TRIALS` cells each, so the
#: default draws 87 items into 261 cells — a few hours of haiku at the account's
#: window, sized by `--limit` and finished by `--resume` like any other run.
#: Widening the pool is a flag, not an edit.
DEFAULT_SEEDS = (20260826,)
DEFAULT_DIFFICULTIES = (2, 3, 4)

#: The pass is one arm at one strength — that is what makes it cheap enough to
#: run before a grid. The engine arm is deliberately absent: what is being
#: measured here is whether the *question* has headroom, not whether the engine
#: fills it.
ARM = "prose"
STRENGTH = HAIKU_4_5


class PoolError(Exception):
    """The pool could not be built as specified."""


class ManifestError(Exception):
    """A manifest cannot be loaded back into the slate it names."""


@dataclass(frozen=True)
class Candidate:
    """One pool item, with the provenance that reproduces it.

    ``Task`` records its ``difficulty`` but not its seed, and a manifest that
    cannot say which seed drew an item cannot regenerate it. The pairing lives
    here rather than on ``Task`` because it is the calibration pass that needs
    it: a hand-authored task has no seed at all.
    """

    pack: str
    seed: int
    difficulty: int
    track: str
    task: Task

    @property
    def key(self) -> str:
        return self.task.key


@dataclass(frozen=True)
class Outcome:
    """What the pass observed for one item."""

    correct: int
    trials: int

    @property
    def rate(self) -> float:
        """Accuracy over the trials that ran.

        Raises on an unmeasured item rather than returning 0.0. A zero would read
        as *always wrong*, which on the `at-scale` floor band is a **keep** — so
        the quiet failure mode of a cell that never ran is an item selected on no
        evidence at all.
        """
        if not self.trials:
            raise ValueError("no trials ran for this item — it has no rate")
        return self.correct / self.trials


@dataclass(frozen=True)
class Judged:
    """One candidate, the outcome it earned, and why it was kept or dropped."""

    candidate: Candidate
    outcome: Outcome | None
    kept: bool
    reason: str


@dataclass(frozen=True)
class Selection:
    judged: list[Judged]

    @property
    def kept(self) -> list[Judged]:
        return [j for j in self.judged if j.kept]

    @property
    def rejected(self) -> list[Judged]:
        return [j for j in self.judged if not j.kept]

    @property
    def tasks(self) -> list[Task]:
        return [j.candidate.task for j in self.kept]


def pool(
    seeds: list[int],
    *,
    packs: list[str] | None = None,
    difficulties: list[int] | None = None,
    tracks: list[str] | None = None,
) -> list[Candidate]:
    """Every ``(pack, seed, difficulty, track)`` combination, validated.

    A degenerate item **raises** rather than being filtered out. That is the rule
    `generate.validate` was written under — a generator that silently drops a
    third of its items is a generator whose difficulty setting no longer means
    what it says — and it costs nothing to keep here, because building the pool
    is offline and happens before a single cell runs. An item that trips it is a
    generator defect to fix, not a pool to shrink.
    """
    names = packs if packs is not None else domains.generators()
    unknown = [name for name in names if name not in domains.generators()]
    if unknown:
        reasons = ", ".join(f"{n} ({domains.NO_GENERATOR.get(n, 'no generator')})" for n in unknown)
        raise PoolError(f"cannot draw a pool from: {reasons}")

    candidates = []
    for name in names:
        for seed in seeds:
            for difficulty in difficulties or [3]:
                for track in tracks or ["in-context"]:
                    for task in domains.generate(name, seed, difficulty, track):
                        domains.validate(name, task)
                        candidates.append(Candidate(name, seed, difficulty, track, task))
    _refuse_collisions(candidates)
    return candidates


def _refuse_collisions(candidates: list[Candidate]) -> None:
    """Two candidates may not share a task key.

    A generated id carries the seed as its **low four hex digits**
    (``g{seed:x}[-4:]-d{difficulty}-…``), so two seeds agreeing in those four
    nibbles produce the same key — which produces the same `Cell.id`, which
    merges two different items' trials into one tally and selects an item on
    another item's evidence. Cheap to check, and invisible if it is not.
    """
    seen: dict[str, Candidate] = {}
    for candidate in candidates:
        clash = seen.get(candidate.key)
        if clash:
            raise PoolError(
                f"two pool items share the key {candidate.key}: seed {clash.seed} "
                f"(0x{clash.seed:x}) and seed {candidate.seed} (0x{candidate.seed:x}) "
                "agree in their low four hex digits, which is all an id records. "
                "Change one seed."
            )
        seen[candidate.key] = candidate


def tally(run_dir: Path, arm: str = ARM) -> dict[str, Outcome]:
    """Correct-count and trial-count per task key, from a pass's records.

    Two readings are deliberate. Only the **last** record per cell counts
    (`report.latest`), so a resumed pass counts each trial once rather than
    weighting the cell that was interrupted. And a cell `resume.failed`
    identifies is **not a trial**: an API error is a cell that did not run, and
    counting it as a miss would make an item look hard because the instrument
    broke.
    """
    outcomes: dict[str, tuple[int, int]] = {}
    for record in report.latest(RecordStore.load(run_dir)):
        if record["arm"] != arm or resume.failed(record):
            continue
        key = f"{record['domain']}/{record['task_id']}"
        correct, trials = outcomes.get(key, (0, 0))
        outcomes[key] = (correct + (record["verdict"] == "correct"), trials + 1)
    return {key: Outcome(correct, trials) for key, (correct, trials) in outcomes.items()}


def select(candidates: list[Candidate], outcomes: dict[str, Outcome]) -> Selection:
    """Keep the items that can carry a difference; say why the rest cannot.

    Every candidate comes back judged, kept or not, with the rate that decided
    it. A pass that keeps nothing has to be *readable* — that is the finding, and
    the shape of the rejections is what says whether the pool was too easy, too
    hard, or never measured.
    """
    judged = []
    for candidate in candidates:
        outcome = outcomes.get(candidate.key)
        if outcome is None or not outcome.trials:
            judged.append(Judged(candidate, None, False, "never measured"))
            continue
        kept, reason = _verdict(candidate.track, outcome.rate)
        judged.append(Judged(candidate, outcome, kept, reason))
    return Selection(judged)


def _verdict(track: str, rate: float) -> tuple[bool, str]:
    if track == "at-scale":
        if rate <= AT_SCALE_CEILING + TOLERANCE:
            return True, f"at-scale floor: prose {rate:.0%}"
        return False, (
            f"prose scored {rate:.0%} — an at-scale fixture the prose arm can "
            "answer did not defeat the arm it was built to defeat"
        )
    low, high = BAND
    if rate < low - TOLERANCE:
        return False, f"too hard: prose {rate:.0%}"
    if rate > high + TOLERANCE:
        return False, f"too easy: prose {rate:.0%}"
    return True, f"in band: prose {rate:.0%}"


def spec(
    seeds: list[int],
    packs: list[str],
    difficulties: list[int],
    tracks: list[str],
    trials: int,
    strength: Strength = STRENGTH,
) -> dict:
    """The pool as data, so a stopped pass can be rebuilt exactly.

    Written into a calibration run's ``run.json``, and carried verbatim into the
    manifest. `resume` rebuilds a grid from what the run recorded rather than
    from the flags given now — a resume that re-drew a different pool would join
    two different passes.

    ``strength`` is **the subject that did the selecting**, and it is recorded
    rather than assumed because an item's difficulty is not a property of the
    item alone (`hypotheses.md`, precondition 4). A slate calibrated on haiku is
    not calibrated for a 14B served locally, and a manifest that cannot say
    which one selected it cannot be checked against the grid that runs it.
    """
    return {
        "packs": list(packs),
        "seeds": list(seeds),
        "difficulties": list(difficulties),
        "tracks": list(tracks),
        "trials": trials,
        "arm": ARM,
        "strength": strength.name,
        "model": strength.model,
        "local": (
            {
                "endpoint": strength.endpoint,
                "protocol": strength.tool_protocol,
                "reasoning_effort": strength.reasoning_effort,
                "context_tokens": strength.context_tokens,
            }
            if strength.is_local
            else None
        ),
    }


def pass_spec(run_dir: Path) -> dict:
    """The pool spec a calibration pass recorded, from its own ``run.json``."""
    path = run_dir / "run.json"
    if not path.exists():
        raise ManifestError(f"no run.json in {run_dir}")
    meta = json.loads(path.read_text(encoding="utf-8"))
    recorded = meta.get("calibration")
    if not recorded:
        raise ManifestError(f"{run_dir.name} recorded no pool, so it is not a calibration pass")
    return recorded


def from_spec(recorded: dict) -> list[Candidate]:
    """Rebuild a pass's pool from what the pass itself recorded.

    Not from the flags given now, for the reason `cmd_resume` already rebuilds a
    grid this way: a re-drawn pool would join two different passes under one run
    id, and the half that ran first would be selecting against the other half's
    items.
    """
    return pool(
        recorded["seeds"],
        packs=recorded["packs"],
        difficulties=recorded["difficulties"],
        tracks=recorded["tracks"],
    )


def manifest(selection: Selection, pool_spec: dict, run_id: str | None = None) -> dict:
    """The selected slate, and enough of the pass to read it later.

    The rejected items are recorded too, with their rates. A slate that says only
    what it kept cannot answer the question the next session asks of it — *was
    the pool too easy, or did the pass not run?* — and that question is the whole
    reason this step exists.
    """
    return {
        "version": MANIFEST_VERSION,
        "written_at": datetime.now(UTC).isoformat(),
        "run_id": run_id,
        "pool": pool_spec,
        "bands": {"in-context": list(BAND), "at-scale": AT_SCALE_CEILING},
        "kept": [_entry(j) for j in selection.kept],
        "rejected": [_entry(j) for j in selection.rejected],
    }


def _entry(judged: Judged) -> dict:
    task = judged.candidate.task
    return {
        "domain": judged.candidate.pack,
        "id": task.id,
        "seed": judged.candidate.seed,
        "difficulty": judged.candidate.difficulty,
        "track": judged.candidate.track,
        "fingerprint": resume.fingerprint(task),
        "question_class": task.question_class,
        "truth_size": len(task.truth.rows),
        "engine_expected_to_help": task.engine_expected_to_help,
        "trials": judged.outcome.trials if judged.outcome else 0,
        "correct": judged.outcome.correct if judged.outcome else 0,
        "reason": judged.reason,
    }


def write(path: Path, payload: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return path


def read(path: Path) -> dict:
    if not path.exists():
        raise ManifestError(f"no manifest at {path}")
    payload = json.loads(path.read_text(encoding="utf-8"))
    version = payload.get("version")
    if version != MANIFEST_VERSION:
        raise ManifestError(
            f"{path} is manifest version {version!r}, this harness writes "
            f"{MANIFEST_VERSION} — the fields do not necessarily mean the same thing"
        )
    return payload


def digest(path: Path) -> str:
    """A hash of the manifest bytes, for a run to record which slate it ran."""
    return hashlib.sha256(path.read_bytes()).hexdigest()[:16]


def load(path: Path) -> list[Task]:
    """The slate a manifest names, regenerated and checked against it.

    **Regenerated, not stored.** A manifest holds provenance and a fingerprint,
    not fixtures: a slate is reproducible because the generator is deterministic,
    and the fingerprint is what turns that from a hope into a check. If a
    generator changes — a threshold, a tie repair, a question's wording — the
    items no longer hash to what the pass measured, and this refuses rather than
    running a grid against a slate that quietly moved. That is
    `resume.fingerprint`'s 2026-08-24 lesson pointed at the source tree.
    """
    payload = read(path)
    entries = payload.get("kept", [])
    tasks: list[Task] = []
    drawn: dict[tuple, dict[str, Task]] = {}
    for entry in entries:
        origin = (entry["domain"], entry["seed"], entry["difficulty"], entry["track"])
        if origin not in drawn:
            generated = domains.generate(
                entry["domain"], entry["seed"], entry["difficulty"], entry["track"]
            )
            drawn[origin] = {task.id: task for task in generated}
        task = drawn[origin].get(entry["id"])
        if task is None:
            raise ManifestError(
                f"{entry['domain']}/{entry['id']} is in {path.name} but seed "
                f"{entry['seed']} at difficulty {entry['difficulty']} no longer "
                "produces an item with that id"
            )
        now = resume.fingerprint(task)
        if now != entry["fingerprint"]:
            raise ManifestError(
                f"{task.key} hashes to {now}, the manifest recorded "
                f"{entry['fingerprint']} — the fixture, question or truth moved "
                "under this slate, so the grid would not measure what was "
                "calibrated. Re-calibrate."
            )
        tasks.append(task)
    if not tasks:
        raise ManifestError(f"{path} selected no items — there is no slate to run")
    return tasks
