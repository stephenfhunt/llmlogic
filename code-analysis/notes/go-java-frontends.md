# Go and Java frontends — design

Overflow for `../decisions.md` 2026-09-14. The Python frontend is the template
(`code-facts.md` § The Python frontend): a program in the target language's own
toolchain streams `{relation, row}` JSON lines; Node validates every row against
`src/schema.ts`; there is one writer, one git layer and one `lib/`. This note is
what Go and Java add to that, and why.

The brief, from the user: **the project's own toolchain is assumed present** (a Go
project has `go`, a Java project a JDK and its Maven or Gradle — the skill bundles
none of it), and **the facts stay maximalist**, so an agent has primitives to
explore with beyond what the library derives.

Order: Go first, then Java. Both get every layer, dataflow included.

## Resolution backends

**Go: `golang.org/x/tools` — `go/packages`, `go/types`, `go/ssa` — vendored.**
`go/packages` loads module-aware packages exactly as `go build` resolves them
(go.work, replace directives, build tags, test variants), and SSA is already the
three-address form the dataflow layer needs. The source checkout builds with a
module download; `package.sh` runs `go mod vendor` so the bundle builds offline,
as it vendors TypeScript. The frontend is compiled on first use with the toolchain
the *target* selects, so `go/types` reads the project's language version.
*Rejected:* standard library only (`go list -json` + `go/types` + `go/ast`) — a
hand-written package loader, and a dataflow lowering nobody else maintains.

**Java: the JDK's compiler API** (`JavacTask`, `Trees`, `Elements`, `Types`; only
`com.sun.source.*`, which the JDK exports). The checker that compiles the project
resolves its names — the TypeScript decision again. Compiled on first use with
`javac --release 17`; the target's own source level comes from its build.
*Rejected:* JavaParser's symbol solver (a third-party jar with weaker resolution),
JDT/jdtls (heavy, and a second compiler's opinion).

**The Java project model is asked of the build, not guessed**: Maven's reactor,
source roots and `dependency:build-classpath`; Gradle through an init script that
prints each project's source sets and resolved classpaths as JSON; a plain
directory as a fallback. A build tool that fails degrades to no classpath and says
so — library names then land in `unresolved_ref`, like a TypeScript checkout
without `node_modules` — and never aborts extraction.

## The schema: the library's vocabulary stays, new facts go in new columns

`lib/` joins on `symbol.kind`, so a construct maps onto an existing kind wherever
the library's meaning holds, and its own name goes in a new nullable
`symbol.form`:

| language | construct | `kind` | `form` |
|---|---|---|---|
| Java | record, enum | `class` | `record`, `enum` |
| Java | annotation type | `interface` | `annotation` |
| Java | static / instance initializer | `static_block` | `static_init` / `instance_init` |
| Go | named struct | `class` | `struct` |
| Go | defined non-struct type (`type Celsius float64`) | `class` | `defined_type` |
| Go | struct field, embedded field | `property` | —, `embedded` |

`visibility` gains `package` (Java package-private, a Go unexported member).

## The module graph stays file-to-file

`modgraph.dl` builds `dep` from `imports.target_file`. Go imports a *package*,
Java a *type*, and both reference names within a package with no import at all —
so a literal reading would leave every impact closure silently missing
same-package dependents. With `lib/` unchanged:

- A Go import spec is one `imports` row per in-root file of the imported package
  that this file references, specifier as written. A Java single-type import's
  `target_file` is the declaring file; an on-demand or static import is one row per
  file referenced. One statement becoming several rows is Python's `from pkg import
  mod` precedent.
- New nullable `imports.target_dir`: the imported package's directory, on every
  such row — and alone on a `_` import, which references no file but runs the
  package's `init`.
- A same-package or fully-qualified reference with no import is a row of new kind
  **`implicit`**, at its first reference, specifier the name referenced.
  `kind != implicit` recovers what was written.
- New kinds: `dot` (Go), `static_import` and `on_demand` (Java), `cgo`.
- `runtime`: always true in Go. In Java false when every reference to the target
  is an inlined compile-time constant, a Javadoc link, or none (an unused import).

*Rejected:* a file-level edge derived from `ref` in `modgraph.dl` gated on `lang`
(a per-language branch in the library, and no module graph at all under
`--layers` without refs); `imports` as written only (the silent impact hole).

**Namespaces**: new nullable `file.namespace` — the Go import path (`…_test` for an
external test package), the Java package, Python's dotted module. The `package`
relation stays the distribution unit: a go.mod module, a Maven artifact `g:a`, a
Gradle project.

**Dependencies**: `package_dep` gains nullable `scope`, the build's own word, and
`kind` keeps the library's four-way split:

| build | `prod` | `dev` | `peer` | `optional` |
|---|---|---|---|---|
| go.mod | direct `require` | `tool` | — | `// indirect` |
| Maven scope | compile, runtime | test | provided | `<optional>` |
| Gradle configuration | implementation, api, runtimeOnly | test* | compileOnly | — |

Java's `imports.target_package` is the Maven coordinate of the jar a class
resolved from, read off the `.m2` or Gradle-cache path.

## Structural interfaces become edges

`types.Implements` is checked for every project named type against every
interface the project mentions, external ones included (`io.Reader`), and each
match is `implements` plus `overrides` rows — so `callgraph.dl`'s virtual expansion
needs no change, and CHA for Go is complete rather than nominal. Embedding is a new
relation `embeds(outer, inner, pointer)`; a method shadowing a promoted one is an
`overrides` row.

## New relations, and old ones widened

| relation | layer | what |
|---|---|---|
| `concurrency_site(fn, file, line, kind)` | flow | `go`, `chan_send`, `chan_recv`, `select`, `defer`, `synchronized_block`, `synchronized_method` |
| `ignored_error(call_site, fn, file, line, how)` | quality | a Go `error` result discarded or assigned to `_` |
| `throws_decl(fn, type)` | refs | a Java method's declared checked exceptions |
| `excluded_file(path, reason)` | structure | what the build context skips: build tags, GOOS/GOARCH, Kotlin and Groovy in a Java build — the blind spot, countable |
| `compiler_directive(file, line, name, text)` | quality | `//go:generate`, `//go:embed`, `//go:linkname`, … |

Widened: `decorator` (Java annotations; new nullable `text` for the arguments),
`jsdoc_tag` (Javadoc, Go `Deprecated:`), `lint_directive.tool` (`golangci`,
`staticcheck`, `javac`, `sonar`, `checkstyle`), `throw_site`/`catch_site` (Go
`panic`/`recover`), `assertion` (Java casts; Go `x.(T)` as `type_assert`),
`any_site` (Go `any`; Java raw types as `raw`), `diagnostic` (nullable string
`key`: javac keys diagnostics by name), `extraction` (`go_version`,
`java_version`), `file.lang` (`go`, `java`).

## Control flow and dataflow

**The CFG is statement-level in both**, because P1 checks it against executions
and the flow facts point at statements. Go gets its own over `go/ast`, not SSA's
blocks: `fallthrough` and `goto` edges, labels, `select`, type switches,
range-over-func (the body is a closure). `defer` is modelled as `finally` — every
exit passes the function's deferred nodes; `panic` reaches `throw_exit`, and a
deferred `recover` may resume at `exit`. Java follows the TypeScript model, plus
switch expressions and `yield`, try-with-resources as Python's `with`, and
`synchronized` as an implicit try/finally.

**Dataflow is the same Doop-shaped facts.** Go lowers from SSA: named values map to
symbol ids through their identifiers, the rest are `<fn>$tN`; address-then-load
and address-then-store are `load`/`store`; a channel send or receive is a store or
load on field `<chan>`; an interface invoke is a `receiver` with `dispatch:
virtual`. Java lowers from javac trees as `layers/dataflow.ts` does; a lambda or
method reference is an `alloc` of kind `function`.

## Shared plumbing

- **Frontends write rows to a file, not a pipe.** `spawnSync`'s buffer capped
  output at 1 GiB, which a large Go or Java repository passes; Node now reads the
  file in chunks. Id counters chain through each frontend's closing `__counters__`
  row, so any number of frontends share the call-site and flow-node id-spaces.
- **Targets** are detected in order: a tsconfig; `go.mod` / `go.work`; `pom.xml` /
  `build.gradle(.kts)` / `settings.gradle(.kts)`; `pyproject.toml` or any other
  directory. `--lang` overrides for the next target (a plain Java directory).

## Properties

Each language gets P1 (CFG against real traces), P2 (module graph over modgen's
model), P3 (cyclomatic = E − N + 2), P4 (hierarchies), P5 (points-to soundness
against execution) and P6 (determinism). The independent oracles:

- **P4-go** asks the compiler: every generated (type, interface) pair as
  `var _ I = (*T)(nil)`, `go build`'s errors read per line.
- **P4-java** asks the JVM: each method called on an instance of each class,
  printing which implementation ran.
