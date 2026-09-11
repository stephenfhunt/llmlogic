"""code-facts' Python frontend, second half: the flow and quality layers.

Called by py_facts.py's main() with the Frontend and with py_facts itself —
which runs as __main__, so importing it here would load a second copy whose
classes are not the ones the Frontend's objects were made from.

The control-flow graph follows the TypeScript layer's model (src/layers/flow.ts)
so that lib/ reads either language: statement-level nodes, an entry, exit and
throw_exit per function, exceptions over-approximated inside a `try` (any node
may raise to the handler), and a `finally` entered by every jump that crosses it
and re-issuing each. What Python adds:

- a loop's `else:` runs when the loop's test fails, and not after `break`;
- `except` clauses are tested in order, each a `catch` node: `on_true` into its
  body, `on_false` to the next, and past the last typed one the exception
  propagates; a `try`'s `else:` runs outside the handlers but inside `finally`;
- `with` is a `try/finally` whose exit (`__exit__`, a `finally` node) may also
  resume after the block, since it can suppress the exception;
- `match` is a `switch` whose cases do not fall through; a case's guard is a
  `cond` of its own, reached only when the pattern matched; an irrefutable,
  unguarded case is `default`;
- `assert` may raise; `def` and `class` are statements, and their decorators,
  defaults and class bodies run there; comprehension `for`/`if` clauses,
  `and`/`or` and `x if c else y` are decisions without nodes of their own.
"""

from __future__ import annotations

import ast
import io
import keyword
import re
import tokenize
from bisect import bisect_right

VAR_KINDS = {"local", "parameter", "variable"}
OWNER_FN_KINDS = {"function", "method", "constructor", "getter", "setter"}
NO_IMPLICIT_THROW = {"entry", "exit", "throw_exit", "catch", "finally", "break", "continue"}
FN_NODES = (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)
TRY_NODES = (ast.Try, ast.TryStar) if hasattr(ast, "TryStar") else (ast.Try,)
NESTING = (ast.If, ast.For, ast.AsyncFor, ast.While, ast.Match, ast.ExceptHandler, ast.IfExp)
MARKER = re.compile(r"\b(TODO|FIXME|HACK|XXX)\b[:\s-]*(.*)")


class Loop:
    def __init__(self, continue_target: int):
        self.continue_target = continue_target
        self.breaks: list[tuple[int, str]] = []


class Catch:
    def __init__(self, node: int):
        self.node = node


class Finally:
    def __init__(self, node: int):
        self.node = node
        self.routed: dict[str, None] = {}  # jump kinds that entered it, in order


class Positions:
    """AST columns are UTF-8 byte offsets and tokenize's are characters; this
    converts the first to the second, line by line."""

    def __init__(self, text: str):
        self.lines = text.splitlines()

    def char_col(self, line: int, byte_col: int) -> int:
        if 1 <= line <= len(self.lines):
            s = self.lines[line - 1]
            if not s.isascii():
                return len(s.encode("utf-8")[:byte_col].decode("utf-8", errors="replace"))
        return byte_col


class FnBuilder:
    def __init__(self, fx: "FlowExtractor", decl, node: ast.AST, kind: str):
        self.fx = fx
        self.fe = fx.fe
        self.src = fx.src
        self.decl = decl
        self.node = node
        self.kind = kind
        self.frames: list = []
        self.nodes: list[tuple[int, str, int, int]] = []
        self.edges: list[tuple[int, int, str]] = []
        self.roots: list[tuple[int, ast.AST, object]] = []
        self.nesting = 0
        self.nesting_of: dict[int, int] = {}
        self.decisions = 0
        start = (1, 0) if kind == "module" else (node.lineno, node.col_offset)
        end = (len(self.src.text.splitlines()) or 1, 0) if kind == "module" else (node.end_lineno, max(0, node.end_col_offset - 1))
        self.entry = self.new("entry", start, False)
        self.exit = self.new("exit", end, False)
        self.throw_exit = self.new("throw_exit", end, False)

    # ── the graph ──

    def new(self, kind: str, at, implicit: bool = True) -> int:
        nid = self.fe.next_flow_node
        self.fe.next_flow_node += 1
        line, col = at if isinstance(at, tuple) else (at.lineno, at.col_offset)
        self.nodes.append((nid, kind, line, col + 1))
        self.nesting_of[nid] = self.nesting
        if implicit and kind not in NO_IMPLICIT_THROW:
            self.implicit_throw(nid)
        return nid

    def connect(self, preds: list[tuple[int, str]], to: int, relabel: str | None = None) -> None:
        """Joins pending edges to `to`. `relabel` renames plain fall-through only:
        a branch edge keeps its kind, or two branches meeting at one node would
        collapse into one edge."""
        for frm, kind in preds:
            self.edges.append((frm, to, relabel if relabel is not None and kind == "next" else kind))

    def evaluates(self, node: int, *exprs, scope=None) -> None:
        for e in exprs:
            if e is not None:
                self.roots.append((node, e, scope or self.decl))

    def decision(self, kind: str, node: int | None, line: int, nesting: int) -> None:
        self.decisions += 1
        self.fx.emit("decision", fn=self.decl.id, node=node, kind=kind, nesting=nesting, file=self.src.path, line=line)

    def implicit_throw(self, node: int) -> None:
        if any(isinstance(f, (Catch, Finally)) for f in self.frames):
            self.jump([(node, "throw")], "throw", len(self.frames))

    def jump(self, preds: list[tuple[int, str]], kind: str, depth: int) -> None:
        """Sends `preds` along a jump, resolving it from frame depth `depth` outward."""
        for i in range(depth - 1, -1, -1):
            f = self.frames[i]
            if isinstance(f, Finally):
                self.connect(preds, f.node, kind)
                f.routed[kind] = None
                return
            if kind == "throw" and isinstance(f, Catch):
                self.connect(preds, f.node, "throw")
                return
            if kind == "break" and isinstance(f, Loop):
                f.breaks.extend((frm, "break" if k == "next" else k) for frm, k in preds)
                return
            if kind == "continue" and isinstance(f, Loop):
                self.connect(preds, f.continue_target, "continue")
                return
        if kind == "return":
            self.connect(preds, self.exit, "return")
        elif kind == "throw":
            self.connect(preds, self.throw_exit, "throw")

    def nested(self, f):
        self.nesting += 1
        try:
            return f()
        finally:
            self.nesting -= 1

    # ── statements ──

    def build(self) -> None:
        start = [(self.entry, "next")]
        if self.kind == "module":
            self.connect(self.statements(self.node.body, start), self.exit, "next")
            return
        for p, _, _ in getattr(self.decl, "params", []):
            self.fx.access(self.entry, p.id, "def")
        if isinstance(self.node, ast.Lambda):
            n = self.new("return", self.node.body)
            self.evaluates(n, self.node.body)
            self.connect(start, n)
            self.jump([(n, "return")], "return", len(self.frames))
        else:
            self.connect(self.statements(self.node.body, start), self.exit, "next")

    def statements(self, body: list[ast.stmt], preds):
        for s in body:
            preds = self.statement(s, preds)
        return preds

    def statement(self, s: ast.stmt, preds):
        if isinstance(s, (ast.Pass, ast.Global, ast.Nonlocal)) or (isinstance(s, ast.Expr) and isinstance(s.value, ast.Constant)):
            return preds  # does nothing at run time: no node
        if isinstance(s, ast.If):
            cond = self.new("cond", s.test)
            self.evaluates(cond, s.test)
            self.decision("if", cond, s.lineno, self.nesting)
            self.connect(preds, cond)
            then_out = self.nested(lambda: self.statements(s.body, [(cond, "on_true")]))
            else_out = self.nested(lambda: self.statements(s.orelse, [(cond, "on_false")])) if s.orelse else [(cond, "on_false")]
            return then_out + else_out
        if isinstance(s, ast.While):
            head = self.new("loop_head", s.test)
            self.evaluates(head, s.test)
            self.decision("while", head, s.lineno, self.nesting)
            self.connect(preds, head)
            return self.loop(s, head)
        if isinstance(s, (ast.For, ast.AsyncFor)):
            # The iterable is evaluated once; the head fetches each element and binds it.
            it = self.new("stmt", s.iter)
            self.evaluates(it, s.iter)
            self.connect(preds, it)
            head = self.new("loop_head", s.target)
            self.evaluates(head, s.target)
            self.decision("for_of", head, s.lineno, self.nesting)
            if isinstance(s, ast.AsyncFor):
                self.fx.emit("await_at", node=head)
            self.connect([(it, "next")], head)
            return self.loop(s, head)
        if isinstance(s, (ast.Break, ast.Continue)):
            kind = "break" if isinstance(s, ast.Break) else "continue"
            n = self.new(kind, s)
            self.connect(preds, n)
            self.jump([(n, kind)], kind, len(self.frames))
            return []
        if isinstance(s, ast.Return):
            n = self.new("return", s)
            self.evaluates(n, s.value)
            self.connect(preds, n)
            self.jump([(n, "return")], "return", len(self.frames))
            return []
        if isinstance(s, ast.Raise):
            n = self.new("throw", s)
            self.evaluates(n, s.exc, s.cause)
            self.connect(preds, n)
            self.jump([(n, "throw")], "throw", len(self.frames))
            return []
        if isinstance(s, ast.Assert):
            n = self.new("stmt", s, False)
            self.evaluates(n, s.test, s.msg)
            self.connect(preds, n)
            self.jump([(n, "throw")], "throw", len(self.frames))
            return [(n, "next")]
        if isinstance(s, TRY_NODES):
            return self.try_statement(s, preds)
        if isinstance(s, (ast.With, ast.AsyncWith)):
            return self.with_statement(s, preds)
        if isinstance(s, ast.Match):
            return self.match_statement(s, preds)
        # Everything else is one straight-line node; a def or class runs its
        # decorators, defaults and (for a class) its body there.
        n = self.new("stmt", s)
        self.evaluates(n, s)
        if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef)):
            d = self.fe.decl_of.get(id(s))
            if d is not None:
                self.fx.emit("closure", node=n, fn=d.id)
        self.connect(preds, n)
        return [(n, "next")]

    def loop(self, s, head: int):
        frame = Loop(head)
        self.frames.append(frame)
        body_out = self.nested(lambda: self.statements(s.body, [(head, "on_true")]))
        self.frames.pop()
        self.connect(body_out, head, "back")
        exits = [(head, "on_false")]
        if s.orelse:  # runs when the test fails; a break skips it
            exits = self.nested(lambda: self.statements(s.orelse, exits))
        return exits + frame.breaks

    def try_statement(self, s, preds):
        fin = Finally(self.new("finally", s.finalbody[0], False)) if s.finalbody else None
        catches: list[int] = []
        for h in s.handlers:
            c = self.new("catch", h, False)
            self.evaluates(c, h.type)
            if h.name:
                self.bind_def(c, h.name)
            self.decision("catch", c, h.lineno, self.nesting)
            catches.append(c)
        if fin is not None:
            self.frames.append(fin)
        if catches:
            self.frames.append(Catch(catches[0]))
        try_out = self.nested(lambda: self.statements(s.body, preds))
        if catches:
            self.frames.pop()
        out = self.nested(lambda: self.statements(s.orelse, try_out)) if s.orelse else try_out
        for i, h in enumerate(s.handlers):
            c = catches[i]
            typed = h.type is not None
            out = out + self.nested(lambda: self.statements(h.body, [(c, "on_true" if typed else "next")]))
            if typed:
                if i + 1 < len(catches):
                    self.connect([(c, "on_false")], catches[i + 1])
                else:  # no clause matched: the exception goes on
                    self.jump([(c, "on_false")], "throw", len(self.frames))
        if fin is None:
            return out
        self.frames.pop()
        self.connect(out, fin.node, "next")
        fin_out = self.nested(lambda: self.statements(s.finalbody, [(fin.node, "next")]))
        depth = len(self.frames)
        for kind in fin.routed:
            self.jump(fin_out, kind, depth)
        return fin_out if out else []

    def with_statement(self, s, preds):
        # __exit__ is a finally: every jump out of the body passes it. It may
        # also swallow an exception, so after a throw it can resume below.
        many = len(s.items) > 1
        ex = Finally(self.new("finally", s, False))
        if many:  # a later context expression can raise into an earlier __exit__
            self.frames.append(ex)
        w = self.new("stmt", s)
        self.evaluates(w, *[i.context_expr for i in s.items], *[i.optional_vars for i in s.items])
        if isinstance(s, ast.AsyncWith):
            self.fx.emit("await_at", node=w)
        self.connect(preds, w)
        if not many:
            self.frames.append(ex)
        body_out = self.statements(s.body, [(w, "next")])
        self.frames.pop()
        self.connect(body_out, ex.node, "next")
        exit_out = [(ex.node, "next")]
        depth = len(self.frames)
        for kind in ex.routed:
            self.jump(exit_out, kind, depth)
        return exit_out if body_out or "throw" in ex.routed else []

    def match_statement(self, s, preds):
        sw = self.new("switch", s.subject)
        self.evaluates(sw, s.subject)
        self.connect(preds, sw)
        tests = [(sw, "next")]
        entries = []
        for case in s.cases:
            if case.guard is None and irrefutable(case.pattern):
                # like `default:`, entered when every test before it failed
                self.evaluates(sw, case.pattern)
                self.pattern_defs(sw, case.pattern)
                entries.append((case, [(frm, "default") for frm, _ in tests]))
                tests = []
                break
            t = self.new("case_test", case.pattern)
            self.evaluates(t, case.pattern)
            self.pattern_defs(t, case.pattern)
            self.decision("case", t, case.pattern.lineno, self.nesting)
            self.connect(tests, t)
            if case.guard is None:
                tests = [(t, "on_false")]
                entries.append((case, [(t, "case")]))
                continue
            # the guard runs only once the pattern has matched: a node of its own
            g = self.new("cond", case.guard)
            self.evaluates(g, case.guard)
            self.decision("if", g, case.guard.lineno, self.nesting)
            self.connect([(t, "case")], g)
            tests = [(t, "on_false"), (g, "on_false")]
            entries.append((case, [(g, "on_true")]))
        out = []
        for case, entry in entries:
            out += self.nested(lambda: self.statements(case.body, entry))
        return out + tests

    def pattern_defs(self, node: int, pattern: ast.AST) -> None:
        for n in ast.walk(pattern):
            name = getattr(n, "name", None) if isinstance(n, (ast.MatchAs, ast.MatchStar)) else None
            if name:
                self.bind_def(node, name)
            if isinstance(n, ast.MatchMapping) and n.rest:
                self.bind_def(node, n.rest)

    def bind_def(self, node: int, name: str) -> None:
        d = self.variable(self.decl, name)
        if d is not None:
            self.fx.access(node, d.id, "def")
            self.capture(d)

    # ── what each node's expressions do ──

    def variable(self, scope, name: str):
        t = self.fe.lookup(self.src, scope, name)
        return t.decl if isinstance(t, self.fx.api.DeclT) and t.decl.kind in VAR_KINDS else None

    def capture(self, d) -> None:
        owner = d.parent
        if owner is not None and owner is not self.decl and owner.kind in OWNER_FN_KINDS:
            self.fx.emit("captures", fn=self.decl.id, var=d.id)

    def flush(self) -> None:
        emit, path = self.fx.emit, self.src.path
        for nid, kind, line, col in self.nodes:
            emit("flow_node", id=nid, fn=self.decl.id, kind=kind, file=path, line=line, col=col)
        for frm, to, kind in self.edges:
            emit("flow_edge", **{"from": frm, "to": to, "kind": kind})
        for node, root, scope in self.roots:
            self.walk(node, root, scope)
        self.fx.flush_accesses()

    def walk(self, node: int, root: ast.AST, scope) -> None:
        emit = self.fx.emit
        nesting = self.nesting_of.get(node, 0)
        stack = [(root, scope)]
        while stack:
            n, sc = stack.pop()
            if isinstance(n, ast.Lambda):
                d = self.fe.decl_of.get(id(n))
                if d is not None:
                    emit("closure", node=node, fn=d.id)
                stack.extend((x, sc) for x in [*n.args.defaults, *[k for k in n.args.kw_defaults if k is not None]])
                continue
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
                # at definition: decorators and defaults; the body is its own function
                stack.extend((x, sc) for x in [*n.decorator_list, *n.args.defaults, *[k for k in n.args.kw_defaults if k is not None]])
                continue
            if isinstance(n, ast.ClassDef):
                d = self.fe.decl_of.get(id(n))
                stack.extend((x, sc) for x in [*n.decorator_list, *n.bases, *[k.value for k in n.keywords]])
                stack.extend((x, d or sc) for x in n.body)  # a class body resolves in the class
                continue
            if isinstance(n, ast.Call) and hasattr(n, "_call_site"):
                emit("call_at", node=node, call_site=n._call_site)
            elif isinstance(n, ast.Await):
                emit("await_at", node=node)
            elif isinstance(n, (ast.Yield, ast.YieldFrom)):
                emit("yield_at", node=node)
            elif isinstance(n, ast.IfExp):
                self.decision("conditional", node, n.lineno, nesting)
            elif isinstance(n, ast.BoolOp):
                for _ in n.values[1:]:
                    self.decision("and" if isinstance(n.op, ast.And) else "or", node, n.lineno, nesting)
            elif isinstance(n, ast.comprehension):
                self.decision("for_of", node, n.target.lineno, nesting)
                for cond in n.ifs:
                    self.decision("if", node, cond.lineno, nesting)
                if n.is_async:
                    emit("await_at", node=node)
            elif isinstance(n, ast.Name):
                self.access(node, n, sc)
            stack.extend((c, sc) for c in reversed(list(ast.iter_child_nodes(n))))

    def access(self, node: int, name: ast.Name, scope) -> None:
        d = self.variable(scope, name.id)
        if d is None:
            return
        parent = getattr(name, "_parent", None)
        if isinstance(parent, ast.AugAssign) and parent.target is name:
            self.fx.access(node, d.id, "def")
            self.fx.access(node, d.id, "use")
        elif isinstance(name.ctx, (ast.Store, ast.Del)):
            self.fx.access(node, d.id, "def")
        else:
            self.fx.access(node, d.id, "use")
        self.capture(d)


def irrefutable(p: ast.AST) -> bool:
    if isinstance(p, ast.MatchAs):
        return p.pattern is None or irrefutable(p.pattern)
    if isinstance(p, ast.MatchOr):
        return any(irrefutable(x) for x in p.patterns)
    return False


# ── per-function metrics ──────────────────────────────────────────────────


def is_elif(n: ast.AST) -> bool:
    p = getattr(n, "_parent", None)
    return isinstance(n, ast.If) and isinstance(p, ast.If) and len(p.orelse) == 1 and p.orelse[0] is n and n.col_offset == p.col_offset


def measure(fn: ast.AST, is_module: bool) -> dict:
    m = {"statements": 0, "max_nesting": 0, "cognitive": 0, "returns": 0, "awaits": 0, "yields": 0, "throws": 0}

    def visit(n: ast.AST, depth: int, cog: int) -> None:
        if isinstance(n, FN_NODES):
            return
        if isinstance(n, ast.stmt):
            m["statements"] += 1
        if isinstance(n, ast.Return):
            m["returns"] += 1
        elif isinstance(n, ast.Raise):
            m["throws"] += 1
        elif isinstance(n, ast.Await):
            m["awaits"] += 1
        elif isinstance(n, (ast.Yield, ast.YieldFrom)):
            m["yields"] += 1
        d = depth
        elif_ = is_elif(n)
        if isinstance(n, NESTING) and not elif_:
            d = depth + 1
            m["max_nesting"] = max(m["max_nesting"], d)
        # SonarSource cognitive complexity
        child_cog = cog
        if isinstance(n, ast.If):
            m["cognitive"] += 1 if elif_ else 1 + cog
            chained = len(n.orelse) == 1 and is_elif(n.orelse[0])
            if n.orelse and not chained:
                m["cognitive"] += 1  # else
            visit(n.test, d, cog)
            for s in n.body:
                visit(s, d, cog + 1)
            for s in n.orelse:
                visit(s, d, cog if chained else cog + 1)
            return
        if isinstance(n, (ast.IfExp, ast.Match, ast.For, ast.AsyncFor, ast.While, ast.ExceptHandler)):
            m["cognitive"] += 1 + cog
            child_cog = cog + 1
        if isinstance(n, ast.BoolOp):
            p = getattr(n, "_parent", None)
            if not (isinstance(p, ast.BoolOp) and type(p.op) is type(n.op)):
                m["cognitive"] += 1
        for c in ast.iter_child_nodes(n):
            visit(c, d, child_cog)

    body = fn.body if is_module or not isinstance(fn, ast.Lambda) else [fn.body]
    for s in body:
        visit(s, 0, 0)
    return m


# ── the extractor ─────────────────────────────────────────────────────────


class FlowExtractor:
    def __init__(self, fe, api, layers: set[str], emit):
        self.fe = fe
        self.api = api
        self.layers = layers
        self.emit = emit
        self.src = None
        self.pending: list[tuple[int, str, str]] = []

    def access(self, node: int, var: str, kind: str) -> None:
        self.pending.append((node, var, kind))

    def flush_accesses(self) -> None:
        seen = set()
        for row in self.pending:
            if row not in seen:
                seen.add(row)
                self.emit(row[2], node=row[0], var=row[1])
        self.pending.clear()

    def run(self) -> None:
        for src in self.fe.sources:
            if src.tree is None or src.decl is None:
                continue
            self.src = src
            tokens = file_tokens(src.text)
            if "flow" in self.layers:
                self.flow(src, tokens)
            if "quality" in self.layers:
                self.quality(src, tokens)

    # ── flow ──

    def flow(self, src, tokens) -> None:
        fns = [(src.decl, src.tree, "module")]
        for n in ast.walk(src.tree):
            if isinstance(n, FN_NODES):
                d = self.fe.decl_of.get(id(n))
                if d is not None:
                    fns.append((d, n, "lambda" if isinstance(n, ast.Lambda) else d.kind))
        pos = Positions(src.text)
        for decl, node, kind in fns:
            b = FnBuilder(self, decl, node, kind)
            b.build()
            b.flush()
            m = measure(node, kind == "module")
            if kind == "module":
                line, end = 1, len(src.text.splitlines()) or 1
            else:
                line, end = node.lineno, node.end_lineno
            h = halstead(tokens, node, kind == "module", pos)
            params = getattr(decl, "params", [])
            bound = decl.is_method and params and "staticmethod" not in getattr(decl, "decorators", []) and kind != "lambda"
            self.emit("fn", id=decl.id, kind=kind, file=src.path, line=line, end_line=end, loc=end - line + 1,
                      statements=m["statements"], params=len(params) - (1 if bound else 0), max_nesting=m["max_nesting"],
                      cyclomatic=1 + b.decisions, cognitive=m["cognitive"], returns=m["returns"], awaits=m["awaits"],
                      yields=m["yields"], throws=m["throws"], halstead_operators=h[0], halstead_operands=h[1],
                      halstead_distinct_operators=h[2], halstead_distinct_operands=h[3])

    # ── quality ──

    def quality(self, src, tokens) -> None:
        for tok in tokens:
            if tok.type != tokenize.COMMENT:
                continue
            line = tok.start[0]
            for tool, directive, rules in lint_directives(tok.string):
                self.emit("lint_directive", file=src.path, line=line, tool=tool, directive=directive,
                          rules=self.api.truncate(rules, 200) if rules and rules.strip() else None)
            mk = MARKER.search(tok.string)
            if mk:
                self.emit("comment_marker", file=src.path, line=line, kind=mk.group(1).lower(), text=self.api.truncate(mk.group(2), 200))
        QualityWalker(self, src).run()


class QualityWalker:
    """literal, throw_site, catch_site, assertion (`typing.cast`),
    floating_promise (an `async def`'s coroutine discarded) and any_site for
    unannotated parameters. `fn` is the enclosing declaration, as `ref.from`."""

    def __init__(self, fx: FlowExtractor, src):
        self.fx, self.fe, self.api, self.src = fx, fx.fe, fx.api, src
        self.emit = fx.emit

    def run(self) -> None:
        self.visit(self.src.tree, self.src.decl, self.src.decl, skip=set())

    def visit(self, n: ast.AST, owner, scope, skip: set) -> None:
        stack = [(n, owner, scope)]
        while stack:
            n, owner, scope = stack.pop()
            if id(n) in skip:
                continue
            path, line = self.src.path, getattr(n, "lineno", owner.line)
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
                d = self.fe.decl_of.get(id(n))
                if d is not None and not isinstance(n, ast.Lambda):
                    self.implicit_any(d)
                args = n.args  # the signature's annotations are not visited: types, not values
                inner = (d, d) if d is not None else (owner, scope)
                if not isinstance(n, ast.Lambda):
                    stack.extend((x, owner, scope) for x in [*n.decorator_list, *args.defaults, *[k for k in args.kw_defaults if k is not None]])
                    stack.extend((x, *inner) for x in n.body)
                else:
                    stack.extend((x, owner, scope) for x in [*args.defaults, *[k for k in args.kw_defaults if k is not None]])
                    stack.append((n.body, *inner))
                continue
            if isinstance(n, ast.ClassDef):
                d = self.fe.decl_of.get(id(n))
                stack.extend((x, owner, scope) for x in [*n.decorator_list, *n.bases, *[k.value for k in n.keywords]])
                stack.extend((x, d or owner, d or scope) for x in n.body)
                continue
            if isinstance(n, ast.Expr) and isinstance(n.value, ast.Constant):
                continue  # a docstring, or a bare constant
            if isinstance(n, ast.AnnAssign):
                skip.add(id(n.annotation))
            if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__all__" for t in n.targets):
                continue
            if isinstance(n, ast.Dict):
                skip.update(id(k) for k in n.keys if k is not None)
            if isinstance(n, ast.JoinedStr):
                seg = ast.get_source_segment(self.src.text, n) or ""
                self.emit("literal", fn=owner.id, file=path, line=line, kind="template",
                          value=self.api.truncate(strip_fstring(seg), 100))
                stack.extend((v.value, owner, scope) for v in n.values if isinstance(v, ast.FormattedValue))
                continue
            if isinstance(n, ast.Constant) and not isinstance(n.value, bool) and n.value is not None and n.value is not Ellipsis:
                v = n.value
                if isinstance(v, str):
                    self.emit("literal", fn=owner.id, file=path, line=line, kind="string", value=self.api.truncate(v, 100))
                elif isinstance(v, bytes):
                    self.emit("literal", fn=owner.id, file=path, line=line, kind="string",
                              value=self.api.truncate(v.decode("utf-8", errors="replace"), 100))
                elif isinstance(v, (int, float, complex)):
                    self.emit("literal", fn=owner.id, file=path, line=line, kind="number", value=self.api.truncate(repr(v), 100))
            if isinstance(n, ast.Raise) and n.exc is not None:
                e = n.exc.func if isinstance(n.exc, ast.Call) else n.exc
                t = self.fe.resolve_expr(self.src, scope, e) if isinstance(e, (ast.Name, ast.Attribute)) else None
                self.emit("throw_site", fn=owner.id, file=path, line=line, type=self.fe.target_id(t) if t is not None else None)
            if isinstance(n, ast.ExceptHandler):
                empty = all(isinstance(s, ast.Pass) or (isinstance(s, ast.Expr) and isinstance(s.value, ast.Constant)) for s in n.body)
                rethrows = any(isinstance(x, ast.Raise) for s in n.body for x in [s, *self.api.walk_own(s)])
                self.emit("catch_site", fn=owner.id, file=path, line=line, binds=n.name is not None, empty=empty, rethrows=rethrows)
            if isinstance(n, ast.Call):
                self.call(n, owner, scope, skip)
            if isinstance(n, ast.Assign) and owner.kind in ("module", "class"):
                # a module- or class-level variable owns its initializer, as in `ref.from`
                ts = [t for t in n.targets if isinstance(t, ast.Name)]
                table = self.src.scope if owner.kind == "module" else owner.members
                d = table.get(ts[0].id) if len(ts) == 1 and len(n.targets) == 1 else None
                if isinstance(d, self.api.Decl) and d.node is ts[0]:
                    stack.append((n.value, d, scope))
                    continue
            stack.extend((c, owner, scope) for c in reversed(list(ast.iter_child_nodes(n))))

    def call(self, n: ast.Call, owner, scope, skip: set) -> None:
        f = n.func
        name = f.id if isinstance(f, ast.Name) else f.attr if isinstance(f, ast.Attribute) else None
        if name == "cast" and n.args:
            t = self.fe.resolve_expr(self.src, scope, f)
            tid = self.fe.target_id(t) if t is not None else None
            if tid in ("ext:typing#cast", "ext:typing_extensions#cast"):
                ty = n.args[0]
                skip.add(id(ty))  # a type, even when spelled as a string
                text = ty.value if isinstance(ty, ast.Constant) and isinstance(ty.value, str) else self.api.base_text(ty)
                self.emit("assertion", fn=owner.id, file=self.src.path, line=n.lineno, kind="cast", to_type=self.api.truncate(text, 200),
                          from_any=False, to_any=text in ("Any", "typing.Any"))
        parent = getattr(n, "_parent", None)
        if isinstance(parent, ast.Expr) and hasattr(n, "_call_site"):
            t = self.fe.resolve_expr(self.src, scope, f) if isinstance(f, (ast.Name, ast.Attribute)) else None
            d = t.decl if isinstance(t, self.api.DeclT) else None
            if d is not None and d.is_async and d.kind in OWNER_FN_KINDS:
                self.emit("floating_promise", call_site=n._call_site, fn=owner.id, file=self.src.path, line=n.lineno)

    def implicit_any(self, d) -> None:
        params = getattr(d, "params", [])
        bound = d.is_method and "staticmethod" not in getattr(d, "decorators", [])
        for i, (p, _, _) in enumerate(params):
            if p.annotation is None and not (bound and i == 0):
                self.emit("any_site", fn=d.id, file=self.src.path, line=p.line, kind="implicit_param")


# ── tokens: Halstead, comments ────────────────────────────────────────────


def file_tokens(text: str) -> list:
    out = []
    try:
        for tok in tokenize.generate_tokens(io.StringIO(text).readline):
            out.append(tok)
    except (tokenize.TokenError, SyntaxError, IndentationError):
        pass
    return out


SKIP_TOKENS = {tokenize.NEWLINE, tokenize.NL, tokenize.INDENT, tokenize.DEDENT, tokenize.COMMENT, tokenize.ENDMARKER,
               tokenize.ENCODING, getattr(tokenize, "TYPE_COMMENT", -1)}
FSTRING_START = getattr(tokenize, "FSTRING_START", -2)
FSTRING_END = getattr(tokenize, "FSTRING_END", -3)


def halstead(tokens: list, fn: ast.AST, is_module: bool, pos: Positions) -> tuple[int, int, int, int]:
    """Token leaves of the body, nested functions excluded. An f-string is one
    operand, as it is one token before Python 3.12."""
    if is_module:
        lo, hi = (1, 0), (10**9, 0)
        body = fn.body
    elif isinstance(fn, ast.Lambda):
        body = [fn.body]
        lo = (fn.body.lineno, pos.char_col(fn.body.lineno, fn.body.col_offset))
        hi = (fn.end_lineno, pos.char_col(fn.end_lineno, fn.end_col_offset))
    else:
        body = fn.body
        lo = (body[0].lineno, pos.char_col(body[0].lineno, body[0].col_offset))
        hi = (fn.end_lineno, pos.char_col(fn.end_lineno, fn.end_col_offset))
    spans = sorted(
        ((n.lineno, pos.char_col(n.lineno, n.col_offset)), (n.end_lineno, pos.char_col(n.end_lineno, n.end_col_offset)))
        for s in body for n in [s, *ast.walk(s)] if isinstance(n, FN_NODES) and n is not fn
    )
    holes: list[tuple[tuple[int, int], tuple[int, int]]] = []  # disjoint: a nested function's span swallows its own
    for a, b in spans:
        if holes and a < holes[-1][1]:
            holes[-1] = (holes[-1][0], max(holes[-1][1], b))
        else:
            holes.append((a, b))
    starts = [h[0] for h in holes]
    ops = opnds = 0
    distinct_ops: set[str] = set()
    distinct_opnds: set[str] = set()
    depth = 0
    fstring: list[str] = []
    for tok in tokens:
        if tok.start < lo or tok.end > hi or tok.type in SKIP_TOKENS:
            continue
        i = bisect_right(starts, tok.start) - 1
        if i >= 0 and tok.start < holes[i][1]:
            continue
        if tok.type == FSTRING_START:
            depth += 1
            fstring.append(tok.string)
            continue
        if depth > 0:
            fstring.append(tok.string)
            if tok.type == FSTRING_END:
                depth -= 1
                if depth == 0:
                    opnds += 1
                    distinct_opnds.add("".join(fstring))
                    fstring = []
            continue
        if tok.type in (tokenize.NUMBER, tokenize.STRING) or (tok.type == tokenize.NAME and (
                not keyword.iskeyword(tok.string) or tok.string in ("True", "False", "None"))):
            opnds += 1
            distinct_opnds.add(tok.string)
        else:
            ops += 1
            distinct_ops.add(tok.string)
    return ops, opnds, len(distinct_ops), len(distinct_opnds)


def strip_fstring(seg: str) -> str:
    m = re.match(r"(?i)^[rfb]*('''|\"\"\"|'|\")", seg)
    if not m:
        return seg
    q = m.group(1)
    return seg[m.end(): len(seg) - len(q) if seg.endswith(q) else len(seg)]


LINT = [
    (re.compile(r"#\s*(?:(ruff|flake8)\s*:\s*)?noqa\b(?:\s*:\s*([A-Za-z0-9, ]+))?", re.I),
     lambda m: ("noqa", "file" if m.group(1) else "ignore", m.group(2))),
    (re.compile(r"pylint\s*:\s*(disable-next|disable|enable|skip-file)\b(?:\s*=\s*([\w\-, ]+))?"),
     lambda m: ("pylint", m.group(1), m.group(2))),
    (re.compile(r"type\s*:\s*ignore\b(?:\[([^\]]*)\])?"), lambda m: ("mypy", "ignore", m.group(1))),
    (re.compile(r"#\s*mypy\s*:\s*([\w\-=, ]+)"), lambda m: ("mypy", m.group(1).strip(), None)),
    (re.compile(r"pyright\s*:\s*ignore\b(?:\[([^\]]*)\])?"), lambda m: ("pyright", "ignore", m.group(1))),
    (re.compile(r"#\s*pyright\s*:\s*(basic|standard|strict)\b"), lambda m: ("pyright", m.group(1), None)),
    (re.compile(r"pragma\s*:\s*no\s*(cover|branch)\b"), lambda m: ("coverage", f"no {m.group(1)}", None)),
]


def lint_directives(comment: str):
    for rx, make in LINT:
        m = rx.search(comment)
        if m:
            yield make(m)


def emit_flow_and_quality(fe, layers: set[str], emit, api) -> None:
    FlowExtractor(fe, api, layers, emit).run()
