"""``harness`` — run the grid, render a run.

``--dry-run`` is the offline gate: the full grid against the stub subject, no API
calls, no cost. It is what CI runs, and what a change has to keep passing.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

from harness import ablate, arms, corpus, domains, reference, report
from harness.agent import (
    DEFAULT_MAX_BUDGET_USD,
    DEFAULT_MAX_TURNS,
    AgentSubject,
)
from harness.cell import ARMS, STRENGTHS, grid
from harness.record import RecordStore
from harness.runner import new_run_id, run_grid
from harness.subject import StubSubject

RESULTS_ROOT = Path(__file__).resolve().parents[2] / "results"
WORKSPACE_ROOT = arms.WORKSPACE_ROOT


def _progress(index: int, total: int, record) -> None:
    print(f"[{index:>3}/{total}] {record.cell_id:<52} {record.verdict}", file=sys.stderr)


def cmd_run(args: argparse.Namespace) -> int:
    names = args.domain or None
    tasks = domains.load_all(names)
    if not tasks:
        print("no domain packs available yet", file=sys.stderr)
        return 1
    if args.smoke:
        tasks = tasks[:1]

    strengths = STRENGTHS
    if args.strength:
        wanted = set(args.strength)
        strengths = tuple(s for s in STRENGTHS if s.name in wanted)
        if not strengths:
            known = ", ".join(s.name for s in STRENGTHS)
            print(f"no such strength: {sorted(wanted)} — have {known}", file=sys.stderr)
            return 1

    # An ablation is engine-arm only: cutting a block of the engine's own
    # documentation cannot move an arm that never had it, so a prose cell here
    # would be paying to re-measure the control.
    cell_arms = ("engine",) if args.ablate else ARMS
    if args.ablate:
        available = ablate.catalogue(arms.DATALOG_SKILL_DIR)
        if args.ablate not in available:
            print(
                f"no block named {args.ablate!r} — have "
                f"{', '.join(sorted(available)) or '(none marked)'}",
                file=sys.stderr,
            )
            return 1

    cells = grid(tasks, strengths, cell_arms, args.ablate)

    # A real run spends money on someone's account, so it says how much it could
    # cost and refuses to start without being told to. The store is built after
    # this, not before: a refused run should leave no trace in `results/`, which
    # is a record of runs that happened.
    if not args.dry_run:
        ceiling = len(cells) * args.budget
        print(
            f"{len(cells)} cells at up to {args.budget:.2f} USD each — ceiling {ceiling:.2f} USD.",
            file=sys.stderr,
        )
        if not args.yes:
            print("re-run with --yes to spend it.", file=sys.stderr)
            return 2

    store = RecordStore(RESULTS_ROOT, new_run_id("dry" if args.dry_run else "run"))
    store.write_metadata(
        dry_run=args.dry_run,
        domains=sorted({task.domain for task in tasks}),
        tasks=len(tasks),
        cells=len(cells),
        strengths=[strength.name for strength in strengths],
        arms=list(cell_arms),
        ablate=args.ablate,
        max_turns=args.max_turns,
        max_budget_usd=args.budget,
    )

    subject = (
        StubSubject()
        if args.dry_run
        else AgentSubject(max_turns=args.max_turns, max_budget_usd=args.budget)
    )
    result = run_grid(cells, subject, store, WORKSPACE_ROOT, on_cell=_progress)
    path = report.write(store.dir)
    simulated = " (simulated)" if args.dry_run else ""
    print(f"\n{len(result.records)} cells · {result.cost_usd:.2f} USD{simulated}")
    print(f"report: {path}")
    return 0


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
        "--ablate",
        metavar="BLOCK",
        help="cut a named documentation block from the engine arm's skill "
        "(`harness blocks` lists them); implies engine arm only",
    )
    run.add_argument("--yes", action="store_true", help="required to start a paid run")
    run.set_defaults(func=cmd_run)

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
