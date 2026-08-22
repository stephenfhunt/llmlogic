"""Ground truth, in plain Python. Control 1: never the datalog engine.

Everything here is computed with the standard library's own parser over the same
bytes the subject was given. The definitions are *syntactic* and are quoted
verbatim into the questions — a call is a call expression, an import is an import
statement, a base class is a name in a base list. Nothing resolves anything.

That is the design decision the domain rests on. A semantic oracle ("is this
function actually reachable?") would be guessing, and the questions would be
graded against a guess.
"""

from __future__ import annotations

import ast
from collections import defaultdict

from harness.domains.static_analysis.fixture import module_name, sources
from harness.task import Answer

#: A function called from at least this many distinct modules is "widely used".
WIDELY_USED_MODULES = 2


def _trees() -> dict[str, ast.Module]:
    return {module_name(path): ast.parse(text) for path, text in sources().items()}


def called_name(node: ast.Call) -> str | None:
    """The name of the thing being called: `f(…)` → `f`, `x.f(…)` → `f`.

    Anything else — a call on a subscript, on a call's result — names nothing,
    and is left out rather than guessed at.
    """
    func = node.func
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        return func.attr
    return None


def module_level_functions() -> set[str]:
    """Names of `def`s at the top level of a file — not in a class, not nested."""
    names = set()
    for tree in _trees().values():
        for node in tree.body:
            if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                names.add(node.name)
    return names


def calls_by_module() -> dict[str, set[str]]:
    """Called name -> the modules that contain a call to that name."""
    calls: dict[str, set[str]] = defaultdict(set)
    for module, tree in _trees().items():
        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and (name := called_name(node)):
                calls[name].add(module)
    return calls


def never_called_functions() -> Answer:
    calls = calls_by_module()
    return Answer.of(*[name for name in module_level_functions() if not calls[name]])


def functions_called_from_several_modules(minimum: int = WIDELY_USED_MODULES) -> Answer:
    calls = calls_by_module()
    return Answer.of(*[name for name in module_level_functions() if len(calls[name]) >= minimum])


def import_edges() -> dict[str, set[str]]:
    """Module -> the package modules it imports.

    Three spellings all count, because all three are how this package imports:
    `import a.b`, `from a.b import name`, and `from a import b` where `a.b` is
    itself a module. The last one is why the edges have to be resolved against
    the set of modules that exist rather than read off the statement.
    """
    modules = set(_trees())
    edges: dict[str, set[str]] = {module: set() for module in modules}
    for module, tree in _trees().items():
        for node in ast.walk(tree):
            targets = set()
            if isinstance(node, ast.Import):
                targets.update(alias.name for alias in node.names)
            elif isinstance(node, ast.ImportFrom):
                if node.level:
                    package = module.rsplit(".", node.level - 1)[0] if node.level > 1 else module
                    package = package.rsplit(".", 1)[0] if "." in package else package
                    base = f"{package}.{node.module}" if node.module else package
                else:
                    base = node.module or ""
                targets.add(base)
                targets.update(f"{base}.{alias.name}" for alias in node.names)
            edges[module] |= (targets & modules) - {module}
    return edges


def reaches(edges: dict[str, set[str]], start: str) -> set[str]:
    """Every module reachable from ``start`` by following imports."""
    seen: set[str] = set()
    queue = [start]
    while queue:
        current = queue.pop()
        for target in edges.get(current, ()):
            if target not in seen:
                seen.add(target)
                queue.append(target)
    return seen


def modules_in_a_cycle() -> set[str]:
    edges = import_edges()
    return {module for module in edges if module in reaches(edges, module)}


def modules_outside_any_cycle() -> Answer:
    edges = import_edges()
    cyclic = modules_in_a_cycle()
    return Answer.of(*[module for module in edges if module not in cyclic])


def base_names() -> dict[str, set[str]]:
    """Class -> the names in its base list, qualifiers dropped."""
    bases: dict[str, set[str]] = defaultdict(set)
    for tree in _trees().values():
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef):
                for base in node.bases:
                    if isinstance(base, ast.Name):
                        bases[node.name].add(base.id)
                    elif isinstance(base, ast.Attribute):
                        bases[node.name].add(base.attr)
    return bases


def subclasses_of(root: str) -> Answer:
    """Every class below ``root``, transitively — the closure, not the level."""
    bases = base_names()
    below: set[str] = set()
    changed = True
    while changed:
        changed = False
        for name, parents in bases.items():
            if name not in below and (root in parents or parents & below):
                below.add(name)
                changed = True
    return Answer.of(*below)
