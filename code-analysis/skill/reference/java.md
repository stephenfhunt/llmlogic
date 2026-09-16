# A Java codebase, extracted

`./code-facts` reads Java too, into **the same relations, ids and library** as
TypeScript — so `typescript.md` §2–§4 (what is in the facts, the library, the
questions worth asking) apply as written. The JDK's own compiler resolves every
name against the classpath the build resolves, so the call graph is resolved, not
guessed. This file is what differs: how Java's constructs map onto the shared
relations, the facts only Java has, and the traps that follow.

## 1. Run it

```sh
./code-facts path/to/pom.xml -o /tmp/facts       # or build.gradle(.kts), settings.gradle(.kts)
./code-facts path/to/project -o /tmp/facts       # a directory holding one of those
./code-facts --lang java path/to/src -o /tmp/facts   # plain sources, no build
./datalog /tmp/facts/lib/checks.dl               # exit 1 = no violations
```

Needs Node.js ≥ 22.18 and a JDK ≥ 17 — both `javac` and `java` — on `PATH`.

**Build the project first**, with its own build: `mvn -DskipTests test-compile`,
or `./gradlew testClasses`. Extraction reads the build's output as the build left
it, and two things exist only after one: sources a plugin generates, and the test
classes one module shares with another. Without them, names go unresolved in
exactly the places that matter, and the facts quietly get smaller (§4 trap 1).

The build is asked for the project model — source sets, the resolved classpath,
compiler arguments, encoding, release level, the module graph — through a Maven
core extension or a Gradle init script, both built on first use. javac then reads
each source set with those settings, and runs the annotation processors the build
configures (Lombok's members are in the facts, and what a processor generates is
extracted, `is_generated`). If Maven or Gradle cannot run, the model degrades to
what the build files say with conventional roots and no classpath; it says so on
stderr, and `diagnostic` fills with unresolved names.

Kotlin, Groovy and Scala files in a Java build are `excluded_file` rows and
nothing else. `--exclude` drops more paths.

## 2. What the facts say — where Java differs

- **Files, modules, namespaces.** A `package` row is a **build module** — a Maven
  or Gradle module, not a Java package — and `file.package` is its coordinate
  (`com.example:core`). The Java package is `file.namespace`, and is also a
  `symbol(kind: namespace)` with id `<dir>#<package>`. `is_test` is the build's
  own test source set, which is exact. `is_generated` is anything under the build
  output directory, plus the usual generated-code headers. `module_directive` is
  empty: a `module-info.java` is read as a file, but its `requires` and `exports`
  are not extracted, so the module graph is the build's, not the JPMS one.
- **Symbols take the shared kinds, and `symbol.form` keeps Java's word.** A class,
  enum and record are all `class`, with form `enum` or `record`; an interface and
  an annotation type are `interface`, with form `annotation`; a field and a record
  component are `property` (form `record_component`), an enum constant is
  `enum_member`, and an initializer block is `static_block` with form
  `static_init` or `instance_init`. Ids are by position under their file:
  `src/main/java/com/example/Shape.java#Shape.area`, nested types under their
  outer (`Outer.Inner`), a constructor `Type.constructor`, a lambda
  `Owner.<lambda@line:col>`. Outside the project, ids are
  `ext:<java.package>#Outer.member`, and `symbol.package` is the JDK module or the
  jar's coordinate.
- **References.** Kinds are `call`, `new`, `type`, `extends`, `implements`,
  `decorator` for an annotation, `value` for a method reference or a class
  literal, and `read` / `write` / `readwrite` for field access. A type naming a
  static member's owner (`Base.unit()`) is a `type` ref at position `other`. A
  reference is *from* the innermost named declaration: a method, constructor,
  lambda, class, a field (for its initializer) or an initializer block.
- **Dispatch.** Constructors, static and private methods and a `super.m()` call
  are `static`; every other method call is `virtual` at the declaration javac
  resolved. That includes a functional interface's own method — `f.apply(x)` — and
  what a lambda there runs is points-to's question, not the call graph's (§4
  trap 4).
- **Hierarchies.** `extends` and `implements` are the clauses **as written**; an
  interface's `extends` list is `extends`. `overrides` is against each *direct*
  supertype, `Object` included, and a method a class inherits overrides its
  interfaces' methods where the class names the interface.
- **Annotations are `decorator` rows**, with `decorator.text` holding the
  arguments as written, and `throws_decl` carries each method's declared checked
  exceptions.
- **Control flow** is a statement-level graph per method, constructor, lambda and
  initializer block. Catches are tested in order, each an `on_true` into its body
  and an `on_false` to the next; try-with-resources closes in a `finally` of its
  own, and `synchronized` releases its monitor in one; a colon-form switch falls
  through and an arrow-form one does not; a switch expression is lowered before
  the statement holding it, its `yield` a break.
- **Quality.** `diagnostic` is what javac reports with the lint options the build
  compiles with, `key` holding javac's own name for it (`compiler.err.cant.resolve`)
  and `code` 0. `lint_directive` covers `@SuppressWarnings` (a row per tool its
  rules name), `@SuppressFBWarnings`, and `NOSONAR`, `NOPMD`, `CHECKSTYLE:OFF`,
  `spotless:off` comments. `any_site` with kind `raw` is a raw type; `assertion`
  is a cast; `throw_site` and `catch_site` are `throw` and each catch clause;
  `floating_promise` is a discarded `Future` or `CompletionStage`.
- **Dataflow.** A field written with no receiver loads off `this`; a static loads
  off its class, which is itself a `cell` allocation so the statics have somewhere
  to live. A lambda and a method reference are `function` allocations. A record
  whose canonical constructor and accessors the compiler wrote is modelled where
  the source writes them: `new R(a, b)` stores each component and `r.x()` loads it
  back. A type pattern binds by copy; a deconstruction pattern binds each
  component by a load.

## 3. The library over Java facts

What each library computes is in [`typescript.md`](typescript.md) §3; this is
what differs over Java facts.

| file | over Java |
|---|---|
| `checks.dl` | run directly |
| `modgraph.dl` | `dep` is file to file; `namespace_dep` and `in_namespace_cycle` are between Java packages, which is usually the question |
| `callgraph.dl`, `callreach.dl`, `flow.dl`, `dominators.dl`, `cochange.dl` | as for TypeScript |
| `metrics.dl` | `dit` and `noc` are meaningful, unlike Go — but they count *written* supertypes, so a class extending nothing has `dit` 0 (§4 trap 5) |
| `coupling.dl`, `cohesion.dl` | ask per file (`G = -1`), which is one top-level class in idiomatic Java; `G = -3` groups by Java package |
| `coupling_kinds.dl` | `content` is an outer class touching a nested class's private member, which Java allows; **`common` is empty**, since Java has no module-level variables — its shared mutable state is `static` fields, so ask `symbol(kind: property, is_static: true, is_readonly: false)` |
| `packages.dl` | against the build's declared dependencies; `unused` works, but check a module's packaging before believing it (a module that only aggregates declares dependencies it never imports) |
| `exports.dl` | give `entry/1` the files of the packages other modules import — for a library, its published API packages |
| `pointsto.dl`, `taint.dl` | as for TypeScript, over what §4 trap 7 leaves modelled. `pointsto.dl` is the costly one on a large base; `taint.dl` over a whole project of this size may not finish — seed it narrowly |

## 4. Traps specific to Java facts

1. **Unresolved names mean the build state is wrong, not the code.** Before any
   negative claim, count them: `?- unresolved_name_count(N).` from `orient.dl`,
   and `diagnostic(category: error)`. On a project built first, both should be 0
   or near it. A large count means a plugin's generated sources are not on disk,
   or a module's tests need another module's test classes that were never
   compiled — build and extract again rather than reading the facts.
2. **Reflection is invisible, and Java uses it everywhere.** `Class.forName`,
   `getMethod(…).invoke(…)`, a `ServiceLoader`, a dependency-injection container
   and a JSON library's binding all produce no call edge. A method reached only
   that way looks dead: `load`, `deserialize`, a no-argument constructor, an
   annotated handler. Check for the annotation, or for a reflective call in the
   class that dispatches, before calling anything unused.
3. **Nothing calls an entry point.** `main`, a servlet's `doGet`, a test method,
   a lifecycle method a framework calls by annotation — all are called from
   outside the code. Reach from `entry_point` first, and treat an annotated
   method as reachable.
4. **A call through a functional interface is answered by points-to.** A lambda
   has no `implements` or `overrides` row, so `callgraph.dl` cannot expand
   `f.apply(x)` to it. `pointsto.dl` names the receiver `callee_var` there and
   resolves it — but only where the project both *holds* and *invokes* the value.
   A lambda handed to the standard library (`stream().map`, `Comparator.comparing`)
   is invoked inside code that was never extracted, so nothing resolves it and
   nothing should.
5. **`extends` is the written clause, so implicit `Object` is not in it** — a
   class extending nothing has `dit` 0, and `dit` therefore measures depth
   *within the project*, usually what you want. `overrides` is the other way: it
   does name `ext:java.lang#Object.equals` and `.toString`, so filter those out
   before counting overrides as design.
6. **What the compiler writes is no one's.** A default constructor, a record's
   accessors and its canonical constructor make no `fn`, no `call_site` and no
   `ref`, and a reference to one names its *type* instead. So a record accessor
   never appears unused, because it never appears.
7. **Points-to is may-point-to, over the modelled subset.** It does not carry a
   bound method reference's receiver (`obj::m`), and a class outside the project
   gets no `cell`, so an outside class's static fields carry nothing. There `pts`
   can miss; elsewhere it only over-approximates.
8. **Two raw types on one line are one `any_site` row**, as any two identical
   facts are one. Count sites, not occurrences.
9. **The notes javac prints only as a compile ends are absent** — the
   "uses deprecated APIs" and unchecked summaries. Analysis never ends a compile.
   Per-site deprecation warnings are there; the summary is not.
10. **A multi-catch parameter has no `throw_site.type`**, because its type is a
    union. Catch clauses are per clause, not per type.

## 5. Verify before you believe

As for TypeScript: the engine is exact about the facts, and whether the facts say
what you think is checked in the source. Java leaves most to check where it defers
to run time — what reflection, a framework or an annotation calls — and where the
build decides what the compiler ever saw. Two probes are worth running before any
analysis of a Java project:

```sh
./datalog /tmp/facts/lib/checks.dl     # exit 1 = the facts are internally consistent
./datalog /tmp/facts/lib/orient.dl     # unresolved_name_count should be 0 or near it
```
