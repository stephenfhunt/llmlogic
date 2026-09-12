"""``harness`` — run the grid, render a run.

``--dry-run`` is the offline gate: the full grid against the stub subject, no API
calls, no cost. It is what CI runs, and what a change has to keep passing.
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path

from harness import (
    ablate,
    arms,
    calibrate,
    corpus,
    domains,
    generate,
    local,
    reference,
    report,
    resume,
)
from harness.agent import (
    DEFAULT_MAX_BUDGET_USD,
    DEFAULT_MAX_TURNS,
    AgentSubject,
)
from harness.cell import ARMS, STRENGTHS, grid
from harness.record import RecordStore
from harness.runner import new_run_id, run_grid
from harness.score import DEFAULT_TIMEOUT, score
from harness.subject import StubSubject

#: Packs a calibration pool does not draw from unless asked by name. `controls`
#: is the negative control: the band would judge it *too easy* — which is what a
#: healthy control is — and a control selected for difficulty has stopped being
#: one. `_calibrated_slate` carries the pinned four into every calibrated grid
#: instead, so precondition 1 stays checkable without the pass paying for them.
CALIBRATION_EXCLUDES = frozenset({"controls"})

RESULTS_ROOT = Path(__file__).resolve().parents[2] / "results"
SLATES_ROOT = Path(__file__).resolve().parents[2] / "slates"
WORKSPACE_ROOT = arms.WORKSPACE_ROOT


def _progress(index: int, total: int, record) -> None:
    print(f"[{index:>3}/{total}] {record.cell_id:<52} {record.verdict}", file=sys.stderr)


def cmd_run(args: argparse.Namespace) -> int:
    if args.resume:
        return cmd_resume(args)
    names = args.domain or None
    if args.slate:
        # A calibrated slate is named by a manifest, not by a pack list: which
        # items it holds was decided by the calibration pass, and re-deriving it
        # from flags here is how the two would drift apart.
        try:
            tasks = _calibrated_slate(Path(args.slate))
        except calibrate.ManifestError as exc:
            print(str(exc), file=sys.stderr)
            return 1
    else:
        tasks = domains.load_all(names)
    if not tasks:
        print("no domain packs available yet", file=sys.stderr)
        return 1
    if args.smoke:
        tasks = tasks[:1]

    if args.local_model:
        prepared = _local_sitting(args)
        if prepared is None:
            return 1
        strengths, local_meta = prepared
    else:
        local_meta = None
        strengths = STRENGTHS
        if args.strength:
            wanted = set(args.strength)
            strengths = tuple(s for s in STRENGTHS if s.name in wanted)
            if not strengths:
                known = ", ".join(s.name for s in STRENGTHS)
                print(f"no such strength: {sorted(wanted)} — have {known}", file=sys.stderr)
                return 1

    # The subject is checked *after* the strengths are built, because until then
    # there is nothing to compare the manifest against — and before a single cell
    # runs, because that is the whole point of checking it here.
    if args.slate and not _slate_subject_holds(Path(args.slate), strengths, args):
        return 1

    return _run_grid(args, tasks, strengths, local_meta)


def _slate_subject_holds(path: Path, strengths: tuple, args: argparse.Namespace) -> bool:
    """Refuse a grid that does not hold the subject its slate was calibrated for.

    **Refuse, do not degrade** (`decisions.md` 2026-08-26): the alternative is a
    grid that runs, reports, and measures a different subject than the band
    selected for — precondition 4 failing with nothing said, which this project
    has now recorded five times and never once caught while it was happening.

    Only `run --slate` needs this. A **resume** does not: `_slate_of` rebuilds
    both the slate and the subject from the run's own `run.json`, so the second
    half of a sitting cannot drift from the first by construction — the check
    would have nothing to disagree with.
    """
    cap = args.max_output_tokens if args.local_model else None
    try:
        moved = calibrate.subject_moved(calibrate.read(path), strengths, cap)
    except calibrate.ManifestError as exc:  # already reported by `_calibrated_slate`
        print(str(exc), file=sys.stderr)
        return False
    if moved:
        print(moved, file=sys.stderr)
        return False
    return True


def _calibrated_slate(path: Path) -> list:
    """The items a manifest kept, **plus the pinned negative controls**.

    The band selects for headroom, and a healthy control has none: it is a
    single-hop lookup the subject should get right every time, so calibration
    rejects it as *too easy* — correctly. Left at that, a calibrated grid holds
    no `controls` cells at all, and `hypotheses.md` precondition 1 becomes
    uncheckable on the one run it exists to gate.

    So the controls are not selected, they are **carried**: the pinned four, the
    same four every other run in `results/` measured, never the calibrated pool's
    (`cmd_calibrate` draws no controls for the same reason). One home for the
    rule, because `_slate_of` has to rebuild exactly this list for a resume.
    """
    return calibrate.load(path) + domains.load("controls")


def _local_sitting(args: argparse.Namespace) -> tuple[tuple, dict] | None:
    """Preflight the server, and build the strengths a local sitting crosses.

    Shared by `run` and `calibrate`, because a calibration pass against a local
    subject is a local sitting in every respect the instrument cares about: the
    same server, the same window, the same wall-clock currency. Two copies of
    this would be two places for the window to drift from what `preflight` held
    the server to.

    Returns ``None`` when the server cannot carry the sitting, having said why —
    a local run is not billed and not rate-limited, so the failure it has to
    refuse is a *misconfiguration*, and refusing it here is the only place that
    happens before cells start.
    """
    if not args.dry_run:
        problems = local.preflight(args.endpoint, args.local_model, args.min_context)
        if problems:
            for problem in problems:
                print(problem, file=sys.stderr)
            return None
    # The window the strengths claim is the window `preflight` just held the
    # server to. They cannot be allowed to disagree: the overflow guard measures
    # against the strength, and the truncation happens at the server.
    window = args.min_context or local.DEFAULT_CONTEXT_TOKENS
    protocols = args.protocol or ["native"]
    strengths = local.strengths(
        args.local_model, protocols, args.endpoint, args.reasoning_effort, window
    )
    meta = {
        "endpoint": args.endpoint,
        "models": list(args.local_model),
        "protocols": list(protocols),
        "reasoning_effort": args.reasoning_effort,
        "max_cell_seconds": args.max_cell_seconds,
        # Recorded because `resume` has to rebuild the strengths this run was
        # measured under, and the window is the one field of a local strength
        # that nothing else in the record implies. A resume that guessed it
        # would run the second half of a sitting under a different overflow
        # guard than the first.
        "context_tokens": window,
        # Same reason, and it bites harder with thinking on: the cap bounds
        # *reasoning plus answer*, so a resume that guessed it would truncate
        # the second half's thoughts where the first half's completed.
        "max_output_tokens": args.max_output_tokens,
    }
    return strengths, meta


def _run_grid(
    args: argparse.Namespace, tasks: list, strengths: tuple, local_meta: dict | None
) -> int:
    """Cross, confirm, record, run — shared by the billed and the local paths.

    The only differences a local run makes are where the confirmation comes from
    (wall clock, not money) and which subject answers the cells. Everything that
    carries the run's validity — the crossing, the fingerprints, the metadata a
    resume rebuilds from — is the same code, because a local run is a run.
    """
    # An ablation is engine-arm only: cutting a block of the engine's own
    # documentation cannot move an arm that never had it, so a prose cell here
    # would be paying to re-measure the control.
    cell_arms = ("engine",) if args.ablate else ARMS
    if args.arm and not args.ablate:
        wanted = set(args.arm)
        cell_arms = tuple(arm for arm in ARMS if arm in wanted)
        if not cell_arms:
            print(f"no such arm: {sorted(wanted)} — have {', '.join(ARMS)}", file=sys.stderr)
            return 1
    if args.ablate:
        available = ablate.catalogue(arms.DATALOG_SKILL_DIR)
        if args.ablate not in available:
            print(
                f"no block named {args.ablate!r} — have "
                f"{', '.join(sorted(available)) or '(none marked)'}",
                file=sys.stderr,
            )
            return 1

    cells = grid(tasks, strengths, cell_arms, args.ablate, args.repeats)
    if local_meta:
        # Group a local sitting by model. `grid` varies strength fastest, which
        # is right when a strength is an API parameter and wrong when it is five
        # gigabytes of weights: measured, a model switch costs ~4s against ~0.2s
        # warm, so the default order spends about half an hour of a 432-cell
        # sweep doing nothing but moving models in and out of VRAM. A stable sort
        # keeps trials outermost — a truncated sitting still holds whole passes,
        # now of one model at a time — and leaves every cell id untouched, so
        # `resume` is unaffected.
        cells.sort(key=lambda cell: (cell.trial, cell.strength.name))

    # **The grid and the sitting are two numbers, and only one of them is the
    # experiment.** `--limit` sizes a sitting to the window and `--resume`
    # finishes what it left — that pair is the documented shape of a run
    # (`AGENTS.md`). Recording the *limited* count as `cells` made the two
    # mutually exclusive: a resume rebuilds the whole grid, compared it against
    # the sitting, and refused its own run as a slate that had moved. Found
    # 2026-08-30, on the first sitting that was ever staged this way.
    planned = cells
    if args.limit:
        cells = cells[: args.limit]

    # A run says what it could cost and refuses to start without being told to.
    # The currency differs: an Anthropic run spends someone's account, a local one
    # spends the night. The store is built after this, not before — a refused run
    # should leave no trace in `results/`, which records runs that happened.
    if not args.dry_run:
        if local_meta:
            hours = len(cells) * args.max_cell_seconds / 3600
            print(
                f"{len(cells)} cells at up to {args.max_cell_seconds}s each — "
                f"worst case {hours:.1f}h. No API cost.",
                file=sys.stderr,
            )
        else:
            ceiling = len(cells) * args.budget
            print(
                f"{len(cells)} cells at up to {args.budget:.2f} USD each — "
                f"ceiling {ceiling:.2f} USD.",
                file=sys.stderr,
            )
        if len(cells) != len(planned):
            print(
                f"This sitting is {len(cells)} of a {len(planned)}-cell grid; "
                "`harness run --resume` finishes the rest.",
                file=sys.stderr,
            )
        if not args.yes:
            print("re-run with --yes to start it.", file=sys.stderr)
            return 2

    store = RecordStore(RESULTS_ROOT, new_run_id("dry" if args.dry_run else "run"))
    store.write_metadata(
        dry_run=args.dry_run,
        domains=sorted({task.domain for task in tasks}),
        tasks=len(tasks),
        # The **grid**, not this sitting: `resume` rebuilds the whole thing and
        # checks it against this number, so a limited sitting that recorded its
        # own size would be refusing itself.
        cells=len(planned),
        limit=args.limit,
        strengths=[strength.name for strength in strengths],
        arms=list(cell_arms),
        repeats=args.repeats,
        fingerprints=resume.fingerprints(tasks),
        ablate=args.ablate,
        max_turns=args.max_turns,
        max_budget_usd=args.budget,
        local=local_meta,
        slate=(
            {"path": str(Path(args.slate).resolve()), "digest": calibrate.digest(Path(args.slate))}
            if args.slate
            else None
        ),
    )

    subject = _subject(args, bool(local_meta))
    result = run_grid(cells, subject, store, WORKSPACE_ROOT, on_cell=_progress)
    path = report.write(store.dir)
    simulated = " (simulated)" if args.dry_run else ""
    print(f"\n{len(result.records)} cells · {result.cost_usd:.2f} USD{simulated}")
    print(f"report: {path}")
    return _halted(result, store.dir, len(cells))


def _subject(args: argparse.Namespace, local_run: bool):
    """Which subject answers the cells.

    Three now, and the choice is the run's, not the cell's: a grid mixing an
    Anthropic strength with a local one would be comparing two models *and* two
    drivers, and nothing in the record would say which difference moved the
    number. `LocalSubject` reads its model and protocol off each cell's strength,
    so one instance still covers a whole sweep.
    """
    if args.dry_run:
        return StubSubject()
    if local_run:
        return local.LocalSubject(
            base_url=args.endpoint,
            max_turns=args.max_turns,
            max_cell_seconds=args.max_cell_seconds,
            max_output_tokens=args.max_output_tokens,
        )
    return AgentSubject(max_turns=args.max_turns, max_budget_usd=args.budget)


def cmd_power(args: argparse.Namespace) -> int:
    """How many paired items a grid needs before it is worth paying for.

    The most expensive kind of null is a run that could not have detected the
    effect it went looking for. The 2026-08-24 grid was one, and nothing said so
    until afterwards.
    """
    from harness.stats import required_items

    print(f"baseline {args.baseline:.0%} · effect {args.effect:+.0%} · power {args.power:.0%}\n")
    needed = required_items(args.baseline, args.effect, power=args.power)
    per_arm = len(STRENGTHS)
    print(f"  paired items needed: {needed}")
    print(f"  = {needed // per_arm} tasks across {per_arm} strengths, or {needed} at one")
    print(f"  = {needed * len(ARMS)} cells at {len(ARMS)} arms\n")

    slate = len(domains.load_all()) * per_arm
    verdict = "enough" if slate >= needed else f"SHORT by {needed - slate}"
    print(f"The slate today is {slate} paired items — {verdict}.")
    return 0


def cmd_score(args: argparse.Namespace) -> int:
    """Run one program against one task's fixture and grade what it derived."""
    domain, _, task_id = args.task.partition("/")
    matches = [t for t in domains.load_all([domain]) if t.id == task_id]
    if not matches:
        known = ", ".join(sorted(t.key for t in domains.load_all([domain]))) or "(none)"
        print(f"no such task: {args.task} — have {known}", file=sys.stderr)
        return 1
    task = matches[0]

    program = Path(args.program).read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory() as tmp:
        result = score(task, program, Path(tmp), relation=args.relation, timeout=args.timeout)

    if not result.accepted:
        print(f"rejected (exit {result.exit_code})", file=sys.stderr)
        print(result.stderr.rstrip(), file=sys.stderr)
        return 2
    if result.derived is None:
        print(result.stderr.rstrip(), file=sys.stderr)
        return 2
    print(
        f"{'correct' if result.correct else 'wrong'} · "
        f"{len(result.derived)} derived, {len(task.truth.rows)} expected · "
        f"missing {result.missing}, extra {result.extra}"
    )
    return 0 if result.correct else 1


def cmd_calibrate(args: argparse.Namespace) -> int:
    """Select the slate a grid will run, instead of designing it.

    One arm at one strength over a generated pool. It is the cheapest possible
    check for the ceiling that made the 2026-08-24 grid unreadable — and the only
    one that can find it *before* the money is spent rather than after.

    The selection is deliberately the second half of this command and not a
    separate tool: a pass whose items were never chosen is a pass nobody can act
    on. `--from` runs that half alone, against a pass already paid for.
    """
    if args.from_run:
        return _reselect(Path(args.from_run), args)

    packs = args.domain or [p for p in domains.generators() if p not in CALIBRATION_EXCLUDES]
    seeds = args.seed or list(calibrate.DEFAULT_SEEDS)
    difficulties = args.difficulty or list(calibrate.DEFAULT_DIFFICULTIES)
    tracks = args.track or ["in-context"]
    try:
        candidates = calibrate.pool(seeds, packs=packs, difficulties=difficulties, tracks=tracks)
    except (calibrate.PoolError, generate.Degenerate) as exc:
        print(str(exc), file=sys.stderr)
        return 1

    local_meta = None
    if args.local_model:
        prepared = _local_sitting(args)
        if prepared is None:
            return 1
        strengths, local_meta = prepared
        # A pass is *one* arm at *one* strength, and `calibrate.tally` counts by
        # task key across every record in the run. Two strengths would put six
        # trials of one item under one rate, mixing two subjects into a band
        # that means something only for one — and precondition 4 exists to stop
        # exactly that. So the crossing `run` sweeps is refused here by name.
        if len(strengths) != 1:
            print(
                f"a calibration pass runs at one strength, and this is "
                f"{len(strengths)}: {', '.join(s.name for s in strengths)}. A "
                "slate is calibrated for one subject — run a pass per subject.",
                file=sys.stderr,
            )
            return 1
        strength = strengths[0]
    else:
        strength = next((s for s in STRENGTHS if s.name == args.strength), None)
        if strength is None:
            known = ", ".join(s.name for s in STRENGTHS)
            print(f"no such strength: {args.strength} — have {known}", file=sys.stderr)
            return 1

    tasks = [candidate.task for candidate in candidates]
    cells = grid(tasks, (strength,), (calibrate.ARM,), None, args.trials)
    planned = cells  # the pass; `cells` below is this sitting of it. See `_run_grid`.
    if args.limit:
        cells = cells[: args.limit]

    print(
        f"{len(candidates)} items from {len(packs)} pack(s) × {len(seeds)} seed(s) × "
        f"{len(difficulties)} difficult(ies) × {len(tracks)} track(s) — "
        f"{len(cells)} cells at {args.trials} trial(s), {calibrate.ARM} / {strength.name}.",
        file=sys.stderr,
    )
    if not args.dry_run:
        if local_meta:
            hours = len(cells) * args.max_cell_seconds / 3600
            print(
                f"up to {args.max_cell_seconds}s each — worst case {hours:.1f}h. No API cost.",
                file=sys.stderr,
            )
        else:
            ceiling = len(cells) * args.budget
            print(
                f"up to {args.budget:.2f} USD each — ceiling {ceiling:.2f} USD.",
                file=sys.stderr,
            )
        if not args.yes:
            print("re-run with --yes to spend it.", file=sys.stderr)
            return 2

    store = RecordStore(RESULTS_ROOT, new_run_id("cal-dry" if args.dry_run else "cal"))
    store.write_metadata(
        dry_run=args.dry_run,
        domains=sorted({task.domain for task in tasks}),
        tasks=len(tasks),
        cells=len(planned),
        limit=args.limit,
        strengths=[strength.name],
        arms=[calibrate.ARM],
        repeats=args.trials,
        fingerprints=resume.fingerprints(tasks),
        ablate=None,
        max_turns=args.max_turns,
        max_budget_usd=args.budget,
        local=local_meta,
        calibration=calibrate.spec(
            seeds,
            packs,
            difficulties,
            tracks,
            args.trials,
            strength,
            args.max_output_tokens if local_meta else None,
        ),
    )

    subject = _subject(args, bool(local_meta))
    result = run_grid(cells, subject, store, WORKSPACE_ROOT, on_cell=_progress)
    report.write(store.dir)
    simulated = " (simulated)" if args.dry_run else ""
    print(f"\n{len(result.records)} cells · {result.cost_usd:.2f} USD{simulated}")

    # A halted pass measured some items and not others, and an unmeasured item
    # is not a hard one. Selecting now would pin a slate whose shape is the shape
    # of where the session window closed.
    if result.halted:
        print(
            "\nNo slate written: the pass stopped before every item was measured, "
            "and an item that never ran is not a hard item.",
            file=sys.stderr,
        )
        return _halted(result, store.dir, len(cells))

    return _write_slate(candidates, store.dir, args)


def cmd_ladder(args: argparse.Namespace) -> int:
    """Write a slate that walks the difficulty axis, screened by nothing.

    `calibrate` answers *which items have headroom for this subject*, and it
    answers it by keeping the middle of a band. A ladder asks the opposite
    question — **where along difficulty does the engine start to pay?** — and the
    band is what would destroy it: which items land in the middle is itself a
    fact about the rung, so a screened ladder measures the screen.

    So there is no pass and no selection here. This is offline: it draws the
    pool, keeps the named questions at every rung, and pins them to a manifest
    that says `selection: none` so nobody later reads its zeros as a subject
    scoring nothing.
    """
    packs = args.domain or [p for p in domains.generators() if p not in CALIBRATION_EXCLUDES]
    seeds = [args.seed]
    difficulties = args.difficulty or list(calibrate.DEFAULT_DIFFICULTIES)
    tracks = args.track or ["in-context"]

    strength = next((s for s in STRENGTHS if s.name == args.strength), None)
    if strength is None:
        known = ", ".join(s.name for s in STRENGTHS)
        print(f"no such strength: {args.strength} — have {known}", file=sys.stderr)
        return 1

    try:
        candidates = calibrate.pool(seeds, packs=packs, difficulties=difficulties, tracks=tracks)
    except (calibrate.PoolError, generate.Degenerate) as exc:
        print(str(exc), file=sys.stderr)
        return 1

    if args.question:
        wanted = set(args.question)
        candidates = [c for c in candidates if calibrate.question(c) in wanted]
        # **Refuse a name that matched nothing**, rather than writing the ladder
        # it did match. A typo'd question silently drops one trajectory out of
        # eight, and a ladder missing a rung of one line reads as a ladder whose
        # subject failed there.
        found = {calibrate.question(c) for c in candidates}
        if missing := sorted(wanted - found):
            print(
                f"no item named {', '.join(missing)} in {', '.join(packs)} at "
                f"difficult{'ies' if len(difficulties) > 1 else 'y'} "
                f"{', '.join(str(d) for d in difficulties)}",
                file=sys.stderr,
            )
            return 1

    if not candidates:
        print("the ladder is empty — nothing to write", file=sys.stderr)
        return 1

    payload = calibrate.ladder(
        candidates,
        calibrate.spec(seeds, packs, difficulties, tracks, trials=0, strength=strength, arm=None),
    )

    out = Path(args.out) if args.out else SLATES_ROOT / f"ladder-{args.seed}.json"
    if not out.is_absolute():
        out = Path.cwd() / out
    calibrate.write(out, payload)

    rungs = sorted({c.difficulty for c in candidates})
    per_rung = len(candidates) // len(rungs)
    print(
        f"{len(candidates)} items — {per_rung} per rung across d"
        f"{', d'.join(str(d) for d in rungs)}, seed {args.seed}, for "
        f"{strength.name}. Nothing was screened.",
    )
    print(f"slate: {out}")
    return 0


def _reselect(run_dir: Path, args: argparse.Namespace) -> int:
    """Select from a pass already run, without running anything.

    The band is a reading of the evidence, not part of collecting it. Moving it
    must not cost another pass — and a re-selection that had to re-run the
    subject would quietly be a *different* pass, since the subject is stochastic.
    """
    if not run_dir.is_absolute():
        run_dir = Path.cwd() / run_dir
    try:
        recorded = calibrate.pass_spec(run_dir)
        candidates = calibrate.from_spec(recorded)
    except (calibrate.ManifestError, calibrate.PoolError, generate.Degenerate) as exc:
        print(str(exc), file=sys.stderr)
        return 1
    return _write_slate(candidates, run_dir, args)


def _write_slate(candidates: list, run_dir: Path, args: argparse.Namespace) -> int:
    """Tally, select, report what the band did, and pin the manifest.

    Whether this was a stub's pass is read from the run directory rather than
    passed in, because `--from` selects out of a pass it did not run and the run
    is the only thing that knows.
    """
    outcomes = calibrate.tally(run_dir)
    selection = calibrate.select(candidates, outcomes)
    recorded = calibrate.pass_spec(run_dir)
    dry_run = bool(json.loads((run_dir / "run.json").read_text()).get("dry_run"))

    out = Path(args.out) if args.out else _slate_path(run_dir, dry_run)
    if not out.is_absolute():
        out = Path.cwd() / out

    for line in _selection_lines(selection):
        print(line)

    if not selection.kept:
        print(
            "\nNothing landed in the band. That is the finding — read the rejection "
            "rates above before widening the pool.",
            file=sys.stderr,
        )
        return 1

    calibrate.write(out, calibrate.manifest(selection, recorded, run_dir.name))
    print(f"\nslate: {out}")
    if dry_run:
        print(
            "This slate came from the stub subject. It is evidence about the "
            "plumbing and about nothing else.",
            file=sys.stderr,
        )
    return 0


def _slate_path(run_dir: Path, dry_run: bool) -> Path:
    """Where a manifest lands when nobody said.

    A paid pass writes into `slates/`, which is tracked: a pinned slate is part
    of the experiment's definition, not an output. A dry run writes inside its own
    run directory, because a stub's selection is evidence about the plumbing and
    committing it would put it where the real ones live.
    """
    return run_dir / "slate.json" if dry_run else SLATES_ROOT / f"{run_dir.name}.json"


def _selection_lines(selection) -> list[str]:
    """What the band did, by track and by pack.

    A pass that keeps nothing has to be as readable as one that keeps everything:
    *too easy* means the pool needs harder settings, *too hard* means the opposite,
    and *never measured* means the pass did not finish and neither reading applies.
    """
    lines = ["", f"{len(selection.kept)} kept of {len(selection.judged)}", ""]
    by_track: dict[str, list] = {}
    for judged in selection.judged:
        by_track.setdefault(judged.candidate.track, []).append(judged)
    for track, judged in sorted(by_track.items()):
        kept = [j for j in judged if j.kept]
        lines.append(f"  {track:<12} {len(kept):>3} kept / {len(judged):>3}")
        buckets: dict[str, int] = {}
        for one in judged:
            if not one.kept:
                buckets[one.reason.split(":")[0]] = buckets.get(one.reason.split(":")[0], 0) + 1
        for reason, count in sorted(buckets.items(), key=lambda pair: -pair[1]):
            lines.append(f"      {count:>3}  {reason}")
    lines.append("")
    for pack in sorted({j.candidate.pack for j in selection.judged}):
        judged = [j for j in selection.judged if j.candidate.pack == pack]
        kept = [j for j in judged if j.kept]
        rates = sorted(j.outcome.rate for j in judged if j.outcome)
        median = f"{rates[len(rates) // 2]:.0%}" if rates else "—"
        lines.append(f"  {pack:<18} {len(kept):>3} / {len(judged):<3}  median prose {median}")
    return lines


def _halted(result, run_dir: Path, planned: int) -> int:
    """Say what stopped the grid and how to pick it up. Exit 3, not 0: a run that
    stopped early is not a run that finished, and a caller scripting a session
    should be able to tell without parsing the report."""
    if not result.halted:
        return 0
    print(
        f"\nHALTED after {len(result.records)} of {planned} cells — {result.halted}\n"
        f"The remaining cells were not run and not recorded. Resume with:\n"
        f"  harness run --resume {run_dir} --yes",
        file=sys.stderr,
    )
    return 3


def cmd_resume(args: argparse.Namespace) -> int:
    """Finish a run that stopped, appending into its own directory.

    The grid is rebuilt from the run's own ``run.json`` rather than from the
    flags given now — resuming with a different slate would silently make the two
    halves different experiments.
    """
    run_dir = Path(args.resume)
    if not run_dir.is_absolute():
        run_dir = Path.cwd() / run_dir
    try:
        meta = resume.metadata(run_dir)
    except resume.NotResumable as exc:
        print(str(exc), file=sys.stderr)
        return 1

    try:
        tasks = _slate_of(run_dir, meta)
    except (calibrate.ManifestError, calibrate.PoolError) as exc:
        print(str(exc), file=sys.stderr)
        return 1
    local_meta = meta.get("local")
    if local_meta:
        rebuilt = _local_strengths_of(local_meta, meta["strengths"])
        if rebuilt is None:
            return 1
        strengths = rebuilt
    else:
        strengths = tuple(s for s in STRENGTHS if s.name in set(meta["strengths"]))
    cell_arms = tuple(meta["arms"])
    # `repeats` is part of the grid's shape: rebuilding without it produces only
    # the first trial of each cell, and every later trial in the record then
    # reads as a stranger — a resume that refuses itself.
    cells = grid(tasks, strengths, cell_arms, meta.get("ablate"), meta.get("repeats", 1))
    if len(cells) != meta["cells"]:
        print(
            f"the grid rebuilt to {len(cells)} cells but the run recorded "
            f"{meta['cells']} — the slate has changed under it, so resuming would "
            "join two different experiments. Start a new run.",
            file=sys.stderr,
        )
        return 1

    shifted = resume.moved(tasks, meta)
    if shifted:
        key, (was, now) = next(iter(shifted.items()))
        print(
            f"{len(shifted)} task(s) have changed since this run started — e.g. "
            f"{key} ({was} → {now}). The fixture, question or truth moved under "
            "the same id, so the two halves would not be one experiment. "
            "Start a new run.",
            file=sys.stderr,
        )
        return 1

    unknown = resume.strangers(cells, run_dir)
    if unknown:
        print(
            f"{len(unknown)} recorded cell(s) are not in the rebuilt grid — e.g. "
            f"{sorted(unknown)[0]}. The slate has changed under this run, so "
            "resuming would join two different experiments. Start a new run.",
            file=sys.stderr,
        )
        return 1

    owed = resume.outstanding(cells, run_dir)
    if not owed:
        print(f"{run_dir.name} is complete — {len(cells)} cells, nothing outstanding.")
        print(f"report: {report.write(run_dir)}")
        return 0
    if args.limit:
        owed = owed[: args.limit]

    budget = meta.get("max_budget_usd", args.budget)
    max_turns = meta.get("max_turns", args.max_turns)
    if local_meta:
        seconds = local_meta.get("max_cell_seconds", local.DEFAULT_MAX_CELL_SECONDS)
        hours = len(owed) * seconds / 3600
        print(
            f"{len(owed)} of {len(cells)} cells outstanding in {run_dir.name} — "
            f"up to {seconds}s each, worst case {hours:.1f}h. No API cost.",
            file=sys.stderr,
        )
    else:
        print(
            f"{len(owed)} of {len(cells)} cells outstanding in {run_dir.name} — "
            f"ceiling {len(owed) * budget:.2f} USD.",
            file=sys.stderr,
        )
    if not args.yes:
        print("re-run with --yes to spend it.", file=sys.stderr)
        return 2

    store = RecordStore(RESULTS_ROOT, run_dir.name)
    if local_meta:
        subject = local.LocalSubject(
            base_url=local_meta["endpoint"],
            max_turns=max_turns,
            max_cell_seconds=local_meta.get("max_cell_seconds", local.DEFAULT_MAX_CELL_SECONDS),
            max_output_tokens=local_meta.get("max_output_tokens", local.DEFAULT_MAX_OUTPUT_TOKENS),
        )
    else:
        subject = AgentSubject(max_turns=max_turns, max_budget_usd=budget)
    result = run_grid(owed, subject, store, WORKSPACE_ROOT, on_cell=_progress)
    path = report.write(store.dir)
    print(f"\n{len(result.records)} cells resumed · {result.cost_usd:.2f} USD")
    print(f"report: {path}")
    return _halted(result, store.dir, len(owed))


def _local_strengths_of(local_meta: dict, recorded: list[str]) -> tuple | None:
    """The local strengths a stopped run was measured under, rebuilt from itself.

    Not from the flags given now, for the reason `_slate_of` rebuilds the slate
    that way: the two halves of a resumed run have to be one experiment. A local
    strength carries an endpoint, a protocol and a **window**, and the window is
    what the overflow guard measures a conversation against — so a resume that
    guessed it would bound the second half differently from the first.

    A run that recorded no window is refused rather than defaulted. That is the
    fourth instance of this instrument being able to report plausible numbers
    from a misconfiguration (`notes/a-local-subject.md`), and the ruling from the
    third stands: refuse, do not degrade.
    """
    window = local_meta.get("context_tokens")
    if not window:
        print(
            "this run recorded no served context window, so the strengths it "
            "measured under cannot be rebuilt — a resume would guess the window "
            "the overflow guard bounds a cell against. Start a new run.",
            file=sys.stderr,
        )
        return None

    problems = local.preflight(local_meta["endpoint"], local_meta["models"], window)
    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        return None

    strengths = local.strengths(
        local_meta["models"],
        local_meta["protocols"],
        local_meta["endpoint"],
        local_meta.get("reasoning_effort"),
        window,
    )
    wanted = set(recorded)
    rebuilt = tuple(s for s in strengths if s.name in wanted)
    if len(rebuilt) != len(wanted):
        print(
            f"the run recorded strengths {sorted(wanted)} but its local block "
            f"rebuilds {[s.name for s in strengths]} — the two halves would not "
            "be one experiment. Start a new run.",
            file=sys.stderr,
        )
        return None
    return rebuilt


def _slate_of(run_dir: Path, meta: dict) -> list:
    """The tasks a run was actually run against.

    Three provenances now, and a resume that guesses wrong joins two different
    experiments: a calibration pass rebuilds its pool from the spec it recorded,
    a calibrated run reloads its manifest — refusing one whose bytes moved — and
    everything else is the pinned slate.
    """
    if meta.get("calibration"):
        return [c.task for c in calibrate.from_spec(meta["calibration"])]
    if meta.get("slate"):
        path = Path(meta["slate"]["path"])
        if calibrate.digest(path) != meta["slate"]["digest"]:
            raise calibrate.ManifestError(
                f"{path.name} has changed since this run started — the slate it "
                "names is not the slate that was measured. Start a new run."
            )
        return _calibrated_slate(path)
    return domains.load_all(meta["domains"])


def cmd_report(args: argparse.Namespace) -> int:
    run_dir = Path(args.run_dir)
    if not run_dir.exists():
        print(f"no such run: {run_dir}", file=sys.stderr)
        return 1
    print(report.render(run_dir))
    return 0


def cmd_domains(_: argparse.Namespace) -> int:
    present = domains.available()
    blocked = domains.blocked()
    for name in domains.SLATE:
        mark = "✓" if name in present else " "
        count = len(domains.load(name)) if name in present else 0
        note = f"  — {blocked[name]}" if name in blocked else ""
        print(f" {mark} {name:<18} {count or '':>2} tasks{note}")
    # A pack can exist and not be slated — one being built, whose fixture is
    # ready and whose questions are not. Printing only `SLATE` would hide it,
    # and "a grid that is smaller than it looks" is the failure this command
    # exists to prevent, from the other direction.
    for name in domains.unslated():
        note = f"  — {blocked[name]}" if name in blocked else ""
        print(f" · {name:<18} {'':>2} not in the slate{note}")
    return 0


def cmd_corpus(args: argparse.Namespace) -> int:
    """Fetching needs the network, which is exactly why it is not part of a run:
    a cell has none, and a corpus downloaded mid-grid would be a fixture that
    changed under the experiment."""
    for item in corpus.CORPORA:
        if args.action == "fetch" and not item.present():
            print(f"fetching {item.slug} from {item.url}", file=sys.stderr)
            corpus.fetch(item)
        state = "present" if item.present() else "missing"
        print(f" {item.slug:<24} {state:<8} {item.root}")
    return 0


def cmd_blocks(_: argparse.Namespace) -> int:
    """The named blocks an ablation can cut, and how big each one is."""
    found = ablate.catalogue(arms.DATALOG_SKILL_DIR)
    if not found:
        print("no blocks marked in the skill", file=sys.stderr)
        return 1
    for name, block in found.items():
        print(f" {name:<32} {block.document:<28} {block.lines:>3} lines  {block.words:>4} words")
    return 0


def cmd_reference(args: argparse.Namespace) -> int:
    """Run the pinned corpus; with ``--repin``, adopt what it printed.

    Reading the diff is the whole job. A pin that moved is a change in the
    instrument, and the run before it and the run after it are no longer the same
    measurement — so `--repin` is a separate, deliberate act, never something a
    red test does on its own.
    """

    try:
        arms.require_engine()
    except arms.EngineMissing as exc:
        print(str(exc), file=sys.stderr)
        return 1

    entries = reference.correct() + reference.malformed()
    moved = 0
    for entry in entries:
        fixture = None
        if entry.domain:
            tasks = domains.load_all([entry.domain])
            if not tasks:
                print(f"{entry.name:24} skipped (no corpus)", file=sys.stderr)
                continue
            fixture = tasks[0].fixture
        with tempfile.TemporaryDirectory() as scratch:
            workdir = Path(scratch)
            reference.materialize(entry, fixture, workdir)
            result = reference.run(entry, workdir)
        drift = [
            name
            for name, got, want in (
                ("stdout", result.stdout, entry.stdout if entry.domain else ""),
                ("stderr", result.stderr, entry.stderr),
            )
            if got != want
        ]
        if args.repin:
            reference.repin(entry, result)
        state = (
            "repinned"
            if (drift and args.repin)
            else ("MOVED: " + ", ".join(drift) if drift else "ok")
        )
        moved += bool(drift)
        print(f"{entry.name:24} exit={result.exit_code} {state}")
    if moved and not args.repin:
        print(
            f"\n{moved} entr{'y' if moved == 1 else 'ies'} moved — look before `--repin`.",
            file=sys.stderr,
        )
        return 1
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="harness", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    run = sub.add_parser("run", help="run the grid")
    run.add_argument("--dry-run", action="store_true", help="stub subject, no API calls")
    run.add_argument("--all", action="store_true", help="every available domain (default)")
    run.add_argument("--domain", action="append", help="restrict to a domain (repeatable)")
    run.add_argument("--smoke", action="store_true", help="one task only")
    run.add_argument(
        "--max-turns", type=int, default=DEFAULT_MAX_TURNS, help="stopping rule per cell"
    )
    run.add_argument(
        "--budget",
        type=float,
        default=DEFAULT_MAX_BUDGET_USD,
        help="hard USD ceiling per cell",
    )
    run.add_argument(
        "--strength",
        action="append",
        help="restrict to a strength (repeatable); default is every strength",
    )
    run.add_argument(
        "--arm",
        action="append",
        help="restrict to an arm (repeatable); default is every arm. A slice, "
        "not a design change — the comparisons a run can support are the ones "
        "whose arms it holds",
    )
    run.add_argument(
        "--repeats",
        type=int,
        default=1,
        help="run each cell N times. The subject is stochastic and one trial "
        "cannot separate a reliable answer from a lucky one",
    )
    run.add_argument(
        "--ablate",
        metavar="BLOCK",
        help="cut a named documentation block from the engine arm's skill "
        "(`harness blocks` lists them); implies engine arm only",
    )
    run.add_argument(
        "--resume",
        metavar="RUN_DIR",
        help="finish a run that stopped: re-run the cells it is missing or "
        "recorded as ERROR, appending into the same run directory",
    )
    run.add_argument(
        "--limit",
        type=int,
        help="stop cleanly after N cells, so a run can be sized to the session "
        "window instead of discovering its edge",
    )
    run.add_argument(
        "--local-model",
        action="append",
        metavar="MODEL",
        help="run against a locally-served model instead of the API (repeatable). "
        "Crossed with --protocol, so each combination is its own strength",
    )
    run.add_argument(
        "--protocol",
        action="append",
        choices=("native", "structured"),
        help="how the local subject calls tools: a `tools` array, or a "
        "constrained decoder. Repeatable; which one wins is a property of the "
        "model, so both is a legitimate sweep",
    )
    run.add_argument(
        "--endpoint",
        default=local.DEFAULT_BASE_URL,
        help="OpenAI-compatible base URL for --local-model",
    )
    run.add_argument(
        "--min-context",
        type=int,
        default=local.DEFAULT_CONTEXT_TOKENS,
        help="refuse to start if a model is served with a smaller window. A "
        "context below what the run assumes is a different experiment, not a "
        "degraded one; 0 skips the check",
    )
    run.add_argument(
        "--max-output-tokens",
        type=int,
        default=local.DEFAULT_MAX_OUTPUT_TOKENS,
        help="tokens one completion may generate. Bounds **reasoning plus "
        "answer**, so a thinking model needs this raised or it spends the whole "
        "budget thinking and emits no action",
    )
    run.add_argument(
        "--reasoning-effort",
        help="passed through to a thinking model; `none` turns thinking off",
    )
    run.add_argument(
        "--max-cell-seconds",
        type=int,
        default=local.DEFAULT_MAX_CELL_SECONDS,
        help="wall clock per cell. A local run is bounded by time, not money",
    )
    run.add_argument(
        "--slate",
        metavar="MANIFEST",
        help="run the calibrated slate a manifest names, instead of the pinned "
        "one; the items are regenerated from it and refused if they have moved",
    )
    run.add_argument("--yes", action="store_true", help="required to start a paid run")
    run.set_defaults(func=cmd_run)

    pw = sub.add_parser("power", help="items needed to detect an effect")
    pw.add_argument("--effect", type=float, default=0.10, help="effect to detect, as a rate")
    pw.add_argument("--baseline", type=float, default=0.85, help="the weaker arm's accuracy")
    pw.add_argument("--power", type=float, default=0.80, help="probability of detecting it")
    pw.set_defaults(func=cmd_power)

    cal = sub.add_parser("calibrate", help="select a slate from a generated pool")
    cal.add_argument("--dry-run", action="store_true", help="stub subject, no API calls")
    cal.add_argument("--domain", action="append", help="restrict the pool to a pack (repeatable)")
    cal.add_argument("--seed", action="append", type=int, help="a seed to draw from (repeatable)")
    cal.add_argument(
        "--difficulty", action="append", type=int, help="a difficulty to draw (repeatable)"
    )
    cal.add_argument(
        "--track",
        action="append",
        choices=("in-context", "at-scale"),
        help="which track to draw; default in-context. `at-scale` is calibrated "
        "too and its band is a floor — prose scoring ~0 is what that track claims",
    )
    cal.add_argument(
        "--trials",
        type=int,
        default=calibrate.TRIALS,
        help="trials per item. One is 0 or 1 and the band is then empty by "
        "construction; three makes it expressible",
    )
    cal.add_argument(
        "--strength",
        default=calibrate.STRENGTH.name,
        help="the one strength the pass runs at; the weak end is where the "
        "signal is, so the default is the weaker model",
    )
    cal.add_argument(
        "--local-model",
        action="append",
        metavar="MODEL",
        help="calibrate against a locally-served model instead of the API. A "
        "slate is calibrated for one subject, so unlike `run` this takes one "
        "model and one protocol",
    )
    cal.add_argument(
        "--protocol",
        action="append",
        choices=("native", "structured"),
        help="how the local subject calls tools: a `tools` array, or a constrained decoder",
    )
    cal.add_argument(
        "--endpoint",
        default=local.DEFAULT_BASE_URL,
        help="OpenAI-compatible base URL for --local-model",
    )
    cal.add_argument(
        "--min-context",
        type=int,
        default=local.DEFAULT_CONTEXT_TOKENS,
        help="refuse to start if a model is served with a smaller window; 0 skips the check",
    )
    cal.add_argument(
        "--max-output-tokens",
        type=int,
        default=local.DEFAULT_MAX_OUTPUT_TOKENS,
        help="tokens one completion may generate. Bounds **reasoning plus "
        "answer**, so a thinking model needs this raised or it spends the whole "
        "budget thinking and emits no action",
    )
    cal.add_argument(
        "--reasoning-effort",
        help="passed through to a thinking model; `none` turns thinking off",
    )
    cal.add_argument(
        "--max-cell-seconds",
        type=int,
        default=local.DEFAULT_MAX_CELL_SECONDS,
        help="wall clock per cell. A local pass is bounded by time, not money",
    )
    cal.add_argument("--limit", type=int, help="stop cleanly after N cells")
    cal.add_argument(
        "--max-turns", type=int, default=DEFAULT_MAX_TURNS, help="stopping rule per cell"
    )
    cal.add_argument(
        "--budget", type=float, default=DEFAULT_MAX_BUDGET_USD, help="hard USD ceiling per cell"
    )
    cal.add_argument("--out", metavar="MANIFEST", help="where to write the slate")
    cal.add_argument(
        "--from",
        dest="from_run",
        metavar="RUN_DIR",
        help="select from a pass already run, without running anything — moving "
        "the band is a reading of the evidence, not a reason to re-collect it",
    )
    cal.add_argument("--yes", action="store_true", help="required to start a paid pass")
    cal.set_defaults(func=cmd_calibrate)

    lad = sub.add_parser("ladder", help="pin a difficulty ladder, screened by nothing")
    lad.add_argument("--domain", action="append", help="restrict the ladder to a pack (repeatable)")
    lad.add_argument(
        "--seed", type=int, required=True, help="the one seed every rung is drawn from"
    )
    lad.add_argument("--difficulty", action="append", type=int, help="a rung to draw (repeatable)")
    lad.add_argument(
        "--track",
        action="append",
        choices=("in-context", "at-scale"),
        help="which track to draw; default in-context",
    )
    lad.add_argument(
        "--question",
        action="append",
        help="keep only this question, at every rung (repeatable). Holding the "
        "question constant is what leaves difficulty as the only thing varying "
        "along the ladder; a name that matches nothing is refused",
    )
    lad.add_argument(
        "--strength",
        default=calibrate.STRENGTH.name,
        help="the subject the ladder is for. Recorded so `run --slate` refuses a "
        "grid that does not hold it",
    )
    lad.add_argument("--out", metavar="MANIFEST", help="where to write the slate")
    lad.set_defaults(func=cmd_ladder)

    sc = sub.add_parser("score", help="grade one Datalog program against one task")
    sc.add_argument("--task", required=True, metavar="DOMAIN/ID")
    sc.add_argument("--program", required=True, metavar="FILE")
    sc.add_argument(
        "--relation",
        help="which derived relation is the answer; default is the engine's "
        "synthesized `answer`, or the sole relation printed",
    )
    sc.add_argument(
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT,
        help="seconds. The engine has no budget of its own, by decision",
    )
    sc.set_defaults(func=cmd_score)

    rep = sub.add_parser("report", help="render a run to markdown")
    rep.add_argument("run_dir")
    rep.set_defaults(func=cmd_report)

    sub.add_parser("domains", help="list the slate and what exists").set_defaults(func=cmd_domains)

    sub.add_parser("blocks", help="the documentation blocks an ablation can cut").set_defaults(
        func=cmd_blocks
    )

    ref = sub.add_parser("reference", help="run the pinned reference corpus")
    ref.add_argument(
        "--repin", action="store_true", help="adopt what it printed — read the diff first"
    )
    ref.set_defaults(func=cmd_reference)

    cor = sub.add_parser("corpus", help="fetch the pinned source corpora")
    cor.add_argument(
        "action", nargs="?", default="list", choices=("list", "fetch"), help="default: list"
    )
    cor.set_defaults(func=cmd_corpus)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
