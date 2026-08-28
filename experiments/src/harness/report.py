"""Rendering a run to markdown.

Three things changed after the first grid could not be read (`decisions.md`
2026-08-25), and each is a section here:

- **Three arms, compared pairwise.** ``engine-forced`` vs ``prose`` is S1's
  sentence read literally; ``engine`` vs ``prose`` folds in whether the subject
  picks the engine up at all. One number cannot be both.
- **Every rate gets an interval, and every delta a paired test.** A delta with no
  interval is not evidence of no effect. The pairing is the design — both arms
  answered the same tasks — so McNemar is the right test and is far more powerful
  here than comparing two independent proportions.
- **Partial credit beside the verdict.** Binary set equality cannot tell an answer
  that dropped one row of forty from one that returned nothing.

The negative controls keep their own table. Mixing them into the headline would
let four questions the engine was never expected to help with move the number
that decides v1. The two **tracks** are separated for the same reason and a
stronger one: they are claims about different things.
"""

from __future__ import annotations

from collections import defaultdict
from pathlib import Path

from harness.cell import ARMS, STRENGTHS
from harness.record import RecordStore
from harness.resume import failed
from harness.runner import STOPPING_RULE
from harness.stats import bootstrap_delta, f1, mcnemar, wilson

#: Which pairings the report renders, and what each one asks. Ordered most
#: directly-answering-S1 first.
COMPARISONS: tuple[tuple[str, str, str], ...] = (
    ("engine-forced", "prose", "does the engine make the agent right?"),
    ("engine", "prose", "does *supplying* the engine help?"),
    ("engine", "engine-forced", "what does not reaching for it cost?"),
)


def _correct(record: dict) -> bool:
    return record["verdict"] == "correct"


def _rate(records: list[dict]) -> str:
    if not records:
        return "—"
    correct = sum(1 for record in records if _correct(record))
    return f"{correct}/{len(records)} ({100 * correct / len(records):.0f}%)"


def _rate_with_interval(records: list[dict]) -> str:
    if not records:
        return "—"
    correct = sum(1 for record in records if _correct(record))
    interval = wilson(correct, len(records))
    return f"{correct}/{len(records)} ({100 * correct / len(records):.0f}%) {interval}"


def _mean_f1(records: list[dict]) -> str:
    """Mean per-item F1, or ``—`` for a run recorded before ``truth_size`` was.

    Deliberately not reconstructed from today's slate for an older run. The
    `scheduling` fixture was repaired after the 2026-08-24 grid, so recovering a
    truth size from the current packs would grade a past run against an oracle it
    never saw — the exact defect class the staleness guard exists for.
    """
    scored = [r for r in records if r.get("truth_size") is not None]
    if not scored or len(scored) != len(records):
        return "—"
    total = sum(f1(r["missing"], r["extra"], r["truth_size"]) for r in scored)
    return f"{total / len(scored):.2f}"


def _paired_units(records: list[dict], arm: str) -> dict[str, bool]:
    """Pairing key -> was it correct, for one arm.

    The key is **task *and* strength**. Pairing on the task alone would treat
    opus and haiku as two trials of one subject, which halves the number of
    paired observations and averages two subjects whose whole reason for being
    in the grid is that they differ — the weaker one is the informative one.

    Repeat *trials* of the same cell do collapse, by thresholded mean, because
    McNemar wants one observation per pair. Ties go to *incorrect*: the
    conservative direction for a harness whose standing risk is flattering the
    engine.
    """
    trials: dict[str, list[bool]] = defaultdict(list)
    for record in records:
        if record["arm"] == arm:
            key = f"{record['domain']}/{record['task_id']}@{record['strength']}"
            trials[key].append(_correct(record))
    return {key: (sum(runs) / len(runs)) > 0.5 for key, runs in trials.items()}


def _arms_present(records: list[dict]) -> tuple[str, ...]:
    seen = {record["arm"] for record in records}
    return tuple(arm for arm in ARMS if arm in seen)


def _strengths_present(records: list[dict]) -> list[str]:
    """The strengths this run actually holds, in a stable order.

    Derived from the records rather than from `STRENGTHS`, for the reason
    `_arms_present` already is: the constant names the *Anthropic* pair, and a
    local sweep's strengths — one per model × protocol — are not in it. Reading
    the constant here rendered a local run as two empty columns with its own
    numbers nowhere. The known pair keeps its order; anything else follows,
    sorted, so a report is stable across runs.
    """
    seen = {record["strength"] for record in records}
    known = [s.name for s in STRENGTHS if s.name in seen]
    return known + sorted(seen - set(known))


def _table(records: list[dict], title: str) -> list[str]:
    arms = _arms_present(records)
    lines = [f"### {title}", ""]
    if not records:
        return lines + ["No cells.", ""]

    strengths = _strengths_present(records)
    lines.append("| | " + " | ".join(strengths) + " | all | mean F1 |")
    lines.append("|---|" + "---|" * (len(strengths) + 2))
    for arm in arms:
        arm_records = [r for r in records if r["arm"] == arm]
        cells = [_rate([r for r in arm_records if r["strength"] == name]) for name in strengths]
        lines.append(
            f"| **{arm}** | "
            + " | ".join(cells)
            + f" | {_rate_with_interval(arm_records)} | {_mean_f1(arm_records)} |"
        )
    lines.append("")
    lines += _comparisons(records, arms)
    return lines


def _comparisons(records: list[dict], arms: tuple[str, ...]) -> list[str]:
    """The paired tests. Each row is a question, not a score."""
    rows = []
    for first, second, question in COMPARISONS:
        if first not in arms or second not in arms:
            continue
        paired = mcnemar(_paired_units(records, first), _paired_units(records, second))
        if paired.n == 0:
            continue
        interval = bootstrap_delta(_paired_units(records, first), _paired_units(records, second))
        rows.append(
            f"| {first} − {second} | {question} | {100 * paired.delta:+.0f} pts | "
            f"[{100 * interval.low:+.0f}, {100 * interval.high:+.0f}] | "
            f"{paired.b}/{paired.c} | {paired.p_value:.3f} | {paired.n} |"
        )
    if not rows:
        return []
    header = [
        "| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |",
        "|---|---|---|---|---|---|---|",
    ]
    tail = [
        "",
        "**wins/losses** are the discordant pairs — the tasks the two arms "
        "disagreed on, and the only ones carrying information about a "
        "difference. `p` is McNemar's exact test. A wide interval around a small "
        "delta is *not* a null: it is the slate saying it was too small or too "
        "easy to tell.",
        "",
    ]
    return header + rows + tail


def _unparseable_line(records: list[dict]) -> str:
    """How the answers that would not parse failed, not just how many.

    Worth a line of its own because the three causes are not one finding: a
    single-column answer joined with `|` was the **prompt** telling it to
    (`catalogue._fields_line`, fixed 2026-08-26), while a sentence where rows
    belong is the subject ignoring the format. Runs recorded before the reason
    existed say so rather than being filled in.
    """
    unparseable = [r for r in records if r["verdict"] == "unparseable"]
    if not unparseable:
        return "- **Answers that would not parse:** none."
    counts: dict[str, int] = {}
    for record in unparseable:
        reason = record.get("unparseable_reason") or "not recorded"
        counts[reason] = counts.get(reason, 0) + 1
    why = ", ".join(f"{n} {reason}" for reason, n in sorted(counts.items(), key=lambda p: -p[1]))
    return (
        f"- **Answers that would not parse:** {len(unparseable)} — {why}. "
        "A format failure is not a wrong answer, and `wrong-arity` on a "
        "single-column question is the instrument's own to fix, not the subject's."
    )


def _budget_line(records: list[dict]) -> list[str]:
    """Where each arm's budget bound it, and what it declared instead.

    Two numbers that have to travel together with any `engine-forced` result.
    The arms do not get the same turn and wall-clock budget — `cell.ARM_BUDGET`
    gives `engine-forced` twice, because it must write and repair a program
    before it can answer at all — and that is an arm-asymmetric instrument
    parameter sitting on the primary endpoint. **A result is only free of it
    while no arm is ending at its cap**, so the rate is printed rather than
    assumed, per arm, next to the outcome the budget was supposed to buy.

    Before this, `engine-forced` hit its cap in 48% of cells with 75% of those
    writing nothing, and the report said none of it.
    """
    if not records:
        return []
    arms_seen = sorted({r["arm"] for r in records})
    parts = []
    for arm in arms_seen:
        rows = [r for r in records if r["arm"] == arm]
        capped = sum(1 for r in rows if STOPPING_RULE.search(r.get("error") or ""))
        parts.append(f"{arm} {capped}/{len(rows)} ({100 * capped / len(rows):.0f}%)")
    unusable = [r for r in records if r["verdict"] == "engine-unusable"]
    lines = [
        f"- **Cells that ended at their budget:** {', '.join(parts)}. "
        "The arms do not share one budget (`cell.ARM_BUDGET`), so a difference "
        "between them is only readable while these are low — a capped cell "
        "measures the cap.",
        f"- **Declared the engine unusable:** {len(unusable)}. "
        "The subject tried, the engine would not run its program, and it said so "
        "instead of looping. Not a correct answer, but a different fact from an "
        "empty file — and evidence about the engine rather than a hole.",
    ]
    return lines


def _reach(records: list[dict]) -> list[str]:
    """Reach as an outcome with an interval, not a footnote.

    The first grid printed `9/56` in prose at the bottom of the report. It was
    the most important number in the run — with reach that low, 47 of 56 cells
    were not measuring the independent variable at all.
    """
    engine_records = [r for r in records if r["arm"] in ("engine", "engine-forced")]
    if not engine_records:
        return []
    lines = ["### Reach — did the subject actually use the engine?", ""]
    lines.append("| arm | strength | answered-from | invoked | none |")
    lines.append("|---|---|---|---|---|")
    for arm in _arms_present(engine_records):
        for name in _strengths_present(engine_records):
            rows = [r for r in engine_records if r["arm"] == arm and r["strength"] == name]
            if not rows:
                continue
            counts = defaultdict(int)
            for record in rows:
                counts[record["signals"].get("engine_use", "unrecorded")] += 1
            if counts["unrecorded"]:
                # A run made before the classification existed. Reporting its
                # `answered-from` as 0 would be a fabricated number, not a
                # missing one, so the row says what it actually knows.
                ran = sum(1 for r in rows if r["signals"].get("ran_engine"))
                lines.append(f"| {arm} | {name} | — (ran the engine: {ran}/{len(rows)}) | — | — |")
                continue
            answered = counts["answered-from"]
            interval = wilson(answered, len(rows))
            lines.append(
                f"| {arm} | {name} | {answered}/{len(rows)} {interval} "
                f"| {counts['invoked']} | {counts['none']} |"
            )
    lines += [
        "",
        "`invoked` is the middle case: it reached for the engine and no program "
        "ran. Only `answered-from` is engine use in the sense S1 means "
        "(`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; "
        "on `engine-forced` it is a **compliance check** — a low number there "
        "means the mandate did not take, and the comparison it feeds is void.",
        "",
    ]
    return lines


def latest(records: list[dict]) -> list[dict]:
    """One record per cell, the last attempt winning, in first-seen order.

    A resumed run holds every attempt at a cell — the errored one that stopped
    the grid and the one that finished it. Both stay in ``records.jsonl``, which
    is the record of what happened; the report is the reading of it, and a cell
    counted twice would weight one question against the rest.
    """
    seen: dict[str, dict] = {}
    for record in records:
        seen[record["cell_id"]] = record
    return list(seen.values())


def render(run_dir: Path) -> str:
    attempts = RecordStore.load(run_dir)
    if not attempts:
        return f"# {run_dir.name}\n\nNo records.\n"
    records = latest(attempts)

    graded = [r for r in records if not failed(r)]
    controls = [r for r in graded if not r["engine_expected_to_help"]]
    measured = [r for r in graded if r["engine_expected_to_help"]]

    lines = [f"# {run_dir.name}", ""]
    lines.append(
        f"{len(records)} cells · {sum(r['cost_usd'] for r in attempts):.2f} USD · "
        f"{sum(1 for r in records if failed(r))} errored"
    )
    resumed = len(attempts) - len(records)
    if resumed:
        lines.append("")
        lines.append(
            f"**Resumed**: {resumed} cell{'s' if resumed != 1 else ''} were run a "
            "second time after the first attempt was cut short, and are counted "
            "once, at their later attempt. The cost above is everything the run "
            "spent, including the attempts that produced nothing. A grid measured "
            "across more than one session window is still one grid, but it was not "
            "one sitting."
        )
    lines.append("")

    # The two tracks are separate claims and are never averaged together.
    for track, heading in (
        ("in-context", "S1 — the measured slate (in-context)"),
        ("at-scale", "At scale — the fact base does not fit the prose arm's window"),
    ):
        rows = [r for r in measured if r.get("track", "in-context") == track]
        if rows:
            lines += _table(rows, heading)

    lines += _table(controls, "Negative controls — the engine is *not* expected to help here")
    lines.append(
        "A delta on the controls is a warning about the instrument, not a result: "
        "these are single-hop lookups and one-step arithmetic."
    )

    lines += ["", "### By domain", ""]
    arms = _arms_present(graded)
    lines.append("| domain | class | " + " | ".join(arms) + " |")
    lines.append("|---|---|" + "---|" * len(arms))
    by_domain: dict[str, list[dict]] = defaultdict(list)
    for record in graded:
        by_domain[record["domain"]].append(record)
    for domain in sorted(by_domain):
        rows = by_domain[domain]
        classes = sorted({r["question_class"] for r in rows})
        cells = " | ".join(_rate([r for r in rows if r["arm"] == arm]) for arm in arms)
        lines.append(f"| `{domain}` | {', '.join(classes)} | {cells} |")

    lines += ["", "### By question class", ""]
    lines.append("| class | " + " | ".join(arms) + " |")
    lines.append("|---|" + "---|" * len(arms))
    by_class: dict[str, list[dict]] = defaultdict(list)
    for record in measured:
        by_class[record["question_class"]].append(record)
    for question_class in sorted(by_class):
        rows = by_class[question_class]
        cells = " | ".join(_rate([r for r in rows if r["arm"] == arm]) for arm in arms)
        lines.append(f"| {question_class} | {cells} |")
    lines.append("")

    lines += _reach(graded)

    engine_records = [r for r in graded if r["arm"] in ("engine", "engine-forced")]
    switched = sum(1 for r in engine_records if r["signals"]["searches_after_engine"] > 0)
    first_programs = sum(1 for r in engine_records if r["first_program"])
    silent = sum(
        1 for r in graded if r["verdict"] == "wrong" and r["missing"] > 0 and r["extra"] == 0
    )
    denials = sum(len(r.get("denials", [])) for r in graded)
    cells_with_denials = sum(1 for r in graded if r.get("denials"))

    lines += [
        "### Process signals",
        "",
        _unparseable_line(graded),
        *_budget_line(graded),
        f"- **First program captured before feedback:** {first_programs}/{len(engine_records)}.",
        f"- **Switched back to search after using the engine:** {switched}. "
        "This is the silent one — the subject had the engine, tried it, and went back to text.",
        f"- **Answers that were a strict subset of the truth:** {silent}. "
        "Rows dropped, and nothing in the output says so.",
        f"- **Tool calls denied for leaving the workspace:** {denials}, across "
        f"{cells_with_denials} cells. A spike here is friction, not an attack — "
        "it usually means the prompt or the fixture made leaving look necessary. "
        "**A floor, not a count**: only the PreToolUse gate records a denial, and "
        "it flags an absolute path that already *exists*, so a write to a new path "
        "outside the workspace is stopped by the OS sandbox and never counted "
        "(seen on the first pilot). A low number is not evidence the subject "
        "stayed put.",
        "",
    ]
    return "\n".join(lines)


def write(run_dir: Path) -> Path:
    path = run_dir / "report.md"
    path.write_text(render(run_dir), encoding="utf-8")
    return path
