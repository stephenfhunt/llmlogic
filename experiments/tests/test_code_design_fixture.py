"""The `code_design` workspace, and the claims its assembly rests on.

A TypeScript project is what its `tsconfig` includes *plus everything that
resolves from there*, so this fixture is assembled rather than copied
(`harness.domains.code_design.fixture`). Two of its constants would break the
corpus quietly if they went stale, and both are re-derived here from the source
text — never from `code-facts`, which is the tool under test in this pack.
"""

from __future__ import annotations

import re

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.corpus import VSCODE, VSCODE_TYPES
from harness.domains import code_design
from harness.domains.code_design import fixture

pytestmark = pytest.mark.skipif(
    fixture.unavailable() is not None,
    reason=fixture.unavailable() or "",
)

#: `from "…"`, `import "…"`, `export … from "…"`, `import("…")`. Deliberately a
#: regex over text: the point is to agree with a real parse without being one,
#: and a specifier is a string literal in a fixed position.
SPECIFIER = re.compile(r"""(?:\bfrom|\bimport|\brequire)\s*\(?\s*['"]([^'"]+)['"]""")


def _resolve(importer: str, specifier: str) -> str | None:
    """A relative specifier to a **TypeScript file** in the tree, `.js` → `.ts`.

    Anything bare (an npm package, a path mapping) is left to the compiler. And
    only `.ts` counts: `import './media/quickInput.css'` names a real file on
    disk and is still not part of the program — it resolves through the ambient
    `declare module "*.css"`, which is why `imports.target_ambient` exists
    (`code-analysis/decisions.md` 2026-09-11 later ii). Counting it here would
    put a stylesheet in `REACHED_OUT` and change nothing about the fact base.
    """
    if not specifier.startswith("."):
        return None
    parts = importer.split("/")[:-1]
    for segment in specifier.split("/"):
        if segment == ".":
            continue
        if segment == "..":
            parts.pop()
        else:
            parts.append(segment)
    path = "/".join(parts)
    for candidate in (re.sub(r"\.js$", ".ts", path), f"{path}.ts"):
        if candidate.endswith(".ts") and (VSCODE.root / candidate).is_file():
            return candidate
    return None


def _reached_from_subject() -> set[str]:
    """Every file outside `src/vs/base` that `vs/base` reaches, transitively."""
    root = VSCODE.root
    seen: set[str] = set()
    queue = [p.relative_to(root).as_posix() for p in (root / fixture.SUBJECT).rglob("*.ts")]
    while queue:
        current = queue.pop()
        text = (root / current).read_text(encoding="utf-8")
        for specifier in SPECIFIER.findall(text):
            target = _resolve(current, specifier)
            if target is None or target.startswith(f"{fixture.SUBJECT}/") or target in seen:
                continue
            seen.add(target)
            queue.append(target)
    return seen


def test_reached_out_is_exactly_what_vs_base_imports_from_outside_itself():
    # The list is stated in `fixture.py` so a reader can see the corpus's shape
    # without running anything. This is what stops it being a guess: leaving a
    # file out does not shrink the corpus, it changes the fact base — 1,336,528
    # facts and 463 unresolved names instead of 1,363,422 and 286.
    assert _reached_from_subject() == set(fixture.REACHED_OUT)


def test_the_workspace_is_the_layout_the_extractor_needs():
    files = fixture.sources()
    # `node_modules` at the workspace root, so resolution finds it by walking up
    # from `vscode/src/vs/base/**` and nothing needs a `paths` mapping.
    assert any(name.startswith("node_modules/@types/") for name in files)
    assert f"{fixture.PACKAGE}/src/tsconfig.json" in files
    assert f"{fixture.PACKAGE}/{fixture.BASE_TSCONFIG}" in files
    # The ambient declarations are what make an asset import resolve at all.
    assert f"{fixture.PACKAGE}/src/typings/css.d.ts" in files
    for relative in fixture.REACHED_OUT:
        assert f"{fixture.PACKAGE}/{relative}" in files


def test_no_manifest_travels_into_the_workspace():
    # The pack's own `package.json` and lockfile stay in the repository: a cell
    # has no network and could not install from them, and a manifest naming only
    # `@types` would make `packages.dl` call every dependency unused. The ones
    # *inside* `node_modules` are a different thing — resolution needs them.
    top_level = [name for name in fixture.sources() if "/" not in name.rstrip("/")]
    assert top_level == []


def test_the_fixture_is_at_scale_by_a_wide_margin():
    # `at-scale` is defined as exceeding the prose arm's window on purpose. A
    # corpus that quietly shrank under that line would turn a claim about scale
    # into a claim about reasoning, reported in the wrong table.
    chars = sum(len(c) for c in fixture.sources().values() if isinstance(c, str))
    assert chars // 4 > 10 * FIXTURE_TOKEN_BUDGET


def test_availability_names_the_half_that_is_missing(monkeypatch):
    monkeypatch.setattr(type(VSCODE_TYPES), "present", lambda self: False)
    reason = code_design.unavailable()
    assert reason is not None and VSCODE_TYPES.slug in reason and "harness corpus fetch" in reason


def test_a_cell_workspace_materializes_at_this_scale(tmp_path):
    """The sealed copy, through the path a real cell takes.

    `static_analysis` writes 21 files; this writes 761 across 20 MB, and the
    nested keys that carry a source tree have to survive `catalogue.verify` and
    `arms.build` unchanged. Cheap to assert and the only thing that proves the
    `at-scale` fixture is a path and not a special case.
    """
    from harness import arms
    from harness.cell import HAIKU_4_5, Cell
    from harness.task import Answer, Task

    task = Task(
        domain="code_design",
        id="workspace-smoke",
        question="Which files are in a runtime import cycle?",
        truth=Answer.of(("x",)),
        fixture=fixture.build(),
        question_class="recursion",
        answer_shape=("file",),
        track="at-scale",
    )
    workspace = arms.build(Cell(task=task, arm="prose", strength=HAIKU_4_5), tmp_path)

    root = workspace.path
    assert (root / "vscode" / "src" / "tsconfig.json").is_file()
    assert (root / "node_modules" / "@types" / "node" / "package.json").is_file()
    assert (root / "vscode" / fixture.SUBJECT / "common" / "uri.ts").is_file()
    assert (root / "vscode" / fixture.REACHED_OUT[0]).is_file()
    # The prompt orients with one line per root, never an index of 761 paths.
    assert "a source tree: 507 files" in workspace.prompt
    assert "uri.ts" not in workspace.prompt
