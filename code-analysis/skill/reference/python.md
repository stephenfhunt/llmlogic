# A Python codebase, extracted

`./code-facts` reads Python too, into **the same relations, ids and library** as
TypeScript — so `typescript.md` §2–§4 (what is in the facts, the library, the
questions worth asking) apply as written. This file is what differs: what a
Python extractor without a type checker can know, what it cannot, and the traps
that follow. `bring-your-own.md` §4 (the name tier) matters more here than for
TypeScript.

## 1. Run it

```sh
./code-facts path/to/project -o /tmp/facts      # a directory, or its pyproject.toml
./datalog /tmp/facts/lib/checks.dl              # exit 1 = no violations
```

Needs Node.js ≥ 22.18 and Python ≥ 3.11 (`CODE_FACTS_PYTHON` picks the
interpreter). A directory without a `tsconfig.json` is read as Python; give a
tsconfig and a Python root together for a mixed repository. Every `.py` and
`.pyi` below the target is read, except virtualenvs, `build`, `dist` and dot
directories; `--exclude` drops more. A module's name climbs directories while
they hold an `__init__.py`, so a `src/` layout needs nothing special. A 21.6k-line
project extracts in 4.3 s to 93k facts.

## 2. What the facts say — where Python differs

- **Imports.** Nothing is erased, so every import is `runtime` — except under
  `if TYPE_CHECKING:`, which is `kind: type_only, runtime: false`.
  `from pkg import mod`, where `mod` is a submodule, is two rows (the package
  runs too). `importlib.import_module("literal")` is `dynamic`. `builtin` is the
  standard library. A package's `__init__` importing its own submodules gets no
  row to itself.
- **Exports** are a literal `__all__`, else the module's public (no leading `_`)
  declarations.
- **Symbols.** `self.x = …` in a method declares property `x`; class-level names
  are properties too. `@property` is a `getter`, `@x.setter` a `setter`,
  `__init__` the `constructor`. `visibility` follows the naming convention: `_x`
  protected, `__x` private. `f = lambda …` *is* `#f`. `param` has no row for a
  method's bound `self` / `cls`, and `fn.params` does not count it.
- **Calls and names resolve by Python's scoping** (LEGB, `global`, `nonlocal`)
  and through imports, re-export chains and `from m import *`. An attribute
  resolves when its receiver's type is known: a module, a class, `self` / `cls`,
  `super()`, a parameter or local whose **annotation** names a project class, or
  a local assigned from a project class's constructor. Anything else is
  `dispatch: unresolved`, with `callee_name` kept. `extends` and `overrides`
  follow Python's C3 method resolution order.
- **Types are annotations as written.** `symbol_type.text` is the annotation's
  source; with no annotation there is no row. Nothing is inferred.
- **Flow.** The CFG follows `typescript.md`'s model, plus: a loop's `else:` runs
  when its test fails and not after `break`; `except` clauses are `catch` nodes
  tested in order (`on_false` to the next, and past the last typed one the
  exception propagates); a `try`'s `else:` runs outside the handlers but inside
  `finally`; `with` is a `try/finally` whose exit may resume below the block
  (`__exit__` can swallow the exception); `match` cases do not fall through, and
  a guard is a `cond` of its own. Comprehension variables belong to the
  enclosing function.
- **Cyclomatic complexity counts like ESLint**, for parity with TypeScript: each
  `if`/`elif`, loop, `except`, `case` and guard, **and** each `and`/`or` operand
  after the first, each conditional expression, and each comprehension `for` and
  `if`. mccabe (ruff `C901`, flake8) counts none of the last three and folds a
  nested function into its parent — so these numbers run higher than a flake8
  threshold expects. Set those differences aside and they agree: on the project
  above, 1116 of 1117 functions equal ruff's count. Compare within the codebase.
- **Quality.** `lint_directive` reads `# noqa` (`# ruff: noqa` is directive
  `file`), `# pylint: disable=…`, `# type: ignore[…]` (tool `mypy`),
  `# pyright: ignore[…]` and `# pragma: no cover`. `any_site` is `Any` written in
  an annotation (`explicit`) and every unannotated parameter (`implicit_param`)
  — on unannotated code it counts parameters, which makes it an annotation
  coverage measure. `assertion` is `typing.cast`. `floating_promise` is a call
  to a project `async def` whose coroutine is discarded. `literal` leaves out
  docstrings, annotations, dict keys and `__all__`. `diagnostic` is syntax
  errors only.
- **Not produced:** the dataflow layer (`var`, `assign`, `alloc`, `load`,
  `store`, `formal`, `actual`, …), `implements`, `ts_directive`, `jsdoc_tag`.

## 3. The library over Python facts

Times on the project above (93k facts), including the import:

| file | time | over Python |
|---|---|---|
| `checks.dl` | 4.0 s | run directly |
| `modgraph.dl`, `callgraph.dl`, `callreach.dl`, `packages.dl`, `cochange.dl` | 0.3–0.9 s | as for TypeScript; the call graph has the holes of §4 trap 3 |
| `coupling.dl`, `cohesion.dl`, `metrics.dl`, `dominators.dl` | 3–5 s | as for TypeScript |
| `coupling_kinds.dl` | 11 s | `content` is also reaching into another class's `_x` (from outside it and its subclasses); `data`, `stamp` and `control` need **annotations** — an unannotated parameter is neither primitive nor a record, so a call through it is classified by nothing |
| `flow.dl` | 11 s | as for TypeScript |
| `pointsto.dl`, `taint.dl` | — | **empty**: there is no Python dataflow layer. "Untested exports" goes over `call_edge_lexical` plus the name tier, and says so |

## 4. Traps specific to Python facts

1. **Distribution names are not import names.** `PyYAML` is imported as `yaml`,
   `beautifulsoup4` as `bs4`, `Pillow` as `PIL`. Names are normalized (case,
   `-` / `_` / `.`) but not mapped, so `packages.dl` reports `undeclared yaml`
   *and* `unused pyyaml`. Pair them up before reporting either.
2. **A tool is declared and never imported.** `ruff`, `black`, `mypy` and pytest
   plugins come out `unused`; so does a dependency the code really stopped
   using. Only the second is a finding — on the project above, `anthropic` was,
   and `ruff` was not.
3. **Unresolved calls are the common case in unannotated code** — 44% of
   sqlparse's call sites, 24% of the annotated project above. Before saying
   "nothing calls X", count `call_site(dispatch: unresolved, callee_name: N)`
   for X's name: the name tier over-approximates what the call graph misses.
4. **What Python decides at run time is invisible:** `getattr` / `setattr` with
   computed names, `__getattr__`, monkeypatching, `exec`, `importlib` with a
   non-literal name, and decorators that register or replace a function. A
   function reached only by a framework — a pytest test, a `@app.route`
   handler, a CLI command, a `__main__` block — is called by nothing in the facts
   (`decorator` names what wraps it). Name your entry points.
5. **Module names collide** when two files have one name outside any package
   (two `scripts/main.py`-like files in sibling directories): an import of that
   name resolves to the first. A directory without `__init__.py` is a namespace
   package with no file of its own, so an import of it targets nothing.
6. **Tests are in the facts** — `tests/`, `test_*.py`, `*_test.py` and
   `conftest.py` are `is_test`. Test modules are many independent functions, so
   `module_lcom4` ranks them highest: filter them out of cohesion questions.
7. **Columns are UTF-8 byte offsets** on lines with non-ASCII text (Python's
   own), not characters.

## 5. Verify before you believe

As for TypeScript: the engine is exact about the facts, and whether the facts
say what you think is checked in the source. On sqlparse the facts answer the
experiments' four static-analysis questions exactly as an independent
`ast`-based answer key does; on the project above, each finding quoted in this
file was opened and confirmed.
