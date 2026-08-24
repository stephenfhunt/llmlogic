"""Extract the fact tables `static_analysis.dl` imports, from the source tree.

This half of the reference entry is the part the other six domains do not have:
`static_analysis`'s fixture ships **no schemas**, because inventing the fact
table is the task. So the reference program cannot be pinned without also
pinning the extraction it assumes.

Standard library only, and every definition is syntactic — a call is a call
expression, a base class is a name in a base list. That is the domain's own rule
(`domains/static_analysis/truth.py`), and an extractor that resolved anything
would be answering a different question from the oracle.

**`call` carries two columns and no more, and that is the load-bearing choice.**
A natural extractor also records the line, and a relation is a set: with a line
column every call site is a distinct fact, so `count { M | call(name: F, module: M) }`
counts *call sites* instead of *modules* and the answer silently changes — 13
functions instead of 7, exit 0, no warning. See `datalog/ROADMAP.md`,
*Count-distinct, and the invisible wildcard*. Emitting only what the questions
are about is the workaround `datalog/skill/recipes/source-analysis.md` teaches.
"""

from __future__ import annotations

import ast
import json
import pathlib
import sys

PACKAGE = "sqlparse"


def module_name(path: str) -> str:
    """`sqlparse/engine/grouping.py` → `sqlparse.engine.grouping`."""
    return path[: -len(".py")].replace("/", ".").removesuffix(".__init__")


def called_name(node: ast.Call) -> str | None:
    """`f(…)` → `f`, `x.f(…)` → `f`. Anything else names nothing."""
    func = node.func
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        return func.attr
    return None


def write(path: pathlib.Path, rows: list[dict[str, str]]) -> None:
    with path.open("w", encoding="utf-8") as out:
        for row in rows:
            out.write(json.dumps(row) + "\n")


def main(root: pathlib.Path) -> int:
    paths = sorted(p.as_posix() for p in root.rglob("*.py"))
    if not paths:
        print(f"no python under {root}", file=sys.stderr)
        return 1
    trees = {
        module_name(p): ast.parse((root.parent / p).read_text(encoding="utf-8")) for p in paths
    }
    modules = set(trees)

    module_rows = [{"module": m} for m in sorted(trees)]

    fn_def_rows = [
        {"name": node.name, "module": module}
        for module, tree in sorted(trees.items())
        for node in tree.body
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef)
    ]

    call_rows = [
        {"name": name, "module": module}
        for module, tree in sorted(trees.items())
        for node in ast.walk(tree)
        if isinstance(node, ast.Call) and (name := called_name(node))
    ]

    # Three spellings all count, because all three are how this package imports:
    # `import a.b`, `from a.b import name`, and `from a import b` where `a.b` is
    # itself a module — which is why targets are resolved against the modules
    # that exist rather than read off the statement.
    import_rows = []
    for module, tree in sorted(trees.items()):
        targets: set[str] = set()
        for node in ast.walk(tree):
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
        for target in sorted((targets & modules) - {module}):
            import_rows.append({"module": module, "target": target})

    class_base_rows = []
    for _, tree in sorted(trees.items()):
        for node in ast.walk(tree):
            if not isinstance(node, ast.ClassDef):
                continue
            names = set()
            for base in node.bases:
                if isinstance(base, ast.Name):
                    names.add(base.id)
                elif isinstance(base, ast.Attribute):
                    names.add(base.attr)
            for base_name in sorted(names):
                class_base_rows.append({"class": node.name, "base": base_name})

    here = pathlib.Path.cwd()
    write(here / "module.jsonl", module_rows)
    write(here / "fn_def.jsonl", fn_def_rows)
    write(here / "call.jsonl", call_rows)
    write(here / "import_edge.jsonl", import_rows)
    write(here / "class_base.jsonl", class_base_rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else PACKAGE)))
