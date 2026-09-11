#!/usr/bin/env python3
"""code-facts' Python frontend: a Python codebase as rows of code-facts' schema.

Run by `code-facts` (Node), never alone: it streams one JSON object per line,
`{"relation": ..., "row": {...}}`, and Node validates every row against
`src/schema.ts` — the one home of the schema for every language — before writing
anything. So this file emits the same relations, with the same id scheme, as the
TypeScript frontend, and `lib/` works over either.

Standard library only (`ast`, `tokenize`, `tomllib`), Python >= 3.11.

What it can know without running the code, and how:

- **Names resolve by Python's own scoping** — a function's locals are every name
  it binds (unless declared `global` / `nonlocal`), a class body is invisible to
  its methods, then the module, then builtins — and through imports, including
  re-export chains and `from m import *`, to the module that defines them.
- **Attributes resolve when the receiver's type is known**: a module, a class,
  `self` / `cls`, `super()`, a parameter or local whose annotation names a
  project class, or a local assigned from a project class's constructor.
  Anything else is `dispatch: unresolved`, with the name kept (`callee_name`).
- **Nothing is inferred beyond that.** `symbol_type.text` is an annotation as
  written. An import is never elided (Python has no type erasure) except under
  `if TYPE_CHECKING:`, which is `runtime: false`.
"""

from __future__ import annotations

import argparse
import ast
import builtins
import io
import json
import os
import re
import sys
import tokenize
from dataclasses import dataclass, field

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    tomllib = None  # type: ignore[assignment]

SKIP_DIRS = {
    ".git", "__pycache__", ".venv", "venv", "env", ".tox", ".nox", "node_modules",
    "site-packages", ".mypy_cache", ".pytest_cache", ".ruff_cache", "build", "dist",
}
TEST_PATH = re.compile(r"(^|/)(tests?|testing)/|(^|/)test_[^/]*\.py$|_test\.py$|(^|/)conftest\.py$")
GENERATED = re.compile(r"@generated|auto-?generated|do not edit", re.I)
STDLIB = set(getattr(sys, "stdlib_module_names", ()))
BUILTIN_NAMES = set(dir(builtins))


# ── output ────────────────────────────────────────────────────────────────

_out = []


def emit(relation: str, **row) -> None:
    _out.append(json.dumps({"relation": relation, "row": row}, ensure_ascii=False))
    if len(_out) >= 2000:
        flush()


def flush() -> None:
    if _out:
        sys.stdout.write("\n".join(_out) + "\n")
        _out.clear()


def truncate(s: str, n: int) -> str:
    one = " ".join(s.split())
    return one if len(one) <= n else one[: n - 1] + "…"


def normalize_dist(name: str) -> str:
    """PEP 503: case-folded, runs of -_. become one -."""
    return re.sub(r"[-_.]+", "-", name).lower()


# ── the model ─────────────────────────────────────────────────────────────


@dataclass(eq=False)
class Decl:
    id: str
    name: str
    kind: str
    node: ast.AST | None
    file: "Source"
    parent: "Decl | None"
    line: int
    end_line: int
    exported: bool = False
    visibility: str | None = None
    is_static: bool = False
    is_abstract: bool = False
    is_async: bool = False
    is_generator: bool = False
    is_readonly: bool = False
    is_optional: bool = False
    # scopes
    members: dict = field(default_factory=dict)  # class: name -> Decl
    locals: dict = field(default_factory=dict)  # function: name -> Decl
    globals: set = field(default_factory=set)
    nonlocals: set = field(default_factory=set)
    bases: list = field(default_factory=list)  # class: ast base expressions
    annotation: ast.AST | None = None  # parameter / local annotation
    constructed: ast.AST | None = None  # local assigned from a call: x = Foo()
    is_method: bool = False


@dataclass(eq=False)
class Source:
    path: str  # repo-relative
    abspath: str
    module: str  # dotted
    is_package: bool  # an __init__.py
    tree: ast.Module | None
    text: str
    project: str
    package: str | None
    decl: Decl | None = None  # the <module> symbol
    scope: dict = field(default_factory=dict)  # module scope: name -> Binding
    star_imports: list = field(default_factory=list)  # modules `from m import *` came from
    all_names: list | None = None


# A module-scope binding: a Decl, or an import to be resolved lazily.
@dataclass
class ImportBinding:
    module: str  # absolute dotted module
    name: str | None  # None: the module itself
    line: int
    type_only: bool


# Resolution targets.
@dataclass
class ModuleT:
    module: str


@dataclass
class DeclT:
    decl: Decl


@dataclass
class InstanceT:
    cls: Decl


@dataclass
class SuperT:
    cls: Decl


@dataclass
class ExternalT:
    id: str


# ── the frontend ──────────────────────────────────────────────────────────


class Frontend:
    def __init__(self, root: str, layers: set[str], exclude: list[re.Pattern], call_site: int, flow_node: int):
        self.root = root
        self.layers = layers
        self.exclude = exclude
        self.sources: list[Source] = []
        self.modules: dict[str, Source] = {}
        self.taken: dict[str, object] = {}
        self.decl_of: dict[int, Decl] = {}  # id(ast node) -> Decl
        self.external: dict[str, dict] = {}
        self.mro_cache: dict[int, list[Decl]] = {}
        self.resolving: set[int] = set()  # locals whose constructor is being resolved: `x = x.next()` is circular
        self.next_call_site = call_site
        self.next_flow_node = flow_node
        self.projects: list[tuple[str, str, list[str]]] = []
        self.packages: dict[str, tuple[str, dict]] = {}  # dir -> (name, pyproject)

    # ── discovery ──

    def rel(self, abspath: str) -> str:
        r = os.path.relpath(abspath, self.root).replace(os.sep, "/")
        return "." if r == "." else r

    def discover(self, targets: list[str]) -> None:
        seen: set[str] = set()
        for target in targets:
            tdir = os.path.dirname(os.path.abspath(target)) if os.path.isfile(target) else os.path.abspath(target)
            files: list[str] = []
            for dirpath, dirnames, filenames in os.walk(tdir):
                dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS and not d.startswith("."))
                for fn in sorted(filenames):
                    if not (fn.endswith(".py") or fn.endswith(".pyi")):
                        continue
                    abspath = os.path.join(dirpath, fn)
                    rel = self.rel(abspath)
                    if any(rx.search(rel) for rx in self.exclude) or abspath in seen:
                        continue
                    seen.add(abspath)
                    files.append(abspath)
            project_id = self.rel(target if os.path.isfile(target) else tdir)
            self.projects.append((project_id, self.rel(tdir), [self.rel(f) for f in files]))
            for f in files:
                self.add_source(f, project_id)
        self.sources.sort(key=lambda s: s.path)

    def package_for(self, directory: str) -> str | None:
        d = directory
        while True:
            if d in self.packages:
                return self.packages[d][0]
            pp = os.path.join(d, "pyproject.toml")
            if os.path.isfile(pp) and tomllib is not None:
                try:
                    with open(pp, "rb") as fh:
                        data = tomllib.load(fh)
                except (OSError, ValueError):
                    data = {}
                name = (data.get("project") or {}).get("name") or ((data.get("tool") or {}).get("poetry") or {}).get("name")
                self.packages[d] = (name or self.rel(d), data)
                return self.packages[d][0]
            parent = os.path.dirname(d)
            if parent == d or not (parent + os.sep).startswith(self.root + os.sep) and parent != self.root:
                return None
            d = parent

    def add_source(self, abspath: str, project: str) -> None:
        try:
            with open(abspath, "rb") as fh:
                raw = fh.read()
            text = raw.decode("utf-8", errors="replace")
        except OSError:
            return
        tree: ast.Module | None
        try:
            tree = ast.parse(text, filename=abspath, type_comments=False)
        except SyntaxError as e:
            tree = None
            if "quality" in self.layers:
                emit("diagnostic", file=self.rel(abspath), line=e.lineno, code=0, category="error", message=truncate(f"SyntaxError: {e.msg}", 300))
        # module name: climb while the directory is a package
        d, parts = os.path.dirname(abspath), []
        base = os.path.basename(abspath).rsplit(".", 1)[0]
        is_package = base == "__init__"
        if not is_package:
            parts.append(base)
        while os.path.isfile(os.path.join(d, "__init__.py")) or os.path.isfile(os.path.join(d, "__init__.pyi")):
            parts.append(os.path.basename(d))
            d = os.path.dirname(d)
        module = ".".join(reversed(parts)) or os.path.basename(os.path.dirname(abspath))
        src = Source(self.rel(abspath), abspath, module, is_package, tree, text, project, self.package_for(os.path.dirname(abspath)))
        self.sources.append(src)
        # A .py wins over a .pyi stub of the same module.
        if module not in self.modules or self.modules[module].path.endswith(".pyi"):
            self.modules[module] = src

    # ── ids ──

    def claim(self, candidate: str, key: object, line: int, col: int) -> str:
        for c in (candidate, f"{candidate}@{line}", f"{candidate}@{line}:{col}"):
            holder = self.taken.get(c)
            if holder is None or holder is key:
                self.taken[c] = key
                return c
        n = 2
        while f"{candidate}@{line}:{col}#{n}" in self.taken:
            n += 1
        self.taken[f"{candidate}@{line}:{col}#{n}"] = key
        return f"{candidate}@{line}:{col}#{n}"

    def declare(self, src: Source, parent: Decl | None, name: str, kind: str, node: ast.AST | None, segment: str | None = None) -> Decl:
        seg = segment or name
        base = f"{src.path}#{seg}" if parent is None or parent.kind == "module" else f"{parent.id}.{seg}"
        line = getattr(node, "lineno", 1) if node is not None else 1
        col = getattr(node, "col_offset", 0) + 1 if node is not None else 1
        key = node if node is not None else object()
        did = self.claim(base, key, line, col)
        end = getattr(node, "end_lineno", line) if node is not None else line
        d = Decl(did, name, kind, node, src, parent, line, end or line)
        if node is not None:
            self.decl_of[id(node)] = d
        return d

    # ── pass 1: declarations ──

    def declare_all(self) -> None:
        for src in self.sources:
            src.decl = Decl(self.claim(f"{src.path}#<module>", src, 1, 1), "<module>", "module", src.tree, src, None, 1,
                            len(src.text.splitlines()) or 1)
            if src.tree is not None:
                self.decl_of[id(src.tree)] = src.decl
                self.module_scope(src)

    def module_scope(self, src: Source) -> None:
        tree = src.tree
        assert tree is not None and src.decl is not None
        # __all__ as a literal list or tuple
        for stmt in tree.body:
            if isinstance(stmt, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__all__" for t in stmt.targets):
                if isinstance(stmt.value, (ast.List, ast.Tuple)):
                    src.all_names = [e.value for e in stmt.value.elts if isinstance(e, ast.Constant) and isinstance(e.value, str)]
        self.bind_block(src, src.decl, tree.body, scope="module", type_only=False)

    def bind_block(self, src: Source, owner: Decl, body: list[ast.stmt], scope: str, type_only: bool) -> None:
        """Declares what a block of statements binds in `owner`'s scope, and
        descends into nested scopes."""
        for stmt in body:
            self.bind_stmt(src, owner, stmt, scope, type_only)

    def scope_map(self, owner: Decl, scope: str, src: Source) -> dict:
        return src.scope if scope == "module" else owner.members if scope == "class" else owner.locals

    def bind_name(self, src: Source, owner: Decl, scope: str, name: str, node: ast.AST, kind: str | None = None) -> Decl | None:
        if scope == "function" and (name in owner.globals or name in owner.nonlocals):
            if name in owner.globals and name not in src.scope:
                d = self.declare(src, src.decl, name, "variable", node)
                src.scope[name] = d
            return None
        table = self.scope_map(owner, scope, src)
        existing = table.get(name)
        if isinstance(existing, Decl):
            return existing
        if kind is None:
            kind = "variable" if scope == "module" else "property" if scope == "class" else "local"
        d = self.declare(src, owner, name, kind, node)
        if scope == "class":
            d.visibility = visibility(name)
        table[name] = d
        return d

    def bind_target(self, src: Source, owner: Decl, scope: str, target: ast.AST, value: ast.AST | None = None, annotation: ast.AST | None = None) -> None:
        if isinstance(target, ast.Name):
            d = self.bind_name(src, owner, scope, target.id, target)
            if d is not None:
                if annotation is not None and d.annotation is None:
                    d.annotation = annotation
                if value is not None and isinstance(value, ast.Call) and d.constructed is None:
                    d.constructed = value
                if value is not None and isinstance(value, ast.Lambda) and id(value) not in self.decl_of:
                    self.decl_of[id(value)] = d  # `f = lambda: …` is `f`
                    d.kind = "function" if scope != "class" else "method"
                    self.parameters(src, d, value.args)
        elif isinstance(target, (ast.Tuple, ast.List)):
            for e in target.elts:
                self.bind_target(src, owner, scope, e)
        elif isinstance(target, ast.Starred):
            self.bind_target(src, owner, scope, target.value)

    def bind_stmt(self, src: Source, owner: Decl, stmt: ast.stmt, scope: str, type_only: bool) -> None:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            self.function(src, owner, stmt, scope)
        elif isinstance(stmt, ast.ClassDef):
            self.klass(src, owner, stmt, scope)
        elif isinstance(stmt, ast.Assign):
            for t in stmt.targets:
                self.bind_target(src, owner, scope, t, stmt.value)
        elif isinstance(stmt, ast.AnnAssign):
            self.bind_target(src, owner, scope, stmt.target, stmt.value, stmt.annotation)
        elif isinstance(stmt, ast.AugAssign):
            self.bind_target(src, owner, scope, stmt.target)
        elif isinstance(stmt, (ast.For, ast.AsyncFor)):
            self.bind_target(src, owner, scope, stmt.target)
            self.bind_block(src, owner, stmt.body + stmt.orelse, scope, type_only)
        elif isinstance(stmt, (ast.With, ast.AsyncWith)):
            for item in stmt.items:
                if item.optional_vars is not None:
                    self.bind_target(src, owner, scope, item.optional_vars, item.context_expr)
            self.bind_block(src, owner, stmt.body, scope, type_only)
        elif isinstance(stmt, ast.If):
            guarded = type_only or is_type_checking(stmt.test)
            self.bind_block(src, owner, stmt.body, scope, guarded)
            self.bind_block(src, owner, stmt.orelse, scope, type_only)
        elif isinstance(stmt, ast.While):
            self.bind_block(src, owner, stmt.body + stmt.orelse, scope, type_only)
        elif isinstance(stmt, (ast.Try, getattr(ast, "TryStar", ast.Try))):
            self.bind_block(src, owner, stmt.body, scope, type_only)
            for h in stmt.handlers:
                if h.name:
                    self.bind_name(src, owner, scope, h.name, h)
                self.bind_block(src, owner, h.body, scope, type_only)
            self.bind_block(src, owner, stmt.orelse + stmt.finalbody, scope, type_only)
        elif isinstance(stmt, ast.Match):
            for case in stmt.cases:
                for n in ast.walk(case.pattern):
                    name = getattr(n, "name", None) if isinstance(n, (ast.MatchAs, ast.MatchStar)) else None
                    if name:
                        self.bind_name(src, owner, scope, name, n)
                    if isinstance(n, ast.MatchMapping) and n.rest:
                        self.bind_name(src, owner, scope, n.rest, n)
                self.bind_block(src, owner, case.body, scope, type_only)
        elif isinstance(stmt, (ast.Import, ast.ImportFrom)):
            self.bind_import(src, owner, scope, stmt, type_only)
        elif isinstance(stmt, ast.Global):
            owner.globals.update(stmt.names)
        elif isinstance(stmt, ast.Nonlocal):
            owner.nonlocals.update(stmt.names)
        # Walrus targets and comprehension variables bind in the enclosing
        # function (comprehension scopes are folded into it — an approximation).
        for n in ast.walk(stmt) if not isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) else ():
            if isinstance(n, ast.NamedExpr):
                self.bind_target(src, owner, scope if scope != "class" else "class", n.target, n.value)
            elif isinstance(n, ast.comprehension):
                self.bind_target(src, owner, "function" if scope == "function" else scope if scope != "class" else "class", n.target)
            elif isinstance(n, ast.Lambda) and id(n) not in self.decl_of:
                self.lambda_(src, owner, n)

    def absolute(self, src: Source, level: int, module: str | None) -> str:
        if level == 0:
            return module or ""
        pkg_parts = src.module.split(".") if src.is_package else src.module.split(".")[:-1]
        if level > 1:
            pkg_parts = pkg_parts[: len(pkg_parts) - (level - 1)]
        base = ".".join(pkg_parts)
        return f"{base}.{module}" if module and base else (module or base)

    def bind_import(self, src: Source, owner: Decl, scope: str, stmt: ast.stmt, type_only: bool) -> None:
        table = self.scope_map(owner, scope, src)
        if isinstance(stmt, ast.Import):
            for alias in stmt.names:
                if alias.asname:
                    table[alias.asname] = ImportBinding(alias.name, None, stmt.lineno, type_only)
                else:
                    top = alias.name.split(".")[0]
                    table.setdefault(top, ImportBinding(top, None, stmt.lineno, type_only))
        elif isinstance(stmt, ast.ImportFrom):
            base = self.absolute(src, stmt.level, stmt.module)
            for alias in stmt.names:
                if alias.name == "*":
                    if scope == "module":
                        src.star_imports.append(base)
                    continue
                table[alias.asname or alias.name] = ImportBinding(base, alias.name, stmt.lineno, type_only)

    def function(self, src: Source, owner: Decl, node: ast.FunctionDef | ast.AsyncFunctionDef, scope: str) -> None:
        in_class = scope == "class"
        decos = [decorator_name(d) for d in node.decorator_list]
        kind = "function"
        if in_class:
            kind = "method"
            if node.name == "__init__":
                kind = "constructor"
            elif "property" in decos or "cached_property" in decos:
                kind = "getter"
            elif any(d.endswith(".setter") or d.endswith(".deleter") for d in decos):
                kind = "setter"
        table = self.scope_map(owner, scope, src)
        prior = table.get(node.name)
        # A redefinition (a property's setter, a conditional def) is a second
        # function under the same name: the name keeps the first.
        d = self.declare(src, owner, node.name, kind, node)
        if not isinstance(prior, Decl):
            table[node.name] = d
        d.is_method = in_class
        d.is_async = isinstance(node, ast.AsyncFunctionDef)
        d.is_static = "staticmethod" in decos or "classmethod" in decos
        d.is_abstract = "abstractmethod" in decos or "abc.abstractmethod" in decos
        d.is_generator = any(isinstance(n, (ast.Yield, ast.YieldFrom)) for n in walk_own(node))
        d.visibility = visibility(node.name) if in_class else None
        d.exported = scope == "module" and is_public(src, node.name)
        d.decorators = decos  # type: ignore[attr-defined]
        self.parameters(src, d, node.args)
        # names bound in the body: collect global/nonlocal first
        for n in walk_own(node):
            if isinstance(n, ast.Global):
                d.globals.update(n.names)
            elif isinstance(n, ast.Nonlocal):
                d.nonlocals.update(n.names)
        self.bind_block(src, d, node.body, "function", False)
        if in_class:
            self.instance_attributes(src, owner, d, node)

    def lambda_(self, src: Source, owner: Decl, node: ast.Lambda) -> None:
        d = self.declare(src, owner, "<lambda>", "function", node, f"<lambda@{node.lineno}:{node.col_offset + 1}>")
        self.parameters(src, d, node.args)

    def parameters(self, src: Source, fn: Decl, args: ast.arguments) -> None:
        params = [*args.posonlyargs, *args.args]
        defaults = [None] * (len(params) - len(args.defaults)) + list(args.defaults)
        ordered: list[tuple[ast.arg, bool, bool]] = [(p, dflt is not None, False) for p, dflt in zip(params, defaults)]
        if args.vararg:
            ordered.append((args.vararg, False, True))
        for p, dflt in zip(args.kwonlyargs, args.kw_defaults):
            ordered.append((p, dflt is not None, False))
        if args.kwarg:
            ordered.append((args.kwarg, False, True))
        fn.params = []  # type: ignore[attr-defined]
        for p, has_default, rest in ordered:
            pd = self.declare(src, fn, p.arg, "parameter", p)
            pd.annotation = p.annotation
            pd.is_optional = has_default
            fn.locals[p.arg] = pd
            fn.params.append((pd, has_default, rest))  # type: ignore[attr-defined]

    def klass(self, src: Source, owner: Decl, node: ast.ClassDef, scope: str) -> None:
        d = self.bind_name(src, owner, scope, node.name, node, "class")
        if d is None:
            return
        self.decl_of[id(node)] = d
        d.bases = list(node.bases)
        d.exported = scope == "module" and is_public(src, node.name)
        d.visibility = visibility(node.name) if scope == "class" else None
        d.decorators = [decorator_name(x) for x in node.decorator_list]  # type: ignore[attr-defined]
        self.bind_block(src, d, node.body, "class", False)
        d.is_abstract = any(isinstance(m, Decl) and m.is_abstract for m in d.members.values()) or any(
            base_text(b) in ("ABC", "abc.ABC", "Protocol", "typing.Protocol") for b in node.bases)

    def instance_attributes(self, src: Source, cls: Decl, method: Decl, node: ast.AST) -> None:
        """`self.x = …` in a method declares `x` on the class."""
        first = method.params[0][0].name if getattr(method, "params", None) else None  # type: ignore[attr-defined]
        if first is None or method.is_static:
            return
        for n in walk_own(node):
            targets: list[ast.AST] = []
            if isinstance(n, ast.Assign):
                targets = list(n.targets)
            elif isinstance(n, (ast.AnnAssign, ast.AugAssign)):
                targets = [n.target]
            for t in targets:
                for sub in ast.walk(t):
                    if isinstance(sub, ast.Attribute) and isinstance(sub.value, ast.Name) and sub.value.id == first:
                        if sub.attr not in cls.members:
                            a = self.declare(src, cls, sub.attr, "property", sub)
                            a.visibility = visibility(sub.attr)
                            if isinstance(n, ast.AnnAssign):
                                a.annotation = n.annotation
                            cls.members[sub.attr] = a

    # ── resolution ──

    def module_member(self, module: str, name: str, seen: set | None = None):
        """What `module.name` is: a submodule, a declaration, or an external."""
        seen = seen if seen is not None else set()
        if (module, name) in seen:
            return None
        seen.add((module, name))
        sub = f"{module}.{name}" if module else name
        src = self.modules.get(module)
        if src is not None:
            b = src.scope.get(name)
            if isinstance(b, Decl):
                return DeclT(b)
            if isinstance(b, ImportBinding):
                # `from pkg import sub` inside pkg/__init__.py binds pkg.sub to
                # itself: the cycle comes back empty, and the submodule answers.
                r = self.resolve_binding(b, seen)
                if r is not None:
                    return r
            if sub in self.modules:
                return ModuleT(sub)
            for star in src.star_imports:
                r = self.module_member(star, name, seen)
                if r is not None:
                    return r
            return None
        if sub in self.modules:
            return ModuleT(sub)
        if module in self.modules:
            return None
        return self.external_target(module, name)

    def resolve_binding(self, b: ImportBinding, seen: set | None = None):
        if b.name is None:
            return ModuleT(b.module) if b.module in self.modules or any(m.startswith(b.module + ".") for m in self.modules) else self.external_target(b.module, None)
        return self.module_member(b.module, b.name, seen)

    def external_target(self, module: str, name: str | None):
        top = module.split(".")[0] if module else (name or "")
        rest = ".".join([*module.split(".")[1:], *([name] if name else [])]) if module else ""
        eid = f"ext:{top}#{rest or '<module>'}"
        if eid not in self.external:
            self.external[eid] = {"name": (name or module.split(".")[-1]), "package": top}
        return ExternalT(eid)

    def builtin_target(self, name: str):
        eid = f"lib#{name}"
        if eid not in self.external:
            obj = getattr(builtins, name, None)
            kind = "class" if isinstance(obj, type) else "function" if callable(obj) else "variable"
            self.external[eid] = {"name": name, "package": None, "origin": "lib", "kind": kind}
        return ExternalT(eid)

    def enclosing_class(self, fn: Decl | None) -> Decl | None:
        return fn.parent if fn is not None and fn.is_method and fn.parent is not None and fn.parent.kind == "class" else None

    def lookup(self, src: Source, scope_decl: Decl, name: str):
        """LEGB from `scope_decl` (a function, a class or the module)."""
        d: Decl | None = scope_decl
        first = True
        while d is not None and d.kind != "module":
            if d.kind == "class":
                if first and name in d.members:
                    return target_of(d.members[name])
            elif d.kind in FN_KINDS:
                if name in d.globals:
                    break
                if name in d.locals:
                    return target_of(d.locals[name])
            first = False
            d = d.parent
        b = src.scope.get(name)
        if isinstance(b, Decl):
            return DeclT(b)
        if isinstance(b, ImportBinding):
            return self.resolve_binding(b)
        for star in src.star_imports:
            r = self.module_member(star, name)
            if r is not None:
                return r
        if name in BUILTIN_NAMES:
            return self.builtin_target(name)
        return None

    def class_bases(self, c: Decl) -> list[Decl]:
        out = []
        for b in c.bases:
            t = self.resolve_expr(c.file, c.parent or c.file.decl, b)
            if isinstance(t, DeclT) and t.decl.kind == "class" and t.decl not in out:
                out.append(t.decl)
        return out

    def mro(self, cls: Decl, active: frozenset = frozenset()) -> list[Decl]:
        """Python's C3 linearization over the project's classes (external bases
        drop out). Where C3 fails — Python would raise TypeError — or the bases
        are circular, the order is depth-first, left to right."""
        key = id(cls)
        if key in self.mro_cache:
            return self.mro_cache[key]
        if key in active:
            return [cls]
        bases = [b for b in self.class_bases(cls) if id(b) not in active]
        seqs = [list(self.mro(b, active | {key})) for b in bases] + [list(bases)]
        order = [cls]
        while any(seqs):
            head = next((s[0] for s in seqs if s and not any(s[0] in t[1:] for t in seqs)), None)
            if head is None:  # inconsistent: fall back to depth-first
                for s in seqs:
                    order.extend(c for c in s if c not in order)
                break
            order.append(head)
            seqs = [s[1:] if s and s[0] is head else s for s in seqs]
        if not active:
            self.mro_cache[key] = order
        return order

    def member(self, cls: Decl, name: str, skip_self: bool = False):
        for c in self.mro(cls)[1 if skip_self else 0:]:
            if name in c.members:
                return c.members[name]
        return None

    def class_of_annotation(self, src: Source, scope: Decl, ann: ast.AST | None) -> Decl | None:
        if ann is None:
            return None
        if isinstance(ann, ast.Constant) and isinstance(ann.value, str):
            try:
                ann = ast.parse(ann.value, mode="eval").body
            except SyntaxError:
                return None
        # Optional[X] / X | None → X
        if isinstance(ann, ast.BinOp) and isinstance(ann.op, ast.BitOr):
            for side in (ann.left, ann.right):
                c = self.class_of_annotation(src, scope, side)
                if c is not None:
                    return c
            return None
        if isinstance(ann, ast.Subscript) and base_text(ann.value) in ("Optional", "typing.Optional"):
            return self.class_of_annotation(src, scope, ann.slice)
        t = self.resolve_expr(src, scope, ann)
        return t.decl if isinstance(t, DeclT) and t.decl.kind == "class" else None

    def resolve_expr(self, src: Source, scope: Decl | None, e: ast.AST):
        scope = scope or src.decl
        assert scope is not None
        if isinstance(e, ast.Name):
            t = self.lookup(src, scope, e.id)
            if isinstance(t, DeclT):
                d = t.decl
                fn = scope if scope.kind in FN_KINDS else None
                if d.kind == "parameter" and fn is not None and getattr(fn, "params", None) and fn.params[0][0] is d and fn.is_method:  # type: ignore[attr-defined]
                    cls = self.enclosing_class(fn)
                    if cls is not None:
                        return DeclT(cls) if "classmethod" in getattr(fn, "decorators", []) else InstanceT(cls) if not fn.is_static else t
                if d.kind in ("parameter", "local", "variable", "property"):
                    c = self.class_of_annotation(src, d.parent or scope, d.annotation)
                    if c is None and d.constructed is not None and id(d) not in self.resolving:
                        self.resolving.add(id(d))
                        try:
                            k = self.resolve_expr(src, d.parent or scope, d.constructed.func)
                        finally:
                            self.resolving.discard(id(d))
                        c = k.decl if isinstance(k, DeclT) and k.decl.kind == "class" else None
                    if c is not None:
                        return InstanceT(c)
            return t
        if isinstance(e, ast.Attribute):
            base = self.resolve_expr(src, scope, e.value)
            return self.attribute(base, e.attr)
        if isinstance(e, ast.Call):
            if isinstance(e.func, ast.Name) and e.func.id == "super":
                cls = self.enclosing_class(scope if scope.kind in FN_KINDS else None)
                return SuperT(cls) if cls is not None else None
            f = self.resolve_expr(src, scope, e.func)
            if isinstance(f, DeclT) and f.decl.kind == "class":
                return InstanceT(f.decl)
            return None
        if isinstance(e, ast.Subscript):
            return self.resolve_expr(src, scope, e.value)
        return None

    def attribute(self, base, attr: str):
        if isinstance(base, ModuleT):
            return self.module_member(base.module, attr)
        if isinstance(base, (DeclT, InstanceT)):
            cls = base.decl if isinstance(base, DeclT) else base.cls
            if cls.kind != "class":
                return None
            m = self.member(cls, attr)
            return DeclT(m) if m is not None else None
        if isinstance(base, SuperT):
            m = self.member(base.cls, attr, skip_self=True)
            return DeclT(m) if m is not None else None
        if isinstance(base, ExternalT):
            # `ext:os#<module>` . path → `ext:os#path`; `ext:os#path` . join → `ext:os#path.join`
            eid = f"{base.id[: -len('<module>')]}{attr}" if base.id.endswith("#<module>") else f"{base.id}.{attr}"
            if eid not in self.external:
                self.external[eid] = {"name": attr, "package": base.id.split("#")[0][4:] if base.id.startswith("ext:") else None,
                                      "origin": "lib" if base.id.startswith("lib#") else "external"}
            return ExternalT(eid)
        return None

    # ── emission ──

    def emit_structure(self) -> None:
        for project_id, pdir, files in self.projects:
            emit("project", id=project_id, dir=pdir, files=len(files), strict=False, module=None, target=None)
            for f in files:
                emit("project_file", project=project_id, file=f)
        projects_of: dict[str, list[str]] = {}
        for pid, _, files in self.projects:
            for f in files:
                projects_of.setdefault(f, []).append(pid)
        for src in self.sources:
            emit("file", path=src.path, dir=src.path.rsplit("/", 1)[0] if "/" in src.path else ".", package=src.package,
                 lang="pyi" if src.path.endswith(".pyi") else "py", loc=src.text.count("\n") + (0 if src.text.endswith("\n") else 1),
                 sloc=sloc(src.text), is_test=bool(TEST_PATH.search(src.path)), is_decl=src.path.endswith(".pyi"),
                 is_generated=bool(GENERATED.search(src.text[:600])))
        for d, (name, data) in sorted(self.packages.items()):
            proj = data.get("project") or {}
            emit("package", name=name, dir=self.rel(d), version=proj.get("version"), private=False)
            for kind, deps in dependency_blocks(data):
                for spec in deps:
                    dep = requirement_name(spec)
                    if dep:
                        emit("package_dep", package=name, dep=normalize_dist(dep), kind=kind, range=spec,
                             types_for=normalize_dist(dep[len("types-"):]) if normalize_dist(dep).startswith("types-") else None)
        for src in self.sources:
            if src.tree is None:
                continue
            self.emit_imports(src)
            self.emit_exports(src)
        for src in self.sources:
            if src.tree is not None:
                self.emit_declaration_detail(src)

    def emit_imports(self, src: Source) -> None:
        def row(node, spec: str, kind: str, runtime: bool, module: str | None):
            target = self.modules.get(module) if module else None
            if target is src:
                return  # `from . import sub` in a package's __init__: the package is already running
            top = (module or spec).lstrip(".").split(".")[0]
            builtin = target is None and top in STDLIB
            emit("imports", file=src.path, line=node.lineno, specifier=spec, kind=kind, runtime=runtime,
                 builtin=builtin, target_file=target.path if target else None,
                 target_package=None if target else (top if builtin else normalize_dist(top)) if top else None,
                 resolved=target is not None or builtin)

        def name(node, local: str, imported: str, t, type_only: bool):
            emit("import_name", file=src.path, line=node.lineno, local=local, imported=imported,
                 target=self.target_id(t), type_only=type_only)

        for n, guarded in walk_with_guard(src.tree):
            if isinstance(n, ast.Import):
                for alias in n.names:
                    module = alias.name if alias.name in self.modules else None
                    row(n, alias.name, "type_only" if guarded else "static", not guarded, module)
                    # `import a.b.c` binds `a`, the top package; `import a.b.c as x` binds x to a.b.c.
                    bound = alias.name if alias.asname else alias.name.split(".")[0]
                    t = self.resolve_binding(ImportBinding(bound, None, n.lineno, guarded))
                    name(n, alias.asname or bound, "*", t, guarded)
            elif isinstance(n, ast.ImportFrom):
                base = self.absolute(src, n.level, n.module)
                spec = "." * n.level + (n.module or "")
                kind = "type_only" if guarded else "reexport_all" if any(a.name == "*" for a in n.names) else "static"
                row(n, spec, kind, not guarded, base if base in self.modules else None)
                for alias in n.names:
                    if alias.name == "*":
                        continue
                    sub = f"{base}.{alias.name}"
                    if sub in self.modules:
                        # `from pkg import mod` also depends on the submodule itself.
                        row(n, f"{spec}.{alias.name}" if spec.strip(".") else spec + alias.name, "type_only" if guarded else "static", not guarded, sub)
                    t = self.module_member(base, alias.name)
                    name(n, alias.asname or alias.name, alias.name, t, guarded)
            elif isinstance(n, ast.Call) and n.args and isinstance(n.args[0], ast.Constant) and isinstance(n.args[0].value, str):
                fn = n.func
                callee = fn.id if isinstance(fn, ast.Name) else fn.attr if isinstance(fn, ast.Attribute) else None
                if callee in ("import_module", "__import__"):
                    spec = n.args[0].value
                    row(n, spec, "dynamic", True, spec if spec in self.modules else None)

    def emit_exports(self, src: Source) -> None:
        names = src.all_names if src.all_names is not None else [
            k for k, v in src.scope.items() if isinstance(v, Decl) and not k.startswith("_") and v.kind != "local"]
        for n in names:
            b = src.scope.get(n)
            t = DeclT(b) if isinstance(b, Decl) else self.resolve_binding(b) if isinstance(b, ImportBinding) else self.module_member(src.module, n)
            local = isinstance(b, Decl)
            emit("exports", file=src.path, name=n, symbol=self.target_id(t), kind="local" if local else "reexport", is_type=False)

    def emit_declaration_detail(self, src: Source) -> None:
        for node in ast.walk(src.tree):
            d = self.decl_of.get(id(node))
            if d is None or d.file is not src:
                continue
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)) and d.node is node:
                for i, (p, has_default, rest) in enumerate(getattr(d, "params", [])):
                    emit("param", fn=d.id, index=i, symbol=p.id, name=p.name, optional=has_default, rest=rest, has_default=has_default)
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and d.node is node:
                parent_kind = d.parent.kind if d.parent else "module"
                if parent_kind in ("module", "class"):
                    doc = ast.get_docstring(node, clean=False)
                    emit("doc", symbol=d.id, has_doc=doc is not None, lines=(doc.count("\n") + 1) if doc else 0)
                for dec in node.decorator_list:
                    callee = dec.func if isinstance(dec, ast.Call) else dec
                    emit("decorator", target=d.id, decorator=self.target_id(self.resolve_expr(src, d.parent, callee)),
                         name=truncate(ast.unparse(callee), 100), line=dec.lineno)
        if isinstance(src.tree, ast.Module):
            doc = ast.get_docstring(src.tree, clean=False)
            emit("doc", symbol=src.decl.id, has_doc=doc is not None, lines=(doc.count("\n") + 1) if doc else 0)

    def target_id(self, t) -> str | None:
        if isinstance(t, DeclT):
            return t.decl.id
        if isinstance(t, InstanceT):
            return t.cls.id
        if isinstance(t, ModuleT):
            src = self.modules.get(t.module)
            if src is not None and src.decl is not None:
                return src.decl.id
            if any(m.startswith(t.module + ".") for m in self.modules):
                return None  # a namespace package of the project's: no file, so no symbol
            return self.external_target(t.module, None).id
        if isinstance(t, ExternalT):
            return t.id
        return None

    # ── refs ──

    def emit_refs(self) -> None:
        for src in self.sources:
            if src.tree is not None:
                RefWalker(self, src).visit(src.tree)
        for src in self.sources:
            for d in self.all_decls(src):
                if d.kind == "class":
                    self.emit_inheritance(src, d)

    def emit_inheritance(self, src: Source, cls: Decl) -> None:
        bases: list[Decl] = []
        for b in cls.bases:
            t = self.resolve_expr(src, cls.parent or src.decl, b)
            tid = self.target_id(t)
            if tid is not None:
                emit("extends", child=cls.id, parent=tid)
            if isinstance(t, DeclT) and t.decl.kind == "class":
                bases.append(t.decl)
        for name, m in cls.members.items():
            if not isinstance(m, Decl) or m.kind == "constructor" or name.startswith("__") and not name.endswith("__"):
                continue
            for b in bases:
                over = self.member(b, name)
                if over is not None and over is not m:
                    emit("overrides", member=m.id, base=over.id)

    def all_decls(self, src: Source):
        seen = set()
        for d in self.decl_of.values():
            if d.file is src and id(d) not in seen:
                seen.add(id(d))
                yield d

    def emit_symbols(self) -> None:
        emitted: set[str] = set()
        for src in self.sources:
            if src.decl is None:
                continue
            decls = sorted({id(d): d for d in self.all_decls(src)}.values(), key=lambda d: (d.line, d.id))
            if src.decl not in decls:
                decls.insert(0, src.decl)
            for d in decls:
                if d.id in emitted:
                    continue
                emitted.add(d.id)
                emit("symbol", id=d.id, name=d.name, kind=d.kind, origin="project", file=src.path, line=d.line,
                     end_line=d.end_line, parent=d.parent.id if d.parent else None, package=src.package,
                     exported=d.exported or (d.parent is src.decl and d.kind in ("variable", "function", "class") and is_public(src, d.name)),
                     visibility=d.visibility, is_static=d.is_static, is_abstract=d.is_abstract, is_async=d.is_async,
                     is_generator=d.is_generator, is_readonly=d.is_readonly, is_optional=d.is_optional,
                     is_ambient=src.path.endswith(".pyi"))
        for eid, info in sorted(self.external.items()):
            if eid in emitted:
                continue
            emit("symbol", id=eid, name=info["name"], kind=info.get("kind", "unknown"), origin=info.get("origin", "external"),
                 file=None, line=None, end_line=None, parent=None, package=info.get("package"), exported=False,
                 visibility=None, is_static=False, is_abstract=False, is_async=False, is_generator=False,
                 is_readonly=False, is_optional=False, is_ambient=True)


FN_KINDS = {"function", "method", "constructor", "getter", "setter"}
SKIPPED_TARGETS = {"local", "parameter"}


def target_of(d):
    return DeclT(d) if isinstance(d, Decl) else d


class RefWalker(ast.NodeVisitor):
    """Emits ref, call_site, member_access, type_ref, symbol_type, unresolved_ref."""

    def __init__(self, fe: Frontend, src: Source):
        self.fe = fe
        self.src = src
        self.owner: list[Decl] = [src.decl]  # innermost named declaration (ref.from)
        self.scope: list[Decl] = [src.decl]  # innermost scope for name lookup

    def visit_FunctionDef(self, node):
        d = self.fe.decl_of.get(id(node))
        for dec in node.decorator_list:
            self.expr(dec, role="decorator")
        self.annotations(node, d)
        for dflt in [*node.args.defaults, *[x for x in node.args.kw_defaults if x is not None]]:
            self.visit(dflt)
        if d is None:
            return
        self.symbol_types(d)
        self.owner.append(d)
        self.scope.append(d)
        for stmt in node.body:
            self.visit(stmt)
        self.scope.pop()
        self.owner.pop()

    visit_AsyncFunctionDef = visit_FunctionDef

    def visit_Lambda(self, node):
        d = self.fe.decl_of.get(id(node))
        for dflt in node.args.defaults:
            self.visit(dflt)
        if d is None or d.node is not node:
            # `f = lambda: …` adopts `f`: its body is f's
            self.scope.append(d) if d is not None else None
            self.visit(node.body)
            self.scope.pop() if d is not None else None
            return
        self.owner.append(d)
        self.scope.append(d)
        self.visit(node.body)
        self.scope.pop()
        self.owner.pop()

    def visit_ClassDef(self, node):
        d = self.fe.decl_of.get(id(node))
        for dec in node.decorator_list:
            self.expr(dec, role="decorator")
        for b in node.bases:
            self.expr(b, role="extends")
        for k in node.keywords:
            self.visit(k.value)
        if d is None:
            return
        self.owner.append(d)
        self.scope.append(d)
        for stmt in node.body:
            self.visit(stmt)
        self.scope.pop()
        self.owner.pop()

    def visit_Assign(self, node):
        top = len(self.scope) == 1 or self.scope[-1].kind == "class"
        target_decls = []
        if top:
            for t in node.targets:
                if isinstance(t, ast.Name):
                    table = self.src.scope if self.scope[-1].kind == "module" else self.scope[-1].members
                    d = table.get(t.id)
                    if isinstance(d, Decl) and d.node is t:
                        target_decls.append(d)
        if len(target_decls) == 1:
            # a module-level or class-level variable owns its initializer's references
            self.owner.append(target_decls[0])
            self.visit(node.value)
            self.owner.pop()
            for t in node.targets:
                self.visit(t)
            return
        self.generic_visit(node)

    def visit_AnnAssign(self, node):
        d = None
        if isinstance(node.target, ast.Name):
            s = self.scope[-1]
            table = self.src.scope if s.kind == "module" else s.members if s.kind == "class" else s.locals
            d = table.get(node.target.id)
            d = d if isinstance(d, Decl) else None
        self.annotation(node.annotation, "variable" if not (self.scope[-1].kind == "class") else "property", d)
        if d is not None and d.annotation is node.annotation:
            self.emit_symbol_type(d, node.annotation)
        if node.value is not None:
            self.visit(node.value)
        self.visit(node.target)

    # names and attributes

    def visit_Name(self, node):
        self.expr(node)

    def visit_Attribute(self, node):
        self.expr(node)

    def visit_Call(self, node):
        self.call(node)

    def expr(self, node, role: str | None = None):
        """A name or attribute chain in an expression."""
        if isinstance(node, ast.Call):
            self.call(node, role)
            return
        if not isinstance(node, (ast.Name, ast.Attribute)):
            self.visit(node)
            return
        if isinstance(node, ast.Attribute):
            # the receiver first, as a plain read
            self.expr(node.value)
        target = self.fe.resolve_expr(self.src, self.scope[-1], node)
        self.reference(node, target, role)

    def reference(self, node, target, role: str | None, called: bool = False):
        src, fe = self.src, self.fe
        frm = self.owner[-1].id
        line = node.lineno
        if target is None:
            if isinstance(node, ast.Name):
                if not self.is_bound_somewhere(node.id):
                    emit("unresolved_ref", **{"from": frm}, name=node.id, kind="call" if called else "read", file=src.path, line=line)
            return
        if isinstance(target, InstanceT):
            return  # `self`, or a typed receiver: the variable, not a symbol
        tid = fe.target_id(target)
        if tid is None:
            return
        if isinstance(target, DeclT) and target.decl.kind in SKIPPED_TARGETS:
            return
        ctx = getattr(node, "ctx", None)
        if role in ("extends", "decorator"):
            kind = role
        elif called:
            kind = "new" if isinstance(target, DeclT) and target.decl.kind == "class" else "call"
        elif isinstance(ctx, ast.Store):
            if isinstance(node, ast.Name) and isinstance(target, DeclT) and target.decl.node is node:
                return  # the declaration itself
            kind = "write"
        elif isinstance(ctx, ast.Del):
            kind = "write"
        elif role == "type":
            kind = "type"
        else:
            k = target.decl.kind if isinstance(target, DeclT) else fe.external.get(tid, {}).get("kind")
            kind = "value" if k in ("function", "method", "class") else "read"
        if getattr(node, "_augmented", False):
            kind = "readwrite"
        emit("ref", **{"from": frm}, to=tid, kind=kind, file=src.path, line=line)
        if isinstance(target, DeclT) and isinstance(node, ast.Attribute):
            m = target.decl
            if m.parent is not None and m.parent.kind == "class" and m.kind in ("property", "method", "getter", "setter", "constructor") and role != "type":
                via_this = isinstance(node.value, ast.Name) and node.value.id in ("self", "cls") or (
                    isinstance(node.value, ast.Call) and isinstance(node.value.func, ast.Name) and node.value.func.id == "super")
                mode = "call" if called else kind if kind in ("write", "readwrite") else "read"
                emit("member_access", fn=frm, member=m.id, owner=m.parent.id, mode=mode, via_this=via_this, file=src.path, line=line)

    def is_bound_somewhere(self, name: str) -> bool:
        return name in ("__file__", "__name__", "__doc__", "__spec__", "__path__", "__builtins__", "__loader__", "__package__", "__class__")

    def visit_AugAssign(self, node):
        node.target._augmented = True  # type: ignore[attr-defined]
        self.expr(node.target)
        self.visit(node.value)

    def call(self, node: ast.Call, role: str | None = None):
        fe, src = self.fe, self.src
        cs = fe.next_call_site
        fe.next_call_site += 1
        fn = node.func
        if isinstance(fn, ast.Attribute):
            self.expr(fn.value)
        target = fe.resolve_expr(src, self.scope[-1], fn)
        if isinstance(fn, (ast.Name, ast.Attribute)):
            self.reference(fn, target, "decorator" if role == "decorator" else None, called=True)
        else:
            self.visit(fn)
        callee_name = fn.id if isinstance(fn, ast.Name) else fn.attr if isinstance(fn, ast.Attribute) else None
        callee, dispatch, kind = None, "unresolved", "decorator" if role == "decorator" else "call"
        if isinstance(target, DeclT):
            d = target.decl
            callee = d.id
            if d.kind == "class":
                dispatch = "static"
                kind = "new" if role != "decorator" else kind
            elif d.kind in ("local", "parameter"):
                dispatch = "indirect"
            elif d.is_method and not d.is_static and isinstance(fn, ast.Attribute):
                receiver = fe.resolve_expr(src, self.scope[-1], fn.value)
                dispatch = "static" if isinstance(receiver, (DeclT, SuperT)) else "virtual"
            else:
                dispatch = "static"
        elif isinstance(target, ExternalT):
            callee, dispatch = target.id, "static"
        parent = getattr(node, "_parent", None)
        emit("call_site", id=cs, caller=self.owner[-1].id, callee=callee, callee_name=callee_name, dispatch=dispatch,
             kind=kind, file=src.path, line=node.lineno, col=node.col_offset + 1, args=len(node.args) + len(node.keywords),
             awaited=isinstance(parent, ast.Await), optional=False,
             spread=any(isinstance(a, ast.Starred) for a in node.args) or any(k.arg is None for k in node.keywords))
        node._call_site = cs  # type: ignore[attr-defined]
        for a in node.args:
            self.visit(a)
        for k in node.keywords:
            self.visit(k.value)

    def visit_Await(self, node):
        node.value._parent = node  # type: ignore[attr-defined]
        self.visit(node.value)

    # annotations

    def annotations(self, node, fn: Decl | None):
        for p in [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs, node.args.vararg, node.args.kwarg]:
            if p is not None and p.annotation is not None:
                self.annotation(p.annotation, "param", fn, scope_decl=self.scope[-1])
        if node.returns is not None:
            self.annotation(node.returns, "return", fn, scope_decl=self.scope[-1])

    def annotation(self, ann: ast.AST, position: str, owner: Decl | None, scope_decl: Decl | None = None):
        if isinstance(ann, ast.Constant) and isinstance(ann.value, str):
            try:
                ann = ast.parse(ann.value, mode="eval").body
                ast.fix_missing_locations(ann)
            except SyntaxError:
                return
        frm = (owner or self.owner[-1]).id
        for n in ast.walk(ann):
            if not isinstance(n, (ast.Name, ast.Attribute)):
                continue
            t = self.fe.resolve_expr(self.src, scope_decl or self.scope[-1], n)
            tid = self.fe.target_id(t)
            if tid is None or (isinstance(t, DeclT) and t.decl.kind in SKIPPED_TARGETS):
                continue
            line = getattr(n, "lineno", getattr(owner, "line", 1))
            emit("ref", **{"from": frm}, to=tid, kind="type", file=self.src.path, line=line)
            if isinstance(t, DeclT) and t.decl.kind == "class":
                emit("type_ref", **{"from": frm}, to=tid, position=position, file=self.src.path, line=line)
            if "quality" in self.fe.layers and ((isinstance(n, ast.Name) and n.id == "Any") or (isinstance(n, ast.Attribute) and n.attr == "Any")):
                emit("any_site", fn=frm, file=self.src.path, line=line, kind="explicit")

    def symbol_types(self, fn: Decl):
        for p, _, _ in getattr(fn, "params", []):
            if p.annotation is not None:
                self.emit_symbol_type(p, p.annotation)
        node = fn.node
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.returns is not None:
            self.emit_symbol_type(fn, node.returns)

    def emit_symbol_type(self, d: Decl, ann: ast.AST):
        text = ast.unparse(ann)
        if isinstance(ann, ast.Constant) and isinstance(ann.value, str):
            text = ann.value
        names = {getattr(n, "id", None) or getattr(n, "attr", None) for n in ast.walk(ann)}
        emit("symbol_type", symbol=d.id, text=truncate(text, 200), is_any="Any" in names and text in ("Any", "typing.Any"),
             is_unknown=False, is_promise=bool(names & {"Awaitable", "Coroutine"}), is_function="Callable" in names,
             is_union="|" in text or bool(names & {"Union", "Optional"}))


# ── helpers ───────────────────────────────────────────────────────────────


def walk_own(node: ast.AST):
    """Every node inside a function or class, nested functions and classes excluded."""
    stack = list(ast.iter_child_nodes(node))
    while stack:
        n = stack.pop()
        yield n
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda)):
            continue
        stack.extend(ast.iter_child_nodes(n))


def walk_with_guard(tree: ast.AST):
    """Every node, with whether it sits under `if TYPE_CHECKING:`."""
    stack = [(tree, False)]
    while stack:
        n, guarded = stack.pop()
        yield n, guarded
        if isinstance(n, ast.If) and is_type_checking(n.test):
            stack.extend((c, True) for c in reversed(n.body))
            stack.extend((c, guarded) for c in reversed(n.orelse))
            stack.append((n.test, guarded))
            continue
        stack.extend((c, guarded) for c in reversed(list(ast.iter_child_nodes(n))))


def is_type_checking(test: ast.AST) -> bool:
    return (isinstance(test, ast.Name) and test.id == "TYPE_CHECKING") or (
        isinstance(test, ast.Attribute) and test.attr == "TYPE_CHECKING")


def decorator_name(d: ast.AST) -> str:
    target = d.func if isinstance(d, ast.Call) else d
    try:
        return ast.unparse(target)
    except Exception:  # noqa: BLE001 — a decorator we cannot print names nothing
        return ""


def base_text(b: ast.AST) -> str:
    try:
        return ast.unparse(b)
    except Exception:  # noqa: BLE001
        return ""


def visibility(name: str) -> str:
    if name.startswith("__") and not name.endswith("__"):
        return "private"
    if name.startswith("_") and not name.startswith("__"):
        return "protected"
    return "public"


def is_public(src: Source, name: str) -> bool:
    if src.all_names is not None:
        return name in src.all_names
    return not name.startswith("_")


def sloc(text: str) -> int:
    lines = set()
    try:
        for tok in tokenize.generate_tokens(io.StringIO(text).readline):
            if tok.type not in (tokenize.COMMENT, tokenize.NL, tokenize.NEWLINE, tokenize.INDENT, tokenize.DEDENT, tokenize.ENDMARKER):
                for ln in range(tok.start[0], tok.end[0] + 1):
                    lines.add(ln)
    except (tokenize.TokenError, SyntaxError, IndentationError):
        return sum(1 for ln in text.splitlines() if ln.strip() and not ln.strip().startswith("#"))
    return len(lines)


def requirement_name(spec: str) -> str | None:
    m = re.match(r"\s*([A-Za-z0-9][A-Za-z0-9._-]*)", spec)
    return m.group(1) if m else None


def dependency_blocks(data: dict):
    proj = data.get("project") or {}
    yield "prod", list(proj.get("dependencies") or [])
    for _, deps in sorted((proj.get("optional-dependencies") or {}).items()):
        yield "optional", list(deps)
    for _, deps in sorted((data.get("dependency-groups") or {}).items()):
        yield "dev", [d for d in deps if isinstance(d, str)]
    poetry = (data.get("tool") or {}).get("poetry") or {}
    yield "prod", [f"{k}" for k in (poetry.get("dependencies") or {}) if k.lower() != "python"]
    yield "dev", list((poetry.get("dev-dependencies") or {}).keys())
    for _, group in sorted((poetry.get("group") or {}).items()):
        yield "dev", list((group.get("dependencies") or {}).keys())


def mark_parents(tree: ast.AST) -> None:
    for parent in ast.walk(tree):
        for child in ast.iter_child_nodes(parent):
            if not hasattr(child, "_parent"):
                child._parent = parent  # type: ignore[attr-defined]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", required=True)
    ap.add_argument("--layers", default="structure,refs,flow,quality")
    ap.add_argument("--exclude", action="append", default=[])
    ap.add_argument("--first-call-site", type=int, default=1)
    ap.add_argument("--first-flow-node", type=int, default=1)
    ap.add_argument("targets", nargs="+")
    a = ap.parse_args()
    layers = set(a.layers.split(","))
    fe = Frontend(os.path.abspath(a.root), layers, [re.compile(x) for x in a.exclude], a.first_call_site, a.first_flow_node)
    fe.discover(a.targets)
    fe.declare_all()
    for src in fe.sources:
        if src.tree is not None:
            mark_parents(src.tree)
    fe.emit_structure()
    if "refs" in layers:
        fe.emit_refs()
    if "flow" in layers or "quality" in layers:
        from py_flow import emit_flow_and_quality  # noqa: PLC0415 — the second half, beside this file

        emit_flow_and_quality(fe, layers, emit, sys.modules[__name__])
    fe.emit_symbols()
    flush()
    sys.stdout.write(json.dumps({"relation": "__counters__", "row": {"call_site": fe.next_call_site, "flow_node": fe.next_flow_node}}) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
