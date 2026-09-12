"""The corpus as a cell sees it: a workspace `code-facts` can be pointed at.

`static_analysis` hands over 21 files of Python and lets the subject read them.
This is the same idea two orders of magnitude larger — 156k lines that do not
fit a context window, which is the whole of what the `at-scale` track claims —
and a TypeScript project is not a directory of files: it is whatever its
`tsconfig` includes, plus everything that resolves from there.

So the workspace is **assembled**, not copied:

    <root>/
      node_modules/@types/…              installed from the pinned lockfile
      src/tsconfig.json                  the pack's, from `corpora/vs-base-types/`
      src/tsconfig.base.json             VS Code's
      src/typings/*.d.ts                 its ambient declarations
      src/vs/base/**                      the subject matter
      src/vs/{amdX,nls}.ts + 9 more      what `vs/base` reaches (REACHED_OUT)

`node_modules` sits at the **workspace root** rather than beside the sources, so
module resolution finds it by walking up and nothing needs a `paths` mapping.
"""

from __future__ import annotations

from harness.corpus import VSCODE, VSCODE_TYPES
from harness.task import Fixture

#: The root inside the workspace, as the prompt names it.
PACKAGE = "vscode"

#: The subtree the questions are about.
SUBJECT = "src/vs/base"

#: `vs/base` is **not self-contained.** It imports these eleven files, and the
#: TypeScript program pulls them in whatever `include` says — so leaving them out
#: does not shrink the corpus, it breaks it: 1,336,528 facts and 463 unresolved
#: names instead of 1,363,422 and 286.
#:
#: Stated rather than discovered, and a test re-derives it from the sources: a
#: list that silently went stale would change the fact base under the answer key.
REACHED_OUT = (
    "src/vs/amdX.ts",
    "src/vs/nls.ts",
    "src/vs/platform/contextkey/common/contextkey.ts",
    "src/vs/platform/contextkey/common/scanner.ts",
    "src/vs/platform/instantiation/common/descriptors.ts",
    "src/vs/platform/instantiation/common/instantiation.ts",
    "src/vs/platform/instantiation/common/serviceCollection.ts",
    "src/vs/platform/quickinput/browser/quickInputUtils.ts",
    "src/vs/platform/quickinput/common/quickAccess.ts",
    "src/vs/platform/quickinput/common/quickInput.ts",
    "src/vs/platform/registry/common/platform.ts",
)

#: VS Code's own compiler options, which the pack's tsconfig extends.
BASE_TSCONFIG = "src/tsconfig.base.json"


def unavailable() -> str | None:
    """Both halves have to be on disk, and each says which one is missing."""
    for item in (VSCODE, VSCODE_TYPES):
        if not item.present():
            return (
                f"{item.slug} is not in the corpus cache — fetch it with "
                "`harness corpus fetch` (it needs the network, which a cell does not have)"
            )
    return None


def sources() -> dict[str, str]:
    """Every file the workspace carries, keyed by its path within it.

    Read once and held as text, so the oracle parses exactly the bytes the
    subject was given and neither touches the filesystem to disagree about it —
    `harness.corpus.Corpus.sources`' reasoning, at 826 files.
    """
    root = VSCODE.require()
    files: dict[str, str] = {}

    def take(relative: str) -> None:
        files[f"{PACKAGE}/{relative}"] = (root / relative).read_text(encoding="utf-8")

    for path in sorted((root / SUBJECT).rglob("*.ts")):
        take(path.relative_to(root).as_posix())
    for path in sorted((root / "src" / "typings").glob("*.d.ts")):
        take(path.relative_to(root).as_posix())
    for relative in REACHED_OUT:
        take(relative)
    take(BASE_TSCONFIG)

    # The pack's own tsconfig travels with the manifest, not with VS Code. The
    # `package.json` and lockfile beside it deliberately do **not**: a cell has
    # no network and could not install from them, a manifest naming only
    # `@types` would make `packages.dl` report every dependency unused, and the
    # pin belongs in the repository and in `run.json` where it can be audited.
    pack_tsconfig = VSCODE_TYPES.manifest / "tsconfig.pack.json"
    files[f"{PACKAGE}/src/tsconfig.json"] = pack_tsconfig.read_text(encoding="utf-8")

    # The installed declarations, at the workspace root so resolution walks up
    # to them. A cell has no network, so they are copied rather than installed.
    installed = VSCODE_TYPES.require()
    for path in sorted((installed / "node_modules").rglob("*")):
        if path.is_file():
            name = path.relative_to(installed).as_posix()
            files[name] = path.read_text(encoding="utf-8", errors="replace")
    return files


def build() -> Fixture:
    return Fixture(files=dict(sources()), schemas={})
