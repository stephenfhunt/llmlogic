"""Rendering a run to markdown.

The report leads with the comparison S1 asks for and keeps the negative controls
in their own table. Mixing them into the headline would let four questions the
engine was never expected to help with move the number that decides v1.
"""

from __future__ import annotations

from collections import defaultdict
from pathlib import Path

from harness.cell import STRENGTHS
from harness.record import RecordStore

_ARMS = ("engine", "prose")


def _rate(records: list[dict]) -> str:
    if not records:
        return "—"
    correct = sum(1 for record in records if record["verdict"] == "correct")
    return f"{correct}/{len(records)} ({100 * correct / len(records):.0f}%)"


def _accuracy(records: list[dict]) -> float | None:
    if not records:
        return None
    return sum(1 for record in records if record["verdict"] == "correct") / len(records)


def _table(records: list[dict], title: str) -> list[str]:
    lines = [f"### {title}", ""]
    lines.append("| | " + " | ".join(s.name for s in STRENGTHS) + " | all |")
    lines.append("|---|" + "---|" * (len(STRENGTHS) + 1))
    for arm in _ARMS:
        arm_records = [r for r in records if r["arm"] == arm]
        cells = [
            _rate([r for r in arm_records if r["strength"] == strength.name])
            for strength in STRENGTHS
        ]
        lines.append(f"| **{arm}** | " + " | ".join(cells) + f" | {_rate(arm_records)} |")

    engine = _accuracy([r for r in records if r["arm"] == "engine"])
    prose = _accuracy([r for r in records if r["arm"] == "prose"])
    if engine is not None and prose is not None:
        delta = 100 * (engine - prose)
        lines += ["", f"**Delta: {delta:+.0f} points** (engine − prose)."]
    lines.append("")
    return lines


def render(run_dir: Path) -> str:
    records = RecordStore.load(run_dir)
    if not records:
        return f"# {run_dir.name}\n\nNo records.\n"

    graded = [r for r in records if r["verdict"] != "error"]
    controls = [r for r in graded if not r["engine_expected_to_help"]]
    measured = [r for r in graded if r["engine_expected_to_help"]]

    lines = [f"# {run_dir.name}", ""]
    lines.append(
        f"{len(records)} cells · {sum(r['cost_usd'] for r in records):.2f} USD · "
        f"{sum(1 for r in records if r['verdict'] == 'error')} errored"
    )
    lines.append("")

    lines += _table(measured, "S1 — the measured slate")
    lines += _table(controls, "Negative controls — the engine is *not* expected to help here")
    lines.append(
        "A delta on the controls is a warning about the instrument, not a result: "
        "these are single-hop lookups and one-step arithmetic."
    )
    lines += ["", "### By domain", ""]
    lines.append("| domain | class | engine | prose |")
    lines.append("|---|---|---|---|")
    by_domain: dict[str, list[dict]] = defaultdict(list)
    for record in graded:
        by_domain[record["domain"]].append(record)
    for domain in sorted(by_domain):
        rows = by_domain[domain]
        classes = sorted({r["question_class"] for r in rows})
        lines.append(
            f"| `{domain}` | {', '.join(classes)} "
            f"| {_rate([r for r in rows if r['arm'] == 'engine'])} "
            f"| {_rate([r for r in rows if r['arm'] == 'prose'])} |"
        )

    engine_records = [r for r in graded if r["arm"] == "engine"]
    reached = sum(1 for r in engine_records if r["signals"]["ran_engine"])
    switched = sum(1 for r in engine_records if r["signals"]["searches_after_engine"] > 0)
    first_programs = sum(1 for r in engine_records if r["first_program"])
    silent = sum(
        1 for r in graded if r["verdict"] == "wrong" and r["missing"] > 0 and r["extra"] == 0
    )
    denials = sum(len(r.get("denials", [])) for r in graded)
    cells_with_denials = sum(1 for r in graded if r.get("denials"))

    lines += [
        "",
        "### Process signals",
        "",
        f"- **Reached for the engine:** {reached}/{len(engine_records)} engine-arm cells.",
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
