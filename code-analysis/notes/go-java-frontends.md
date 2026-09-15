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

**The Java project model is asked of the build, not guessed**: Maven through a
core extension, compiled on first use against the Maven that loads it
(`-Dmaven.ext.class.path`), which writes the reactor — source roots, release
level, declared dependencies, classpaths resolved with their scopes — once the
projects are read, then stops the build so no plugin runs; Gradle through an init
script that prints each project's source sets and resolved classpaths as JSON; a
plain directory as a fallback. Why an extension rather than plugin goals:
`../decisions.md` 2026-09-14, amended 2026-09-15. A build tool that fails degrades to no classpath and says
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

## The Go refs layer, as built

- **What is no reference**: the basic types, `any`, `comparable`, `nil`, `true`,
  `false`, `iota` — Go spells them as identifiers, TypeScript as keywords, and
  counting them would swamp `ref`. `error` is a reference (`lib#error`), and so
  is every builtin call (`lib#append`, `lib#panic`), with a `call_site`.
- **A conversion `T(x)` is no call**: a `type` ref at position `assertion`. A
  composite literal `T{…}` is ref kind `new`, with no `call_site` — nothing runs.
- **Dispatch**: a concrete method is `static` (Go resolves it at compile time);
  a method of an interface, or of a type parameter's constraint, `virtual`; a
  project variable holding a function `indirect`, except one adopting a literal
  (`f := func…`), which is that function and `static`, as in TypeScript.
- **`implements` is asked, not read**, of every project named type against the
  project's interfaces and each outside interface the project names — so
  `lib#error` and `io.Reader` appear when code mentions them. Types and
  interfaces from different packages are compared in a package view whose
  imports reach both, since a test variant type-checks its package again.
  Generic types and interfaces are skipped: uninstantiated, they satisfy nothing.
- **A method promoted from an embedded type is named where it is declared**, in
  `overrides` and `call_site` alike: the declaration that runs.
- Code-facts drops exact duplicate rows, so two `unresolved_ref`s of one name on
  one line are one row.

## The Go flow layer, as built

- **`defer` is one `finally` node per function that defers**, around the whole
  body: a `defer` registers dynamically, so no lexical frame fits. The body's
  end, every `return` and every panic enter it. It has a `back` edge to itself
  when more than one call may be deferred (several `defer`s, or one in a loop),
  since the deferred calls drain in a loop. The deferred call's `call_at` is that
  node; its function value and arguments are evaluated at the `defer` statement.
- **A panic may resume at `exit`** when a deferred call may `recover`. `recover`
  works only in the deferred function itself, so a literal or a project function
  is read for a direct call; the standard library and builtins cannot; anything
  else (a function value, an outside module) may.
- **Implicit panics exist only in functions that defer**, as TypeScript's exist
  only inside `try`: outside one, a panic leaves the function and no edge is
  needed to say so.
- **`select` evaluates every channel operand at its own node**, then tests its
  cases in turn like a switch; without `default` the last case is taken when
  none before it is, and is no decision. A receive's assignment is at its case.
- **A label is a node only when a `goto` targets it.** `fallthrough` is carried
  exactly from a clause ending in it (the one place Go allows it).
- A type switch's variable is defined at the switch; a channel `range` is a
  receive at the loop head; package-level initializers run in their file's
  `<module>` in source order, where Go orders them by dependency.
- `fn.kind` is `function`, `method`, `function_expression` (a literal) or
  `module`. Cognitive complexity follows the TypeScript layer's counting, with
  `goto` and labeled jumps +1 each.

## Go's own facts, and how the library reads them

Added after the flow layer, on an audit of where a shared shape means something
else in Go (the user's question: is a TypeScript module a Go package?). It is
not — a TypeScript module is a file, a Go package a directory.

**Facts.** `entry_point` (main, init, and what `go test` runs, by its naming and
signature rules — `Testlower` and `TestWrong(x int)` are not tests);
`field_tag`, a row per `key:"value"` pair, since tags couple code to encoders
and databases by reflection, which no reference shows; `typed_const` with value
and `iota` — the shape of an enum, not a verdict; `module_directive` for the
rest of go.mod; `symbol.form` `pointer_receiver` / `value_receiver`;
`implements.pointer` for a type only its pointer satisfies; and generic types in
`implements` and embedding, instantiated with their own type parameters (the
type a receiver `T[X]` sees — `types.Implements` is unspecified on an
uninstantiated generic).

**Library.** `units.dl` gains granularity `-3`, the namespace (`file.namespace`:
a Go package, a Python module), so coupling and cohesion rules run per package;
`modgraph.dl` gains `namespace_dep` and its cycles; `coupling_kinds.dl` knows
Go's basic type names. Bench on tsdl, trunk against these: every digest of
`orient`, `modgraph`, `coupling`, `cohesion` and `coupling_kinds` identical —
TypeScript files have no namespace.

**Traps for `reference/go.md`**, when it is written:

- nothing calls an `entry_point` — reach from them before calling anything unused;
- `in_cycle` finds cycles between files of one package, which Go allows; ask
  `in_namespace_cycle` (always empty for Go packages — the compiler forbids it —
  but the right question in Java);
- `dit` and `noc` are 0 for every struct: embedding is `embeds`, not `extends`;
- a promoted field read through the outer struct (`s.name` from `base`) is
  `member_access` on the embedded type, so the outer struct's `lcom4` misses it;
- `module_lcom4` is per file, and a package spans files: ask cohesion at `-3`;
- `imports` rows within a package are `implicit`; `kind != implicit` is what the
  source wrote.

## The Go quality layer, as built

- **`diagnostic`** is what go/packages reported — the go command's listing
  errors, parse errors, type errors — once each, though a test variant
  type-checks its package again. The go command echoes a failed compile as
  `# <package>` and the compiler's lines; where the type checker has reported
  errors for that package the echo is dropped, since it only repeats them (a
  probe found both rows). Go numbers no diagnostics: `code` is 0.
- **Comments are read from the file as written**: `//nolint[:rules]`
  (golangci), `//lint:ignore` / `//lint:file-ignore` (staticcheck), `#nosec`
  (gosec), directives in a new `compiler_directive` (`//go:…`, cgo's
  `//export`, `//line` — a `//` with no space, as the toolchain requires), and
  markers.
- **`ignored_error`** (new): a call with a result implementing `error`, made as a
  statement, assigned to `_`, deferred, or started with `go`. `fmt.Println` is
  in it — the facts do not decide which dropped errors matter.
- **`any_site`** is `any` or `interface{}` written, except as a type-parameter
  constraint, and a call returning an empty interface (`recover()` among them).
- **`assertion`** is `x.(T)` (`type_assert`) and a type switch (`type_switch`);
  a conversion is no assertion — it cannot fail at run time.
- **`throw_site` is `panic(…)`**, typed by the panicked value's named type;
  **`catch_site` is `recover()`**: `binds` when its value is used, `empty` when
  the function is nothing but `recover()`, `rethrows` when that function panics.
- **`literal`** excludes import paths, struct tags and array lengths in types.

## The Go dataflow layer, as built

- **Lowered from SSA**, built by x/tools over every loaded package: from syntax
  where it type-checked, from type information for dependencies and ill-typed
  packages. A package whose SSA build panics loses its dataflow facts, says so on
  stderr, and the extraction goes on. `ssa.GlobalDebug` keeps `DebugRef`
  instructions, which tie SSA registers (`<fn>$t3`) back to the variables they
  are read or written through: a named variable's symbol id receives each.
- **A pointer to a struct or array is that object**; any other pointer's content
  is field `*`. A field or element address resolves to its object and field, and
  an inline embedded struct's fields are the outer object's — which also makes a
  promoted method's receiver, handed the outer object, read the right fields.
- **Closures** are `function` allocations whose captured variables are fields
  `free0`, …, loaded through the literal's `$this`; a call through a function
  value names it both `callee_var` and `receiver`. A method's receiver parameter
  is its `this_var`.
- **Package-level variables are `cell` allocations** and every project function
  a `function` allocation, both in their file's `<module>`; a package initializer
  is attributed to the file of each instruction.
- A multiple return is one `$ret` slot; `append` and `copy` move elements;
  channels are field `<chan>`; `panic` stores `$thrown` and `recover()` reads it.
- **Not modelled**: a field address escaping its function (`f(&s.x)`), the
  address of a package-level variable passed along, a method value's receiver
  (`g := x.M; g()`), a promoted method through an embedded pointer.
- A `DebugRef`'s variable was first written without a `var` row; `checks.dl`
  caught it on the first extraction.

## The Go dogfood: caddy

Chosen for shape: interface-saturated (a module system matched structurally),
goroutines, build-tagged platform files, deep history. It has no go.work and no
cgo. 107.5k lines in 350 files (130 tests), 2,684 commits. Run from the packaged
bundle, following `SKILL.md` from step 2.

**Calibration.** Extraction with every layer and git: 23.4 s, 723 MB peak (22.0 s
in the Go frontend), 737k facts — `implements` 976, `overrides` 1,045, `embeds`
96, `extends` 8, `entry_point` 748, `excluded_file` 20 (`gofuzz` tags and platform
names), `ignored_error` 573. The bench: `checks.dl` 3.4 s, `flow.dl` 9.4 s,
`dominators.dl` 9.7 s, `pointsto.dl` 53.7 s at 840 MB, every other library under
3 s.

**Defects found, each fixed with a test:**
- `GOOS`/`GOARCH` in the environment built the frontend for that platform, and
  it could not run.
- `unsafe`'s functions are `types.Builtin`s with a package, so `unsafe.Sizeof`
  was an unresolved call in type-checked code.
- `implements` missed types declared in `_test.go` files — 5 of 144
  `caddy.Module` implementations. A test file's type exists only in the view
  `go test` compiles, which shares its package's import path, and the view was
  chosen by path. P4-go had never generated a test file; it does now.
- `reference/go.md` was wrong three times where the verification module had no
  case: interface embedding is `extends`; a `_windows.go` exclusion has no
  detail; a function value computed in place (`wrap(r)(next)`) is unresolved in
  type-checked code, and stays so, since `checks.dl` wants a callee on every
  `indirect` call.

**What held, and how it was checked:**
- `in_cycle`: 71 files in 11 packages; no cycle edge crosses a package and none
  is an explicit import; `in_namespace_cycle` is empty.
- `implements` recall for `caddy.Module`: 144, against a source grep's 143 plus
  one its regex missed.
- A negative probe: of 504 unexported production functions, none is uncalled —
  73 are reached only through `call_edge_pt` (handlers and callbacks passed as
  values), 25 only through a non-call `ref`. The project lints with `unused`, so
  zero is right; over `called` alone 98 would look dead.
- `packages.dl`: `unused(_, _, prod)` is empty; `only_in_tests` names testify and
  `prometheus/client_model`, which only `admin_test.go` imports.
- `ignored_error` is mostly `Close` (63 through `io.Closer`), `strings.Builder`
  writes and `fmt.Print*`; a deferred `countRequest(-1)` under
  `//nolint:errcheck` is intended, so cross with `lint_directive` first.

**What an analysis would report**, checked in the source:
- `modules/caddyhttp` and `modules/caddypki` depend on `cmd` (an `sdp_violation`
  at `-3`): modules register CLI subcommands from `init` through
  `caddycmd.RegisterCommand` — a plugin pattern, not layering drift.
- Hot spots, summed cyclomatic × revisions in production files:
  `caddyconfig/httpcaddyfile/httptype.go` (444, 160),
  `modules/caddyhttp/reverseproxy/caddyfile.go` (436, 98),
  `modules/caddyhttp/matchers.go` (311, 126), `modules/caddyhttp/server.go`
  (235, 179).
- `hidden_coupling` from `admin.go` (12 commits with `caddyhttp.go` and with
  `tls.go`) rests on API sweeps just under the 50-file bulk limit — the module
  interface's early refactors, lint and licence passes. Narrow `--git-since`
  before reporting such a pair.

Not exercised by caddy: go.work, cgo, `dot` imports.

## The Java structure layer, as built

- **One javac task per source set** (main, test). Its classpath is the build's;
  its source path is its own roots, main's for a test set, and each sibling
  module's, so a reactor needs no jar built. Every set is parsed before any is
  analysed and ids are assigned over every file in path order, keyed by
  declaration offset — a sibling's declaration read from the source path is the
  same id. `-proc:none` for now (see *Not yet*).
- **Ids from syntax, facts from javac.** Ids are claimed from the parse, keyed by
  file, offset and tree kind, before anything is analysed. Once a source set is
  analysed its elements set what the compiler knows: modifiers written or
  implied (an interface's members, an enum's constants, nested enums and
  records), what a variable in an enum or a record is, varargs, Javadoc as
  `DocTrees` reads it, `main` by its resolved signature, and test methods by
  their annotations' resolved types. Where analysis fails, a row keeps what was
  written.
- **Kinds**: record and enum are `class`, an annotation type `interface`; enum
  constants `enum_member`; record components `property` / `record_component`,
  public as their accessor is; initializers `static_block` named
  `<static@L:C>` / `<instance@L:C>`; constructors `constructor`; lambdas
  `function` `<lambda@L:C>`; anonymous classes `<class@L:C>`.
- **`imports`** is a row per file each statement is *used* to reach. A
  single-type or static import always names its type's file (unused: `runtime`
  false); an on-demand import reaching nothing is a `target_dir` row. A name
  imported by a single-type import is never its package's `*`. `runtime` is false
  when every use is an inlined constant (its qualifier included) or a Javadoc
  link. `implicit` covers same-package, fully qualified and Javadoc-only
  references to a file no import of this file reaches, at the first reference by
  the name's own offset.
- **Outside the root**, `target_package` is the JDK module (`java.base`, and
  `builtin`) or the coordinate the build resolved the jar as, else one read off an
  `.m2` or Gradle-cache path; symbols are `ext:<package>#Outer.Inner`.
- **`entry_point`**: `public static void main(String[])`, instance mains from
  release 25, static initializers (`init`), and methods carrying a JUnit or
  TestNG annotation, or one JUnit 5 composes with `@Testable` (`test`). An
  annotation whose type did not resolve — no test jar on the classpath — is
  judged by its simple name, the one guess left.
- **Degradation**: when Maven cannot read the reactor its modules come from the
  poms' `<modules>` with conventional roots; when Gradle cannot, every directory
  holding a build script. Each says so on stderr, which code-facts now forwards
  from every frontend (Go's warnings were dropped until then).
- **Not yet**: annotation processors — the extractor-mirrors-its-build rule says
  run the build's, and without them Lombok's members and generated sources are
  absent, which the refs layer will make visible; `module-info.java` as symbols;
  source roots a plugin adds (the extension runs before any plugin); files the
  build's includes and excludes drop, as `excluded_file`.
- Byte-comparing fixture output for the Java schema columns found Go's dataflow
  allocation sites varying run to run (`../bugs/resolved/007`).

## The Java refs layer, as built

- **`ref`** is from the innermost named declaration: a method, constructor,
  lambda, class, a field (its initializer), an initializer block, else the
  file's `<module>`. Kinds: `call` for a method name being invoked; `new`; `type`;
  `extends` and `implements` for the written clause (an interface's `extends`
  list is `extends`); `decorator` for an annotation; `value` for a method or
  constructor reference and a class literal; `read`, `write` and `readwrite` by
  assignment. A type naming a static member's owner (`Base.unit()`) is `type` at
  position `other`.
- **`call_site`**: constructors (`new`, `super(…)`, `this(…)`), static and private
  methods and `super.m()` are `static`; any other method is `virtual` at the
  declaration javac resolved. That includes a functional interface's method
  (`f.apply(x)`): a lambda has no `overrides` row, so what it runs is
  points-to's question. `new` of a class declaring no constructor names the
  class; of an anonymous class, the anonymous class.
- **What the compiler writes is no one's.** A default constructor or a record's
  accessors (an element whose origin is not explicit) makes no call site, ref or
  type, and a reference to one names its type.
- **`overrides`**: for each method, the members of each *direct* supertype
  (`Types.directSupertypes`, `Object` included for a class) that
  `Elements.overrides` says it overrides. So `toString()` overrides
  `ext:java.lang#Object.toString`, and a diamond gives a row per base. And a
  method a class inherits overrides its interfaces' methods there: in `class T1
  extends T0 implements I0`, `T0.m` overrides `I0.m` though `T0` never names
  `I0`. Without that row, a call through `I0.m` expanded to nothing that runs —
  P4-java's first case; Go's promoted methods are the same idea. A supertype's
  members are the ones it *inherits*: `Elements.getAllMembers(T1)` also lists
  `I0.m`, which `T0.m` overrides from `T1`, so a base another base overrides
  there is dropped — else `T2 extends T1` redeclaring `m` overrode `I0.m` too
  (P4-java's second finding, at case 27).
  One exception to javac's `Elements.overrides`: it counts an inherited method
  as implementing an interface's only when it is concrete, a rule for its own
  "does not override abstract method" error. An *abstract* method a class
  inherits is still what a call through the interface resolves to — the JVM's
  `getMethod` and the language agree — so it overrides the interface's method
  there, checked with `Types.isSubsignature` (P4-java's third finding).
  **`extends`/`implements`** are the written clauses; an anonymous class
  implements what it instantiates; `Object`, `Enum` and `Record` stay implicit.
- **`member_access`** covers fields and methods of project classes and
  interfaces. `via_this` is `this.x`, `super.x`, `Outer.this.x`, and an
  unqualified instance member — Java's implicit `this`, which cohesion counts.
- **`type_ref`** positions: `param` (method and lambda parameters), `return`,
  `property`, `variable`, `extends`, `implements`, `type_arg` (explicit type
  arguments of a call or `new`), `assertion` (a cast, `instanceof`), and `other`
  (a throws clause, a qualifier, a class literal). A type argument inside a
  declared type takes the declaration's position, as TypeScript's does.
- **`symbol_type`** is javac's `TypeMirror` text; a method's is its signature,
  `(int)java.lang.String`. `is_function` marks methods and values of a
  functional interface, `is_promise` a `Future` or `CompletionStage`, and
  `is_union` a multi-catch parameter. **`throws_decl`** is the written clause.
- **`unresolved_ref`** is a name javac left an error symbol for — not a member
  selected from an expression whose type is an error, and not a package segment.
- Found on the way: a lambda and its first implicitly typed parameter shared an
  id, so keys carry the tree kind now; and a field whose *type* did not resolve
  counted as unresolved itself.
- **Not yet**: the module path. How javac runs otherwise is the build's; see
  *Javac as the build runs it*.

## Javac as the build runs it

Why: `../decisions.md` 2026-09-15 (night ii).

- **The model carries each source set's compiler settings**: processor path,
  processors, `-proc`, encoding, compiler arguments, the processors' output
  directory, and each module's build directory. Maven reads them from
  maven-compiler-plugin — its configuration with the `default-compile` or
  `default-testCompile` execution's over it — and resolves
  `annotationProcessorPaths` through Maven's own `RepositorySystem`, with their
  dependencies, as the plugin does. Gradle reads its compile task's options, and
  passes `-proc:none` when the processor path is empty, as Gradle does.
- **Processing runs when the build's javac would**: a processor path, named
  processors, or `-proc:full`/`only`. From JDK 23 javac runs nothing it only
  *discovers* on the classpath (checked on JDK 26); before it, discovery counts.
- **Two passes.** Before any id is claimed, javac runs with `-proc:only` and
  `-s` the build's directory. The `.java` files written there *by this pass*
  join the source set; an older file is an earlier build's and is left alone.
  Analysis then runs the processors again, since Lombok rewrites trees there,
  with `-s` a throwaway directory. A generator's attempt to recreate a type
  already among the sources is a Filer error, which is swallowed.
- **Generated is where it lives**: every file under the module's build directory
  is `is_generated`, whether a processor or a plugin wrote it. Maven's other
  `generated-sources/*` directories join the roots; Gradle already lists its
  generated roots among a source set's directories. A sibling module's
  processor output is on the source path, as its sources are.
- **Arguments** pass through, except those choosing output (`-d`, `-s`, `-h`),
  paths, release and encoding; `--enable-preview` only with a release. If javac
  rejects them, the set is read without them, and code-facts says so.
- Tested with a processor built by the tests (`com.acme:gen`), run by a Maven
  build through `annotationProcessorPaths` and by a Gradle build through its
  `annotationProcessor` configuration; with a Latin-1 source under
  `project.build.sourceEncoding`; and with sources a plugin left under
  `target/generated-sources`.
- **Not yet**: the module path (`module-info.java` projects read on the
  classpath); a Lombok project on a real subject.

## Properties

Each language gets P1 (CFG against real traces), P2 (module graph over modgen's
model), P3 (cyclomatic = E − N + 2), P4 (hierarchies), P5 (points-to soundness
against execution) and P6 (determinism). The independent oracles:

- **P4-go** asks the running program, as a test so types can live in the test
  file: reflect says whether `*T` and `T` implement `I`, and each method called
  through reflect prints the declaration that ran.
- **P4-java** asks the JVM: each method called on an instance of each class,
  printing which implementation ran.
