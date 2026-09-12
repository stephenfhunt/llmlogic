"""The `code_design` answer key, computed from the syntax tree and nothing else.

Control 1, stated for this pack: nothing here imports or runs `code-facts`,
`lib/` or the engine — and nothing here shells out at all, which
`tests/test_truth_independence.py` enforces for every `truth.py`, bluntly, so
that "the oracle shelled out to the engine" cannot happen by degrees. Parsing
is `harness.corpus.parse_typescript`, on the corpus-preparation side of that
line. What it *does* use is a real parser — `ts.createSourceFile`
through `parse.mjs`, the syntax tree with no checker — exactly as
`static_analysis`'s key uses `ast`, and for the same reason: an oracle with its
own hand-rolled parser trades the thing under test's bugs for its own, and a
wrong key voids both arms at once without saying so.

Everything above the syntax is this module's own: module resolution, the graphs,
and every answer. `vs/base` makes resolution simple enough to be exact — all
2,002 of its internal imports are relative, so it is path arithmetic with
TypeScript's `.js` → `.ts`, and no bare specifier names a file in the tree.

**The questions are syntactic and say so.** `tasks.py` spells out every
definition this module applies, because a rule the subject cannot apply the same
way grades a guess rather than an answer.
"""

from __future__ import annotations

import re
from collections import defaultdict
from functools import cache
from pathlib import Path

from harness.corpus import parse_typescript
from harness.domains.code_design import fixture
from harness.task import Answer

PARSER = Path(__file__).resolve().parent / "parse.mjs"

#: A parsed file: what `parse.mjs` emits, keyed by path within the corpus.
Parsed = dict[str, dict]


@cache
def parsed() -> Parsed:
    """Every TypeScript file the fixture ships, parsed once per process.

    Cached because `domains.load_all()` is called all over the test suite and
    this spawns Node over 507 files; the corpus is pinned, so the answer cannot
    change under the cache.
    """
    sources = fixture.build().files
    prefix = f"{fixture.PACKAGE}/"
    payload = [
        {"path": name[len(prefix) :], "text": text}
        for name, text in sorted(sources.items())
        if name.startswith(prefix) and name.endswith(".ts") and not name.endswith(".d.ts")
    ]
    return {entry["path"]: entry for entry in parse_typescript(payload, PARSER)}


def resolve(importer: str, specifier: str) -> str | None:
    """A relative specifier to a file in the tree, TypeScript's `.js` → `.ts`.

    Bare specifiers name node builtins and npm packages — 213 of them in
    `vs/base`, and not one names a file here. An asset import (`'./x.css'`)
    resolves through an ambient `declare module` and is not a module edge, so
    only `.ts` counts.
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
    files = parsed()
    for candidate in (re.sub(r"\.js$", ".ts", path), f"{path}.ts"):
        if candidate in files:
            return candidate
    return None


@cache
def runtime_edges() -> dict[str, frozenset[str]]:
    """`file -> the files it imports at run time`.

    An import is dropped when **every** name it binds is written `type`, or the
    whole clause is `import type` — the rule `tasks.py` states. An import that
    binds no names at all (`import './x.js'`) is a side-effect import and always
    runs.
    """
    edges: dict[str, set[str]] = defaultdict(set)
    for path, entry in parsed().items():
        for record in entry["imports"]:
            target = resolve(path, record["specifier"])
            if target is None or target == path:
                continue
            names = record["names"]
            if names and all(name["typeOnly"] for name in names):
                continue
            edges[path].add(target)
    return {path: frozenset(targets) for path, targets in edges.items()}


def _cycle_members(nodes: set[str], edges: dict[str, frozenset[str]]) -> set[str]:
    """Nodes on a cycle: those that reach themselves, by Tarjan's SCCs.

    A node is on a cycle when its strongly-connected component has more than one
    member, or it points at itself.
    """
    index: dict[str, int] = {}
    low: dict[str, int] = {}
    on_stack: set[str] = set()
    stack: list[str] = []
    counter = 0
    found: set[str] = set()

    def strongconnect(start: str) -> None:
        nonlocal counter
        # Iterative: `vs/base` has chains far deeper than the recursion limit.
        work: list[tuple[str, int]] = [(start, 0)]
        while work:
            node, position = work.pop()
            if position == 0:
                index[node] = low[node] = counter
                counter += 1
                stack.append(node)
                on_stack.add(node)
            successors = sorted(edges.get(node, frozenset()) & nodes)
            recursed = False
            for offset in range(position, len(successors)):
                successor = successors[offset]
                if successor not in index:
                    work.append((node, offset + 1))
                    work.append((successor, 0))
                    recursed = True
                    break
                if successor in on_stack:
                    low[node] = min(low[node], index[successor])
            if recursed:
                continue
            if low[node] == index[node]:
                component = []
                while True:
                    member = stack.pop()
                    on_stack.discard(member)
                    component.append(member)
                    if member == node:
                        break
                if len(component) > 1 or node in edges.get(node, frozenset()):
                    found.update(component)
            if work:
                parent, _ = work[-1]
                low[parent] = min(low[parent], low[node])

    for node in sorted(nodes):
        if node not in index:
            strongconnect(node)
    return found


def files_in_runtime_cycle(directory: str) -> Answer:
    """Files of `directory` that are on a runtime import cycle **within it**.

    Scoped to the directory on purpose: it is what makes one template many
    items, and it is a question about that directory's own shape rather than
    about the whole tree seen through it.
    """
    return Answer(
        frozenset((path,) for path in _cycle_members(files_under(directory), runtime_edges()))
    )


def files_under(directory: str) -> set[str]:
    """The files of `directory` **and everything beneath it**.

    The natural reading of "in this directory", and the one the questions state.
    Taking immediate children only would make `common/observableInternal` a
    different eight files from the ones anyone looking at the tree would name.
    """
    return {path for path in parsed() if path.startswith(f"{directory}/")}


def directories(minimum: int = 8) -> list[str]:
    """Directories holding at least `minimum` files, including subdirectories.

    The parameter that turns a template into items. A directory too small makes
    a question whose answer is obvious by inspection.
    """
    counts: dict[str, int] = defaultdict(int)
    for path in parsed():
        segments = path.split("/")[:-1]
        for depth in range(1, len(segments) + 1):
            counts["/".join(segments[:depth])] += 1
    return sorted(name for name, count in counts.items() if count >= minimum)


@cache
def imported_names() -> dict[str, frozenset[str]]:
    """`file -> the names some other file imports from it`.

    Direct imports only: no re-export chain is followed, which is what makes
    this computable by inspection and is stated in the question. A namespace
    import (`import * as x`) takes everything, so it marks the whole file used.
    """
    used: dict[str, set[str]] = defaultdict(set)
    for path, entry in parsed().items():
        for record in entry["imports"]:
            target = resolve(path, record["specifier"])
            if target is None or target == path:
                continue
            for name in record["names"]:
                if name.get("namespace"):
                    used[target].update(n["name"] for n in _exports_of(target))
                else:
                    used[target].add(name["name"])
    return {path: frozenset(names) for path, names in used.items()}


def _exports_of(path: str) -> list[dict]:
    entry = parsed().get(path)
    return [] if entry is None else [d for d in entry["topLevel"] if d["exported"]]


def unimported_exports(directory: str) -> Answer:
    """Exported declarations of `directory` that no **other** file imports.

    The answer names `file|export`, so one item carries many independent
    decisions rather than a single yes/no.
    """
    rows = set()
    used = imported_names()
    for path in sorted(files_under(directory)):
        taken = used.get(path, frozenset())
        for declaration in _exports_of(path):
            if declaration["name"] not in taken:
                rows.add((path, declaration["name"]))
    return Answer(frozenset(rows))


@cache
def _reachable_from(entries: tuple[str, ...]) -> frozenset[str]:
    """Files reachable by following imports from `entries`, entries included."""
    edges = runtime_edges()
    seen: set[str] = set()
    queue = [entry for entry in entries if entry in parsed()]
    while queue:
        node = queue.pop()
        if node in seen:
            continue
        seen.add(node)
        queue.extend(target for target in edges.get(node, frozenset()) if target not in seen)
    return frozenset(seen)


def files_no_entry_point_reaches(directory: str, entries: tuple[str, ...]) -> Answer:
    """Files of `directory` that no chain of imports from `entries` arrives at.

    The entry points are named in the question — there is no such thing as *the*
    entry point of a library, and inferring one would be the oracle deciding
    something the subject was not told.
    """
    reached = _reachable_from(entries)
    return Answer(frozenset((path,) for path in sorted(files_under(directory) - reached)))


def exports_no_test_imports(directory: str, test_root: str) -> Answer:
    """Exported declarations of `directory` that no file under `test_root` imports.

    Name-based and direct, like `unimported_exports`: what a test *exercises* is
    a question about running it, and this one is about what it names.
    """
    wanted: dict[str, set[str]] = defaultdict(set)
    for path, entry in parsed().items():
        if not path.startswith(f"{test_root}/"):
            continue
        for record in entry["imports"]:
            target = resolve(path, record["specifier"])
            if target is None:
                continue
            for name in record["names"]:
                if name.get("namespace"):
                    wanted[target].update(n["name"] for n in _exports_of(target))
                else:
                    wanted[target].add(name["name"])
    rows = set()
    for path in sorted(files_under(directory)):
        if path.startswith(f"{test_root}/"):
            continue
        taken = wanted.get(path, set())
        for declaration in _exports_of(path):
            if declaration["name"] not in taken:
                rows.add((path, declaration["name"]))
    return Answer(frozenset(rows))
