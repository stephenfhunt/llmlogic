"""Cutting a doc block, and the two ways that measurement goes wrong quietly.

An ablation compares a cell against itself minus one paragraph, so the *only*
difference between the two workspaces has to be that paragraph. Two failures
would leave the numbers looking fine:

- **A marker survives into a workspace.** Then the conditions differ by a comment
  as well as by the cut, and every engine cell in every ordinary run carries a
  stray token nobody meant to ship.
- **The cut misses.** A renamed or deleted block means the ablated cell is the
  control, run twice, at twice the price — and it reports a null result that
  reads as "the doc line does not matter".
"""

import filecmp
from pathlib import Path

import pytest

from harness import ablate, arms
from harness.cell import HAIKU_4_5, OPUS_5, Cell, grid
from harness.domains.controls import tasks as controls_tasks

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; `cargo build --release --offline` in datalog/",
)

SKILL_ROOT = ".claude/skills/datalog"

MARKED = """\
Intro line.

<!-- block: alpha -->
The alpha guidance.
<!-- /block -->

Between the two.

<!-- block: beta -->
The beta guidance.
<!-- /block -->

Trailing line.
"""


# ---- The marker syntax --------------------------------------------------------


def test_blocks_are_found_with_their_size():
    found = {block.name: block for block in ablate.blocks_in(MARKED, "doc.md")}
    assert set(found) == {"alpha", "beta"}
    assert found["alpha"].lines == 1
    assert found["alpha"].words == 3


def test_strip_removes_the_markers_and_keeps_the_text():
    stripped = ablate.strip(MARKED)
    assert "<!--" not in stripped
    assert "The alpha guidance." in stripped
    assert "The beta guidance." in stripped
    assert "Intro line." in stripped


def test_cut_removes_one_block_and_leaves_the_other():
    text, removed = ablate.cut(MARKED, "alpha")
    assert removed
    assert "The alpha guidance." not in text
    assert "The beta guidance." in text
    assert "Between the two." in text


def test_cut_reports_a_miss_rather_than_silently_doing_nothing():
    text, removed = ablate.cut(MARKED, "gamma")
    assert not removed
    assert text == MARKED


def test_a_block_defined_twice_is_an_error(tmp_path):
    # Two blocks under one name means an ablation cuts both and the result names
    # neither. Better to refuse than to report an average of two cuts.
    (tmp_path / "one.md").write_text("<!-- block: dup -->\na\n<!-- /block -->\n")
    (tmp_path / "two.md").write_text("<!-- block: dup -->\nb\n<!-- /block -->\n")
    with pytest.raises(ablate.UnknownBlock, match="defined twice"):
        ablate.catalogue(tmp_path)


# ---- The real skill -----------------------------------------------------------


def test_the_skill_marks_the_blocks_an_ablation_is_for():
    found = ablate.catalogue(arms.DATALOG_SKILL_DIR)
    # The count-distinct guidance is the one an ablation was wanted for: the open
    # question in `datalog/ROADMAP.md` is whether documenting the trap is enough,
    # and this is how that gets an answer instead of an opinion.
    assert "count-wildcard" in found
    assert "source-analysis-count-trap" in found
    assert found["source-analysis-count-trap"].document == "recipes/source-analysis.md"


def test_cutting_an_unmarked_name_raises_rather_than_running(tmp_path):
    (tmp_path / "SKILL.md").write_text("no markers here\n")
    with pytest.raises(ablate.UnknownBlock, match="produces a result about nothing"):
        ablate.apply(tmp_path, "count-wildcard")


# ---- A real workspace ---------------------------------------------------------


def _engine_workspace(root: Path, ablated: str | None, strength=OPUS_5) -> Path:
    task = controls_tasks.tasks()[0]
    return arms.build(Cell(task, "engine", strength, ablate=ablated), root).path


def test_an_ordinary_engine_workspace_carries_no_markers(tmp_path):
    # The one that would go wrong in every run rather than only in an ablation.
    path = _engine_workspace(tmp_path, None)
    for document in (path / SKILL_ROOT).rglob("*.md"):
        text = document.read_text(encoding="utf-8")
        assert "<!-- block" not in text, f"{document.name} shipped a marker"
        assert "<!-- /block" not in text, f"{document.name} shipped a marker"


def test_the_ablated_workspace_is_missing_exactly_the_block(tmp_path):
    control = _engine_workspace(tmp_path, None)
    ablated = _engine_workspace(tmp_path, "source-analysis-count-trap")

    recipe = "recipes/source-analysis.md"
    kept = (control / SKILL_ROOT / recipe).read_text(encoding="utf-8")
    cut = (ablated / SKILL_ROOT / recipe).read_text(encoding="utf-8")

    assert "36 — call sites" in kept
    assert "36 — call sites" not in cut
    # The neighbours stay: an ablation that takes the section with it measures
    # the section, and the finding would be attributed to one paragraph.
    for neighbour in ("A join across two id-spaces", "Disjunction is rule-only"):
        assert neighbour in cut


def test_no_other_file_in_the_skill_differs(tmp_path):
    control = _engine_workspace(tmp_path, None)
    ablated = _engine_workspace(tmp_path, "count-wildcard")

    left, right = control / SKILL_ROOT, ablated / SKILL_ROOT
    comparison = filecmp.dircmp(left, right)
    changed = []

    def walk(node, prefix=""):
        assert not node.left_only, f"{prefix}: only in the control — {node.left_only}"
        assert not node.right_only, f"{prefix}: only in the ablated — {node.right_only}"
        changed.extend(f"{prefix}{name}" for name in node.diff_files)
        for name, sub in node.subdirs.items():
            walk(sub, f"{prefix}{name}/")

    walk(comparison)
    assert changed == ["SKILL.md"], f"the cut moved more than one file: {changed}"


def test_the_prose_arm_has_no_skill_to_cut_from(tmp_path):
    task = controls_tasks.tasks()[0]
    workspace = arms.build(Cell(task, "prose", OPUS_5, ablate="count-wildcard"), tmp_path)
    assert not (workspace.path / ".claude").exists()
    assert workspace.ablated is None


# ---- The cell it produces -----------------------------------------------------


def test_an_ablated_cell_is_a_different_cell():
    task = controls_tasks.tasks()[0]
    plain = Cell(task, "engine", HAIKU_4_5)
    cut = Cell(task, "engine", HAIKU_4_5, ablate="count-wildcard")
    assert plain.id != cut.id
    assert cut.id.endswith(".minus-count-wildcard")


def test_an_ablation_grid_is_engine_only():
    tasks = controls_tasks.tasks()
    cells = grid(tasks, (HAIKU_4_5,), ("engine",), "count-wildcard")
    assert {cell.arm for cell in cells} == {"engine"}
    assert all(cell.ablate == "count-wildcard" for cell in cells)
    assert len(cells) == len(tasks)


def test_the_workspace_hash_separates_an_ablated_cell_from_its_control(tmp_path):
    # The directory is named by a hash of the cell id. If the ablation were not
    # in that id the two conditions would share a directory, and the second would
    # run against whatever the first left behind.
    control = _engine_workspace(tmp_path, None)
    ablated = _engine_workspace(tmp_path, "count-wildcard")
    assert control != ablated
