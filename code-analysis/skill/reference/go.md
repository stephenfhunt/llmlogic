# A Go codebase, extracted

`./code-facts` reads Go too, into **the same relations, ids and library** as
TypeScript — so `typescript.md` §2–§4 (what is in the facts, the library, the
questions worth asking) apply as written. Go's own type checker resolves every
name, so the call graph is resolved, not guessed. This file is what differs: how
Go's constructs map onto the shared relations, the facts only Go has, and the
traps that follow.

## 1. Run it

```sh
./code-facts path/to/module -o /tmp/facts       # a directory holding go.mod or go.work, or the file
./datalog /tmp/facts/lib/checks.dl              # exit 1 = no violations
```

Needs Node.js ≥ 22.18 and Go ≥ 1.26 on `PATH`. The first run compiles the
extractor's Go part with the toolchain the project selects (its `go` and
`toolchain` lines, as `go build` would), so the project's language version is
the one read. A `go.work` reads every module it `use`s. The packages read are
what `go list ./...` finds, with their tests — so `vendor/`, `testdata/` and
directories starting `.` or `_` are out; `--exclude` drops more. The build
context is the environment's: `GOOS`, `GOARCH` and `GOFLAGS=-tags=…` choose
another. A directory holding both a `tsconfig.json` and a `go.mod` is read as
TypeScript; `--lang go` before it reads it as Go.

## 2. What the facts say — where Go differs

- **Files, packages, namespaces.** A `package` row is a module; `file.package`
  is its module path. The Go package is `file.namespace`, its import path — an
  external test package (`package foo_test`) is `<import path>_test`. Each
  package is a `symbol(kind: namespace)` with id `<dir>#<package>` (the root
  directory is `.`), or `<dir>#<package_test>`. `is_test` is `_test.go`, which is
  exact; `is_generated` reads the `// Code generated … DO NOT EDIT.` header, and
  names like `*.pb.go`.
- **Imports are file to file, as `modgraph.dl` needs.** An import of a project
  package is a row for each file of that package this file refers to, with
  `target_dir` naming the package's directory (a `_` import is `kind:
  side_effect` with `target_dir` only). A name from another file of the same
  package, which Go uses without an import, is a row of kind `implicit`: its
  `specifier` is the name, its line the first use. `dot` is `import . "p"` and
  `cgo` is `import "C"`. Outside the project, `target_package` is the module
  providing the package, and `builtin` marks the standard library. Every import
  is `runtime`.
- **Symbols take the shared kinds, and `symbol.form` keeps Go's word.** A struct
  is `class` (form `struct`), and so is every other defined type (form
  `defined_type`) — both carry methods. An interface is `interface`, a field
  `property` (an embedded field has form `embedded`), and a method's form is
  `pointer_receiver` or `value_receiver`. A method's id is under its type,
  `shapes.go#Circle.Area`, even when another file of the package declares it —
  `symbol.file` says which. `exported` is a capitalised package-level name, and
  members have `visibility` `public` or `package` by the same rule. Outside the
  project, ids are `ext:<import path>#Name`; the universe's are `lib#error`,
  `lib#append`.
- **References.** A conversion `T(x)` is no call: a `type` ref. A composite
  literal `T{…}` is ref kind `new`, with no `call_site`. The basic types, `any`,
  `nil` and `iota` are no references; `error` and every builtin call are. A
  concrete method call is `dispatch: static`; a call through an interface or a
  type parameter's constraint is `virtual`; a call through a function value is
  `indirect`. A method promoted from an embedded type is named where it is
  declared: `t.Fatal` in a test calls `ext:testing#common.Fatal`.
  `unresolved_ref` and `dispatch: unresolved` occur only in code that does not
  type-check, and `diagnostic` says why.
- **`implements` is computed**, since Go never writes it: every project named
  type (a generic one with its own type parameters) is checked against every
  interface the project declares or names from outside (`error`, `io.Reader`).
  `pointer: true` means only `*T` satisfies it. `overrides` follows, so
  `call_edge` expands an interface call to each implementation. Embedding is
  `embeds(outer, inner, pointer)`, not `extends`, and a method shadowing a
  promoted one is an `overrides` row.
- **Facts only Go has:**
  - `entry_point`: `main`, `init`, and what `go test` runs (`test`, `benchmark`,
    `fuzz`, `example`, `test_main`), by its naming and signature rules;
  - `field_tag`: a row per `key:"value"` pair in a struct tag;
  - `typed_const`: a constant's named type, value, and whether it uses `iota`;
  - `module_directive`: the rest of go.mod (`go`, `toolchain`, `replace`,
    `exclude`, `retract`, `godebug`);
  - `excluded_file`: the files the build context skipped, with the constraint;
  - `concurrency_site`: `go`, `defer`, channel send and receive, `select`;
  - `ignored_error`: a call returning an `error`, made as a statement
    (`discarded`), assigned to `_` (`blank`), `deferred`, or started with `go`;
  - `compiler_directive`: `//go:generate`, `//go:embed`, `//go:linkname`, cgo's
    `//export`, `//line`.
- **Dependencies are go.mod's.** A `require` is `prod`, an `// indirect` one is
  `optional`, and a `tool` directive is `dev`; `package_dep.scope` keeps which
  (`require`, `indirect`, `tool`).
- **Flow.** The CFG follows `typescript.md`'s model, plus:
  - `defer` is one `finally` node per function that defers, entered from the
    body's end, every `return` and every panic. It has a `back` edge to itself
    when more than one call may be deferred.
  - A panic may resume at `exit` when a deferred call may `recover`. Implicit
    panics are edges only in functions that defer.
  - `select` evaluates its channel operands at its own node, then tests its
    cases in turn.
  - `goto` and `fallthrough` are node and edge kinds, and a label is a node only
    when a `goto` targets it.
  - Package-level initializers run in their file's `<module>` in source order,
    where Go orders them by dependency.
- **Cyclomatic complexity** counts each `if`, `for`, `range`, `case` (not
  `default`) and `&&` / `||`. `select` counts the same way, except that its
  last case counts nothing when there is no `default`, since that case is
  taken. Cognitive complexity adds one for each `goto` and labeled jump.
- **Quality.**
  - `lint_directive` reads `//nolint[:linters]` (golangci),
    `//lint:ignore` / `//lint:file-ignore` (staticcheck) and `#nosec` (gosec).
  - `diagnostic` is `go list`, parse and type errors, all `code: 0`.
  - `any_site` is `any` or `interface{}` written (`explicit`, but not as a
    constraint), and a call returning one (`call_result`).
  - `assertion` is `x.(T)` (`type_assert`) and a type switch (`type_switch`).
  - `throw_site` is `panic(…)`. `catch_site` is `recover()`: `binds` when its
    value is used, `empty` when the function is nothing but `recover()`,
    `rethrows` when that function panics.
  - `jsdoc_tag` holds `Deprecated:` notes.
  - `literal` leaves out import paths, struct tags and array lengths.
- **Dataflow** is lowered from SSA, so `pointsto.dl` and `taint.dl` run.
  - Named variables keep their symbol ids, and temporaries are `<fn>$t3`.
  - A pointer to a struct or array is that object; any other pointer's content
    is field `*`.
  - A closure is a `function` allocation whose captured variables are fields
    `free0`, …; a package-level variable is a `cell` allocation.
  - A channel's contents are field `<chan>`. `panic` stores `$thrown`, and
    `recover()` reads it.
- **Not produced:** `extends`, `decorator`, `ts_directive`, `floating_promise`,
  `await_at`, `yield_at`.

## 3. The library over Go facts

What each library computes is in [`typescript.md`](typescript.md) §3; this is
what differs over Go facts.

| file | over Go |
|---|---|
| `checks.dl` | run directly |
| `modgraph.dl` | `dep` includes `implicit` rows, so `in_cycle` finds cycles among one package's files, which Go allows (§4 trap 2); `namespace_dep` and `in_namespace_cycle` are between packages; `runtime_dep` is `dep` |
| `callgraph.dl`, `callreach.dl`, `flow.dl`, `dominators.dl`, `cochange.dl` | as for TypeScript |
| `coupling.dl`, `cohesion.dl` | ask per package, `G = -3` (the namespace); per file (`G = -1`, `module_lcom4`) splits a package where Go does not (trap 5) |
| `metrics.dl` | `dit` and `noc` are 0 (trap 3); `wmc`, `rfc`, `fan_in`, `fan_out` as for TypeScript |
| `coupling_kinds.dl` | Go's basic types are primitive, so `data` and `stamp` classify; `content` is empty, since another package cannot reach an unexported name |
| `packages.dl` | against go.mod; `unused` lists every `// indirect` requirement as kind `optional` — nothing imports those by definition, so ask `unused(P, D, prod)` |
| `exports.dl` | give `entry/1` every file of the packages other modules import — for a library, everything outside `internal/`; a command publishes nothing |
| `pointsto.dl`, `taint.dl` | as for TypeScript, over what §4 trap 9 leaves modelled |

## 4. Traps specific to Go facts

1. **Nothing calls an entry point.** `main`, `init` and tests are called by the
   runtime and `go test`, and a method satisfying an outside interface by the
   library that takes it — `String` by `fmt`, `ServeHTTP` by `net/http`. A type
   passed to `fmt.Println` has no `implements` row unless the code names
   `fmt.Stringer`. Reach from `entry_point` before calling a function unused,
   and look for a method's name among interfaces outside the project.
2. **Files of one package may import each other in a cycle, and that is
   normal.** Go packages cannot form cycles, so ask `in_namespace_cycle`; a
   file-level `in_cycle` inside a package is not a finding. The `imports` rows
   between a package's files are `kind: implicit`; `kind != implicit` is what the
   source wrote.
3. **`dit` and `noc` are 0 for every type**: embedding is `embeds`, which is
   composition, not inheritance. Ask `embeds` for how types are built and
   `implements` for what can stand in for what.
4. **A promoted member is accessed on the type that declares it.** Where
   `Circle` embeds `base`, `c.name` is `member_access` with owner `base`, so
   `Circle`'s `lcom4` counts a method that uses only `base`'s fields as a group
   of its own. Check `embeds` before splitting a type `lcom4` says is two.
5. **A package is many files, and a type's methods may be spread across
   them.** `module_lcom4`, per-file coupling and `coupling_kinds.dl`'s file
   pairs treat each file as a module. Ask cohesion and coupling at `G = -3`.
6. **The build context hides files.** A file for another OS or behind a build
   tag is an `excluded_file` row and nothing else — its references, calls and
   imports are in no fact. Before a negative, count `excluded_file`. To see a
   variant, extract again with its `GOOS` or `GOFLAGS=-tags=…`.
7. **`ignored_error` is every dropped error.** `fmt.Println`, a deferred
   `Close`, and `(*strings.Builder).WriteString`, which never fails, are all in
   it. Filter by callee before ranking.
8. **Reflection is invisible.** `field_tag` says which fields an encoder or a
   database reads by name; `reflect`, `//go:linkname` and code that
   `go generate` would write are not in the call graph.
9. **Points-to is may-point-to, over the modelled subset.** It does not follow
   a field's address passed out of its function (`f(&s.x)`), a package-level
   variable's address passed along, a method value's receiver (`g := x.M`), a
   method promoted through an embedded pointer, or `unsafe`. There `pts` can
   miss; everywhere else it only over-approximates.
10. **Columns are byte offsets**, 1-based, on lines with non-ASCII text, and a
    tab counts one.

## 5. Verify before you believe

As for TypeScript: the engine is exact about the facts, and whether the facts
say what you think is checked in the source. Go leaves most to check where it
defers to the runtime: what an entry point, a library or reflection calls, and
what a build tag left out.
