// The relation catalog: every relation code-facts emits, its columns, their types,
// and what they mean. This is the single normative home for the fact schema —
// the writer validates every row against it, and `schema/*.dl` and `SCHEMA.md`
// in the output are generated from it. Change a relation here and nowhere else.

export type Layer = "meta" | "structure" | "refs" | "flow" | "dataflow" | "quality" | "git";

export const LAYERS: readonly Layer[] = [
  "meta",
  "structure",
  "refs",
  "flow",
  "dataflow",
  "quality",
  "git",
];

/** Layers the user may switch off. `meta` and `structure` always run: every
 * other layer's symbol-valued columns point into `symbol` and `file`. */
export const OPTIONAL_LAYERS: readonly Layer[] = ["refs", "flow", "dataflow", "quality", "git"];

export type ColumnType = "string" | "int" | "bool" | "symbol" | "timestamp";

export interface Column {
  readonly name: string;
  readonly type: ColumnType;
  readonly doc: string;
  /** May be `null` in a row (arrives as `absent`). */
  readonly nullable?: boolean;
  /** For `symbol` columns: the closed set of values. */
  readonly values?: readonly string[];
}

export interface Relation {
  readonly name: string;
  readonly layer: Layer;
  readonly doc: string;
  readonly columns: readonly Column[];
}

function col(name: string, type: ColumnType, doc: string): Column {
  return { name, type, doc };
}
function opt(name: string, type: ColumnType, doc: string): Column {
  return { name, type, doc, nullable: true };
}
function oneOf(name: string, values: readonly string[], doc: string): Column {
  return { name, type: "symbol", doc, values };
}

export const SYMBOL_KINDS = [
  "module",
  "namespace",
  "class",
  "interface",
  "type_alias",
  "enum",
  "enum_member",
  "function",
  "method",
  "constructor",
  "getter",
  "setter",
  "property",
  "variable",
  "local",
  "parameter",
  "type_parameter",
  "signature",
  "object",
  "type_literal",
  "static_block",
  "unknown",
] as const;
export type SymbolKind = (typeof SYMBOL_KINDS)[number];

export const FN_KINDS = [
  "function",
  "method",
  "constructor",
  "getter",
  "setter",
  "arrow",
  "function_expression",
  "static_block",
  "lambda",
  "module",
] as const;
export type FnKind = (typeof FN_KINDS)[number];

export const FLOW_NODE_KINDS = [
  "entry",
  "exit",
  "throw_exit",
  "stmt",
  "cond",
  "loop_head",
  "switch",
  "case_test",
  "catch",
  "finally",
  "return",
  "throw",
  "break",
  "continue",
  "select",
  "goto",
  "fallthrough",
] as const;
export type FlowNodeKind = (typeof FLOW_NODE_KINDS)[number];

export const FLOW_EDGE_KINDS = [
  "next",
  "on_true",
  "on_false",
  "case",
  "default",
  "back",
  "break",
  "continue",
  "return",
  "throw",
  "goto",
  "fallthrough",
] as const;
export type FlowEdgeKind = (typeof FLOW_EDGE_KINDS)[number];

export const DECISION_KINDS = [
  "if",
  "conditional",
  "and",
  "or",
  "nullish",
  "and_assign",
  "or_assign",
  "nullish_assign",
  "for",
  "for_in",
  "for_of",
  "while",
  "do",
  "case",
  "catch",
  "default_value",
  "optional_chain",
] as const;
export type DecisionKind = (typeof DECISION_KINDS)[number];

const ID = "a symbol id (see `symbol.id`)";
const FILE = "repo-relative path of the source file";
const LINE = "1-based line";

export const RELATIONS: readonly Relation[] = [
  // ── meta ────────────────────────────────────────────────────────────────
  {
    name: "extraction",
    layer: "meta",
    doc: "One row describing the run that produced these facts.",
    columns: [
      col("tool_version", "string", "code-facts version"),
      col("typescript_version", "string", "the TypeScript compiler code-facts carries"),
      opt("python_version", "string", "the Python that read the Python sources, if any were read"),
      col("node_version", "string", "the Node.js that ran the extractor"),
      col("root", "string", "absolute path every `file` column is relative to"),
      col("targets", "string", "what was extracted — tsconfigs, go.mod or go.work files, Maven or Gradle builds, Python and Java source directories — comma-separated, repo-relative"),
      col("layers", "string", "the layers extracted, comma-separated"),
      col("time", "timestamp", "when the extraction ran (UTC)"),
      opt("git_head", "string", "HEAD commit of the repository, if it is one"),
      opt("go_version", "string", "the Go toolchain that read the Go sources, if any were read"),
      opt("java_version", "string", "the JDK whose compiler read the Java sources, if any were read"),
    ],
  },
  {
    name: "relation_rows",
    layer: "meta",
    doc: "Row count of every relation written. A zero here means *extracted and empty*.",
    columns: [
      col("relation", "string", "relation name"),
      oneOf("layer", LAYERS, "the layer it belongs to"),
      col("rows", "int", "number of rows"),
    ],
  },

  // ── structure ───────────────────────────────────────────────────────────
  {
    name: "project",
    layer: "structure",
    doc: "A project loaded: a tsconfig (given on the command line or reached through `references`), a Python root, a Go module, or a Java build module (a Maven module, a Gradle project, or a directory of sources with no build).",
    columns: [
      col("id", "string", "repo-relative path of the tsconfig, the Python root (or its pyproject.toml), the go.mod, or the Java module's pom.xml, build.gradle(.kts) or source directory"),
      col("dir", "string", "repo-relative directory of the project"),
      col("files", "int", "source files the project compiles that are under the root"),
      col("strict", "bool", "whether `strict` is on"),
      opt("module", "string", "the `module` option, lowercased"),
      opt("target", "string", "the `target` option, lowercased; Java: the release level the build compiles for"),
    ],
  },
  {
    name: "project_file",
    layer: "structure",
    doc: "Which projects compile which files. A file in several projects has several rows.",
    columns: [col("project", "string", "`project.id`"), col("file", "string", FILE)],
  },
  {
    name: "file",
    layer: "structure",
    doc: "A source file under the root that some project compiles.",
    columns: [
      col("path", "string", FILE),
      col("dir", "string", "its directory (`.` for the root)"),
      opt("package", "string", "`package.name` of the distribution unit it belongs to: the nearest enclosing package.json, a Go module, a Maven module or a Gradle project"),
      oneOf(
        "lang",
        ["ts", "tsx", "mts", "cts", "dts", "js", "jsx", "mjs", "cjs", "py", "pyi", "go", "java", "json"],
        "file flavour. `json` is **data, not source** — a file a module imports rather than one that was compiled; exclude it wherever you mean code",
      ),
      col("loc", "int", "lines"),
      col("sloc", "int", "lines carrying at least one token (not blank, not only comments)"),
      col("is_test", "bool", "a test file (`*.test.*`, `*.spec.*`, `__tests__/`, `test(s)/`, or only in a test tsconfig; Go's `_test.go`; a Java file in a test source set)"),
      col("is_decl", "bool", "a `.d.ts` file"),
      col("is_generated", "bool", "a heuristic: the header says `@generated`, `auto-generated` or `DO NOT EDIT`, or the name is `*.gen.*`, `*.generated.*`, `*_pb.*`/`*_pb2.py`/`*.pb.*`, or under `__generated__/`; or the build's own output — what an annotation processor or a source-generating plugin wrote under the build directory"),
      opt(
        "namespace",
        "string",
        "the language's own name for the unit the file belongs to: a Go import path (`…_test` for an external test package), a Java package (empty for the unnamed package), a Python dotted module; absent for TypeScript, where the file is the module",
      ),
    ],
  },
  {
    name: "excluded_file",
    layer: "structure",
    doc: "A source file under a target that the build leaves out, so no other relation describes it — count these before a negative answer.",
    columns: [
      col("path", "string", FILE),
      oneOf(
        "reason",
        ["build_constraint", "other_language"],
        "`build_constraint`: a Go build constraint or file-name suffix excludes it for this platform; `other_language`: a Kotlin, Groovy or Scala file under a Java source root, which the Java compiler does not read",
      ),
      opt("detail", "string", "the constraint as written, when there is one"),
    ],
  },
  {
    name: "dir",
    layer: "structure",
    doc: "Every directory that holds a file, and its ancestors up to the root.",
    columns: [
      col("path", "string", "repo-relative (`.` is the root)"),
      opt("parent", "string", "parent directory; absent for the root"),
      col("name", "string", "last path segment (`.` for the root)"),
      col("depth", "int", "0 for the root, 1 for its children, …"),
    ],
  },
  {
    name: "file_ancestor",
    layer: "structure",
    doc: "Each directory enclosing a file, at every depth — group by `depth` to treat any level of the tree as a component.",
    columns: [
      col("file", "string", FILE),
      col("dir", "string", "an enclosing directory, the file's own included"),
      col("depth", "int", "`dir.depth` of that directory"),
    ],
  },
  {
    name: "package",
    layer: "structure",
    doc: "A distribution unit within the root: a package.json at or above a project file, a go.mod module, a Maven module (`groupId:artifactId`) or a Gradle project (its path, `:app`).",
    columns: [
      col("name", "string", "package name (the directory path if it has none)"),
      col("dir", "string", "repo-relative directory"),
      opt("version", "string", "version field"),
      col("private", "bool", "`private: true`"),
    ],
  },
  {
    name: "package_dep",
    layer: "structure",
    doc: "A dependency declared by a package: in a package.json, a go.mod, a pom.xml or a Gradle build.",
    columns: [
      col("package", "string", "`package.name` declaring it"),
      col("dep", "string", "the dependency's package name"),
      oneOf("kind", ["prod", "dev", "peer", "optional"], "which dependency block"),
      col("range", "string", "the version range as written"),
      opt("types_for", "string", "for an `@types/` package, the package it types (`@types/node` → `node`)"),
      opt("scope", "string", "the build's own word for the dependency, where `kind` summarises it: `require`, `indirect` or `tool` in a go.mod; the Maven scope (`compile`, `runtime`, `test`, `provided`, `system`); the Gradle configuration (`implementation`, `api`, `compileOnly`, `testImplementation`, …)"),
    ],
  },
  {
    name: "imports",
    layer: "structure",
    doc: "A module-level dependency written in a file: every import, re-export, `require`, and dynamic `import()`.",
    columns: [
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("specifier", "string", "the module specifier as written"),
      oneOf(
        "kind",
        [
          "static",
          "type_only",
          "side_effect",
          "dynamic",
          "require",
          "import_equals",
          "reexport",
          "reexport_all",
          "type_query",
          "dot",
          "cgo",
          "implicit",
          "static_import",
          "on_demand",
        ],
        "`type_only` is `import type`; `type_query` is `import(\"x\").T` in a type; `dot` is Go's `import . \"x\"`, `cgo` its `import \"C\"`; `static_import` is Java's `import static`, `on_demand` its `import p.*`; `implicit` is no statement at all — a reference to another file of the same package, which that language needs no import for (at the first reference, `specifier` the name referenced)",
      ),
      opt(
        "runtime",
        "bool",
        "the statement survives into the emitted JavaScript — false when TypeScript elides it (its bindings are only used as types, or it is `import type`); a `.d.ts` never runs; absent if the emitter could not say. Java: false when the compiled class needs nothing of the target — every use is an inlined compile-time constant or a Javadoc link, or there is none",
      ),
      opt("target_file", "string", "the resolved file, when it is under the root"),
      opt("target_package", "string", "package name, for a specifier resolving outside the root (`node:fs` for builtins); Java: the Maven coordinate `groupId:artifactId` of the jar the class resolved from, the module name (`java.base`) for the JDK"),
      opt(
        "target_ambient",
        "string",
        'the pattern of the ambient `declare module` the specifier matched, when that is what resolved it — `"*.css"` for an asset import. Such a specifier names no file and no package, which is why `resolved` alone does not imply a target',
      ),
      col("builtin", "bool", "the specifier names a Node.js builtin module (`fs`, `node:fs`), a Go standard-library package, or a class of the JDK"),
      col("resolved", "bool", "the compiler resolved the specifier"),
      opt(
        "unresolved_package",
        "string",
        "TypeScript only: for a bare specifier the compiler could not resolve, its first segment (`@scope/pkg` for a scoped one) — a *guess* at a package, never a `target_package`, because a bundler alias (`vendor/x.css`) looks the same as a package that is not installed. Python fills `target_package` by name instead: it has no aliases",
      ),
      opt(
        "target_dir",
        "string",
        "Go and Java: the directory of the package imported, when it is under the root. A Go import, and a Java on-demand or static import, names a package or a type's members rather than a file, so one statement is a row per file it references — each with `target_file` — and a Go import referencing none is a row with `target_dir` alone (a `_` import runs the package's `init`)",
      ),
    ],
  },
  {
    name: "import_name",
    layer: "structure",
    doc: "One name bound by an import or re-export.",
    columns: [
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("local", "string", "the local binding (the exported-as name for a re-export)"),
      col("imported", "string", "the name imported: `default`, `*` for a namespace, else the export name"),
      opt("target", "string", `${ID} it resolves to; absent when unresolved`),
      col("type_only", "bool", "imported with `type`"),
    ],
  },
  {
    name: "exports",
    layer: "structure",
    doc: "A name a module exports, after resolving re-export chains.",
    columns: [
      col("file", "string", FILE),
      col("name", "string", "exported name (`default` for a default export)"),
      opt("symbol", "string", `${ID} it resolves to`),
      oneOf("kind", ["local", "reexport"], "declared in this file, or re-exported from another"),
      col("is_type", "bool", "the symbol has no value meaning (an interface or type alias)"),
    ],
  },
  {
    name: "symbol",
    layer: "structure",
    doc:
      "Every declared thing: project declarations down to locals and parameters, plus every external (library) symbol something refers to. " +
      "`id` is the one id-space every symbol-valued column in every relation uses.",
    columns: [
      col(
        "id",
        "string",
        "`path#Container.name` for project symbols (`@line` on a collision, `<arrow@L:C>` for anonymous functions, `#<module>` for a file); `ext:pkg#name` or `lib#name` outside",
      ),
      col("name", "string", "the declared name (`<arrow>`, `<module>`, … when anonymous)"),
      oneOf("kind", SYMBOL_KINDS, "what it is"),
      oneOf("origin", ["project", "external", "lib"], "declared under the root, in a package, or in TypeScript's own lib"),
      opt("file", "string", `${FILE}; absent outside the project`),
      opt("line", "int", "line of the (first) declaration"),
      opt("end_line", "int", "last line of that declaration"),
      opt("parent", "string", `the enclosing symbol's id — a local's function, a method's class`),
      opt("package", "string", "package the declaration lives in"),
      col("exported", "bool", "exported from its own file"),
      {
        name: "visibility",
        type: "symbol",
        doc: "class members, and Java types: `hash_private` is an ECMAScript `#name`; `package` is visible within its package only (a Go member with a lower-case name, a Java declaration with no access modifier)",
        values: ["public", "protected", "private", "hash_private", "package"],
        nullable: true,
      },
      col("is_static", "bool", "`static` member"),
      col("is_abstract", "bool", "`abstract` class or member"),
      col("is_async", "bool", "`async` function"),
      col("is_generator", "bool", "generator function"),
      col("is_readonly", "bool", "`readonly` / `const`"),
      col("is_optional", "bool", "optional member or parameter"),
      col("is_ambient", "bool", "`declare`d or in a .d.ts"),
      {
        name: "form",
        type: "symbol",
        doc:
          "the language's own construct, where `kind` is the nearest shared one: a Go `struct` or `defined_type` (`type Celsius float64`) is kind `class`, an `embedded` field kind `property`; a Go method is a `pointer_receiver` or a `value_receiver`. A Java `record` or `enum` is kind `class`, an `annotation` type kind `interface`, a `record_component` kind `property`, and a `static_init` or `instance_init` initializer kind `static_block`",
        values: ["struct", "defined_type", "embedded", "pointer_receiver", "value_receiver", "record", "enum", "annotation", "record_component", "static_init", "instance_init"],
        nullable: true,
      },
    ],
  },
  {
    name: "param",
    layer: "structure",
    doc: "A parameter of a function-like, by position.",
    columns: [
      col("fn", "string", "the function's id"),
      col("index", "int", "0-based position"),
      col("symbol", "string", "the parameter's own symbol id"),
      col("name", "string", "its name (`<pattern>` for a destructuring parameter)"),
      col("optional", "bool", "`?` parameter"),
      col("rest", "bool", "`...rest` parameter"),
      col("has_default", "bool", "has a default value"),
    ],
  },
  {
    name: "doc",
    layer: "structure",
    doc: "Documentation state of every project declaration above local scope.",
    columns: [
      col("symbol", "string", ID),
      col("has_doc", "bool", "carries a JSDoc comment"),
      col("lines", "int", "length of that comment in lines"),
    ],
  },
  {
    name: "jsdoc_tag",
    layer: "structure",
    doc: "A JSDoc tag on a project declaration (`deprecated`, `internal`, `throws`, …).",
    columns: [
      col("symbol", "string", ID),
      col("tag", "string", "tag name without `@`"),
      opt("text", "string", "tag text, truncated to 200 characters"),
    ],
  },
  {
    name: "entry_point",
    layer: "structure",
    doc:
      "A function the runtime or a test runner calls and no code does — reach from these before calling anything unused. " +
      "Go: `main` in package main, every `init`, and in test files `TestMain` and the `Test`, `Benchmark`, `Fuzz` and `Example` functions `go test` runs. " +
      "Java: a `main` method the launcher accepts, every static initializer (`init`), and each method annotated for a test framework to run or to call around a test — `@Test`, `@ParameterizedTest`, `@BeforeEach` and the like (`test`).",
    columns: [
      col("symbol", "string", ID),
      oneOf("kind", ["main", "init", "test_main", "test", "benchmark", "fuzz", "example"], "who calls it"),
    ],
  },
  {
    name: "field_tag",
    layer: "structure",
    doc: "A Go struct field's tag, a row per `key:\"value\"` pair — how the field is named to encoding/json, a database, a validator, found by reflection and so by no reference.",
    columns: [
      col("field", "string", "the field's symbol id"),
      opt("key", "string", "the key (`json`); absent when the tag does not follow the `key:\"value\"` convention"),
      opt("value", "string", "the value between the quotes (`name,omitempty`), truncated to 200 characters"),
      col("text", "string", "the whole tag, truncated to 200 characters"),
    ],
  },
  {
    name: "typed_const",
    layer: "structure",
    doc: "A constant of a named type — the shape of a Go enumeration (`const ( Red Color = iota; Green )`).",
    columns: [
      col("symbol", "string", ID),
      col("type", "string", "the named type"),
      opt("value", "string", "the value as the compiler computes it, truncated to 100 characters"),
      col("iota", "bool", "its value comes from `iota`, written or repeated from a line above"),
    ],
  },
  {
    name: "module_directive",
    layer: "structure",
    doc: "A go.mod directive other than `require` and `tool`: the Go version and toolchain, and what is replaced, excluded, retracted or run with a changed GODEBUG default.",
    columns: [
      col("package", "string", "`package.name` of the module"),
      oneOf("directive", ["go", "toolchain", "replace", "exclude", "retract", "godebug"], "which directive"),
      opt("path", "string", "the module path it names (`replace`, `exclude`), or the GODEBUG setting (`godebug`)"),
      opt("version", "string", "the version (a range for `retract`, the setting's value for `godebug`); for `replace`, absent when every version is replaced"),
      opt("replacement", "string", "`replace`: the module path or directory put in its place"),
      opt("replacement_version", "string", "`replace`: the replacement's version; absent for a directory"),
    ],
  },
  {
    name: "decorator",
    layer: "structure",
    doc: "A decorator applied to a declaration; in Java, an annotation.",
    columns: [
      col("target", "string", "the decorated symbol"),
      opt("decorator", "string", "the decorator function's symbol, when resolved; a Java annotation's type"),
      col("name", "string", "the decorator expression's callee name as written"),
      col("line", "int", LINE),
      opt("text", "string", "Java: the annotation's arguments as written, without parentheses, truncated to 200 characters; absent when it has none"),
    ],
  },

  // ── refs ────────────────────────────────────────────────────────────────
  {
    name: "ref",
    layer: "refs",
    doc:
      "The universal reference edge: symbol `from` mentions symbol `to`. `from` is the innermost enclosing named declaration " +
      "(a function, method, class, module-level variable — or the file's `<module>`). References to locals and parameters " +
      "are in the flow layer (`def`/`use`), not here.",
    columns: [
      col("from", "string", ID),
      col("to", "string", ID),
      oneOf(
        "kind",
        ["read", "write", "readwrite", "call", "new", "type", "typeof", "extends", "implements", "jsx", "decorator", "value"],
        "how it is mentioned — `value` is a function or class passed around rather than called",
      ),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },
  {
    name: "call_site",
    layer: "refs",
    doc: "Every call, `new`, `super(…)`, tagged template, decorator call and JSX element, resolved by the type checker.",
    columns: [
      col("id", "int", "call-site id, shared with `call_at`, `actual`, `receiver`, `floating_promise`"),
      col("caller", "string", "the enclosing named declaration, as `ref.from`"),
      opt("callee", "string", "the target: the function for static/virtual dispatch, the called variable/parameter/property for indirect"),
      opt("callee_name", "string", "the callee's name as written"),
      oneOf(
        "dispatch",
        ["static", "virtual", "indirect", "unresolved"],
        "`virtual`: an instance member — expand through `overrides`; `indirect`: a function-typed value in project code — see `callee_var` (a library's function-typed value is named as the target instead)",
      ),
      oneOf("kind", ["call", "new", "super", "tagged", "jsx", "decorator"], "syntactic form"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("col", "int", "1-based column"),
      col("args", "int", "argument count as written"),
      col("awaited", "bool", "directly under `await`"),
      col("optional", "bool", "`?.()` call"),
      col("spread", "bool", "has a spread argument"),
    ],
  },
  {
    name: "extends",
    layer: "refs",
    doc: "Class or interface inheritance, direct edges only — for Go, an interface embedding another.",
    columns: [col("child", "string", ID), col("parent", "string", ID)],
  },
  {
    name: "implements",
    layer: "refs",
    doc:
      "A class's `implements` clause, direct edges only. Go writes none: there a row is the type checker's answer — a named type whose method set (or its pointer's) satisfies a project interface, or an outside one the project names.",
    columns: [
      col("class", "string", ID),
      col("interface", "string", ID),
      opt("pointer", "bool", "Go: only the pointer (`*T`) satisfies it — a `T` value does not, since a method it needs has a pointer receiver"),
    ],
  },
  {
    name: "embeds",
    layer: "refs",
    doc:
      "A Go struct embedding a type: the embedded type's fields and methods are promoted to the outer one. Composition, not inheritance — `extends` does not have these rows.",
    columns: [
      col("outer", "string", "the struct: its named type, or the field holding an anonymous struct"),
      col("inner", "string", "the embedded type"),
      col("pointer", "bool", "embedded as a pointer (`*T`)"),
    ],
  },
  {
    name: "overrides",
    layer: "refs",
    doc: "A member redeclaring one of its supertypes' members (class override, or interface member implementation).",
    columns: [
      col("member", "string", "the redeclaring member"),
      col("base", "string", "the member of a direct supertype it redeclares (possibly inherited by that supertype)"),
    ],
  },
  {
    name: "throws_decl",
    layer: "refs",
    doc: "A type a method or constructor declares it throws — a Java `throws` clause, checked and unchecked exceptions alike, as written.",
    columns: [col("fn", "string", "the declaring method or constructor"), col("type", "string", "the exception type")],
  },
  {
    name: "member_access",
    layer: "refs",
    doc:
      "A function touching a member of a project type — a class, an interface, an object type alias, or an inline object type. " +
      "The raw material of class cohesion, and of stamp coupling (which of a record's fields a function actually reads).",
    columns: [
      col("fn", "string", "the accessing function (as `ref.from`)"),
      col("member", "string", ID),
      col("owner", "string", "the type declaring the member: a class, interface, type alias or inline object type"),
      oneOf("mode", ["read", "write", "readwrite", "call"], "how"),
      col("via_this", "bool", "accessed as `this.x` / `this.#x`"),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },
  {
    name: "type_ref",
    layer: "refs",
    doc: "A type named in a type position, and which position — the raw material of type coupling and API-leak analysis.",
    columns: [
      col("from", "string", ID),
      col("to", "string", ID),
      oneOf(
        "position",
        ["param", "return", "property", "variable", "extends", "implements", "type_arg", "alias", "assertion", "other"],
        "where the type is written",
      ),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },
  {
    name: "symbol_type",
    layer: "refs",
    doc: "The checker's type for a value-carrying project symbol.",
    columns: [
      col("symbol", "string", ID),
      col("text", "string", "the type as the language's checker prints it, truncated to 200 characters; a Java method's is its signature, `(int)java.lang.String`"),
      col("is_any", "bool", "`any`; Go: an empty interface"),
      col("is_unknown", "bool", "`unknown`"),
      col("is_promise", "bool", "a Promise (or thenable); Java: a `Future` or `CompletionStage`"),
      col("is_function", "bool", "has call signatures; Java: a method, or a value of a functional interface type"),
      col("is_union", "bool", "a union type; Java: a multi-catch parameter"),
    ],
  },
  {
    name: "unresolved_ref",
    layer: "refs",
    doc: "A name the checker could not resolve — what the reference graph is missing.",
    columns: [
      col("from", "string", ID),
      col("name", "string", "the name as written"),
      oneOf("kind", ["call", "read", "type", "jsx", "other"], "where it appeared"),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },

  // ── flow ────────────────────────────────────────────────────────────────
  {
    name: "fn",
    layer: "flow",
    doc: "Every function-like with a body, plus one `<module>` per file for its top-level code. Metrics are per body, nested functions excluded.",
    columns: [
      col("id", "string", ID),
      oneOf("kind", FN_KINDS, "syntactic form"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("end_line", "int", "last line"),
      col("loc", "int", "lines spanned"),
      col("statements", "int", "statements, nested functions excluded"),
      col("params", "int", "parameter count"),
      col("max_nesting", "int", "deepest nesting of control structures"),
      col("cyclomatic", "int", "1 + the function's `decision` rows (ESLint `complexity` counting)"),
      col("cognitive", "int", "SonarSource cognitive complexity"),
      col("returns", "int", "`return` statements"),
      col("awaits", "int", "`await` expressions"),
      col("yields", "int", "`yield` expressions"),
      col("throws", "int", "`throw` statements"),
      col("halstead_operators", "int", "Halstead N1: operator occurrences"),
      col("halstead_operands", "int", "Halstead N2: operand occurrences"),
      col("halstead_distinct_operators", "int", "Halstead n1"),
      col("halstead_distinct_operands", "int", "Halstead n2"),
    ],
  },
  {
    name: "flow_node",
    layer: "flow",
    doc:
      "A control-flow-graph node: a statement or a condition, plus one `entry`, `exit` and `throw_exit` per function. " +
      "Short-circuit operators and `?:` are `decision`s, not nodes.",
    columns: [
      col("id", "int", "node id, unique across the fact base"),
      col("fn", "string", "the function whose graph it belongs to"),
      oneOf("kind", FLOW_NODE_KINDS, "what it is"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("col", "int", "1-based column"),
    ],
  },
  {
    name: "flow_edge",
    layer: "flow",
    doc:
      "A control-flow edge. Exceptions are over-approximated: every node inside a `try` gets a `throw` edge to its handler, " +
      "and a `finally` exits to every place a jump through it was headed.",
    columns: [
      col("from", "int", "`flow_node.id`"),
      col("to", "int", "`flow_node.id`"),
      oneOf("kind", FLOW_EDGE_KINDS, "why control moves"),
    ],
  },
  {
    name: "decision",
    layer: "flow",
    doc: "A branch point counted by cyclomatic complexity.",
    columns: [
      col("fn", "string", "the function"),
      opt("node", "int", "the flow node containing it"),
      oneOf("kind", DECISION_KINDS, "what kind of branch"),
      col("nesting", "int", "control-structure nesting depth at the decision"),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },
  {
    name: "def",
    layer: "flow",
    doc: "Flow node `node` assigns variable `var` (a local, parameter, catch binding or module-level variable).",
    columns: [col("node", "int", "`flow_node.id`"), col("var", "string", ID)],
  },
  {
    name: "use",
    layer: "flow",
    doc: "Flow node `node` reads variable `var`.",
    columns: [col("node", "int", "`flow_node.id`"), col("var", "string", ID)],
  },
  {
    name: "captures",
    layer: "flow",
    doc: "A function mentions a variable declared in an enclosing function (a closure capture).",
    columns: [col("fn", "string", "the capturing function"), col("var", "string", ID)],
  },
  {
    name: "closure",
    layer: "flow",
    doc: "A nested function is created at this flow node of its enclosing function.",
    columns: [col("node", "int", "`flow_node.id`"), col("fn", "string", "the nested function")],
  },
  {
    name: "call_at",
    layer: "flow",
    doc: "A call site evaluated at this flow node — ties `call_site` to the function that actually executes it.",
    columns: [col("node", "int", "`flow_node.id`"), col("call_site", "int", "`call_site.id`")],
  },
  {
    name: "await_at",
    layer: "flow",
    doc: "This flow node suspends on `await` (or `for await`).",
    columns: [col("node", "int", "`flow_node.id`")],
  },
  {
    name: "yield_at",
    layer: "flow",
    doc: "This flow node suspends on `yield`.",
    columns: [col("node", "int", "`flow_node.id`")],
  },
  {
    name: "concurrency_site",
    layer: "flow",
    doc: "Where a function starts a goroutine, defers a call, sends on or receives from a channel, or selects among channels.",
    columns: [
      col("fn", "string", "the function whose graph it belongs to"),
      col("node", "int", "`flow_node.id` — for a receive or send in a `select` case, the case; the deferred call itself runs at the function's `finally` node"),
      oneOf("kind", ["go", "defer", "chan_send", "chan_recv", "select"], "what happens"),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },

  // ── dataflow ────────────────────────────────────────────────────────────
  {
    name: "var",
    layer: "dataflow",
    doc:
      "A value-holder in the three-address normal form: named variables (their symbol id), plus temporaries, `this` and " +
      "return slots. Flow-insensitive and field-based, in the style of Doop's input facts.",
    columns: [
      col("id", "string", "a symbol id, or `<fn>$tN` / `<fn>$ret` / `<class>$this` / `$thrown`"),
      col("fn", "string", "the function it belongs to (`<module>` for module-level variables)"),
      oneOf("kind", ["local", "param", "module", "temp", "this", "ret", "catch", "thrown"], "what it is"),
    ],
  },
  {
    name: "assign",
    layer: "dataflow",
    doc: "Value flows from `from` into `to`. `copy` is the same value; `derive` is a value computed from it (arithmetic, templates, …) — taint follows both, points-to only `copy`.",
    columns: [
      col("to", "string", "`var.id`"),
      col("from", "string", "`var.id`"),
      oneOf("kind", ["copy", "derive"], "same value, or computed from"),
      col("fn", "string", "the function it happens in"),
    ],
  },
  {
    name: "alloc",
    layer: "dataflow",
    doc: "A heap object is created and held by `var`: an object/array literal, a `new`, a function or class value — for Go also a slice, map or channel made, and a `cell`: an address-taken or captured variable, or a package-level one.",
    columns: [
      col("var", "string", "`var.id` receiving it"),
      col("site", "int", "allocation-site id, unique"),
      oneOf("kind", ["object", "array", "instance", "function", "class", "slice", "map", "chan", "cell"], "what is allocated"),
      opt("type", "string", "the class instantiated, for `instance`"),
      opt("fn_target", "string", "the function the value is, for `function`/`class`"),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },
  {
    name: "load",
    layer: "dataflow",
    doc: "`to = base.field`. Element and computed accesses use field `[]`; for Go, a pointer's content is field `*`, a channel's elements `<chan>`, and a closure's captured variables `free0`, `free1`, … of the closure.",
    columns: [
      col("to", "string", "`var.id`"),
      col("base", "string", "`var.id`"),
      col("field", "string", "property name, or `[]`"),
      col("fn", "string", "the function it happens in"),
    ],
  },
  {
    name: "store",
    layer: "dataflow",
    doc: "`base.field = from`. Array elements and computed keys use field `[]`; for Go, `*`, `<chan>` and `free0`… as for `load`.",
    columns: [
      col("base", "string", "`var.id`"),
      col("field", "string", "property name, or `[]`"),
      col("from", "string", "`var.id`"),
      col("fn", "string", "the function it happens in"),
    ],
  },
  {
    name: "formal",
    layer: "dataflow",
    doc: "Parameter `index` of `fn` is held in `var`.",
    columns: [col("fn", "string", ID), col("index", "int", "0-based"), col("var", "string", "`var.id`")],
  },
  {
    name: "formal_ret",
    layer: "dataflow",
    doc: "`fn` returns the value held in `var`.",
    columns: [col("fn", "string", ID), col("var", "string", "`var.id`")],
  },
  {
    name: "actual",
    layer: "dataflow",
    doc: "Argument `index` of a call site is the value held in `var`.",
    columns: [col("call_site", "int", "`call_site.id`"), col("index", "int", "0-based"), col("var", "string", "`var.id`")],
  },
  {
    name: "actual_ret",
    layer: "dataflow",
    doc: "A call site's result is held in `var`.",
    columns: [col("call_site", "int", "`call_site.id`"), col("var", "string", "`var.id`")],
  },
  {
    name: "receiver",
    layer: "dataflow",
    doc: "A method call's receiver (`o` in `o.m()`), or the object a `new` constructs.",
    columns: [col("call_site", "int", "`call_site.id`"), col("var", "string", "`var.id`")],
  },
  {
    name: "callee_var",
    layer: "dataflow",
    doc: "The value a call site invokes — resolve it through points-to to find the functions an indirect call reaches.",
    columns: [col("call_site", "int", "`call_site.id`"), col("var", "string", "`var.id`")],
  },
  {
    name: "this_var",
    layer: "dataflow",
    doc: "Inside `fn`, `this` is held in `var` (shared by every instance member of a class).",
    columns: [col("fn", "string", ID), col("var", "string", "`var.id`")],
  },

  // ── quality ─────────────────────────────────────────────────────────────
  {
    name: "diagnostic",
    layer: "quality",
    doc: "A compiler diagnostic (syntactic, semantic, or from the tsconfig; for Go, what loading and type-checking a package reported; for Java, what javac reports with the lint options the build compiles with).",
    columns: [
      opt("file", "string", FILE),
      opt("line", "int", LINE),
      col("code", "int", "TS error code (2322 = not assignable, …); 0 where the compiler numbers none (Go, Java)"),
      oneOf("category", ["error", "warning", "suggestion", "message"], "severity"),
      col("message", "string", "first line of the message, truncated to 300 characters"),
      opt("key", "string", "Java: javac's name for the diagnostic (`compiler.err.cant.resolve.location`, `compiler.warn.has.been.deprecated`)"),
    ],
  },
  {
    name: "ts_directive",
    layer: "quality",
    doc: "A `// @ts-…` comment directive.",
    columns: [
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf("kind", ["ts_ignore", "ts_expect_error", "ts_nocheck", "ts_check"], "which directive"),
    ],
  },
  {
    name: "lint_directive",
    layer: "quality",
    doc: "A linter or formatter suppression comment; in Java, also a `@SuppressWarnings` or `@SuppressFBWarnings` annotation.",
    columns: [
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf(
        "tool",
        ["eslint", "biome", "prettier", "tslint", "istanbul", "c8", "noqa", "pylint", "mypy", "pyright", "coverage", "golangci", "staticcheck", "gosec", "javac", "sonar", "checkstyle", "pmd", "spotbugs", "spotless"],
        "which tool; `@SuppressWarnings` rules go to checkstyle (`checkstyle:`), sonar (`java:`, `squid:`), pmd (`PMD.`), and otherwise javac, which reads the rest",
      ),
      col("directive", "string", "e.g. `disable-next-line`, `ignore`; for an annotation, its name (`SuppressWarnings`)"),
      opt("rules", "string", "the rules named, as written"),
    ],
  },
  {
    name: "comment_marker",
    layer: "quality",
    doc: "A TODO / FIXME / HACK / XXX comment.",
    columns: [
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf("kind", ["todo", "fixme", "hack", "xxx"], "the marker"),
      col("text", "string", "the rest of the line, truncated to 200 characters"),
    ],
  },
  {
    name: "any_site",
    layer: "quality",
    doc: "Where `any` enters: written, implied by a missing annotation, or returned by a call. Java's is a raw type (`List` for `List<E>`), where javac's rawtypes lint finds one.",
    columns: [
      col("fn", "string", "enclosing declaration (as `ref.from`)"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf("kind", ["explicit", "implicit_param", "implicit_var", "call_result", "raw"], "how it arrives; `raw` is a raw type written, `call_result` also a Java call returning one"),
    ],
  },
  {
    name: "assertion",
    layer: "quality",
    doc: "A type assertion — a place the author overrode the checker.",
    columns: [
      col("fn", "string", "enclosing declaration"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf("kind", ["cast", "angle_cast", "non_null", "satisfies", "as_const", "type_assert", "type_switch"], "`cast` is `x as T`, or Java's `(T) x`; `angle_cast` is `<T>x`; Go's `type_assert` is `x.(T)`, `type_switch` a `switch x.(type)`"),
      opt("to_type", "string", "asserted type as written, truncated to 200 characters"),
      col("from_any", "bool", "the asserted expression was `any` (Java: of a raw type)"),
      col("to_any", "bool", "asserted to `any` (Java: to a raw type)"),
    ],
  },
  {
    name: "literal",
    layer: "quality",
    doc: "A literal value in code (not a type, import specifier or property key; in Java, not an annotation's argument) — shared literals across modules are connascence of meaning.",
    columns: [
      col("fn", "string", "enclosing declaration"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf("kind", ["string", "number", "bigint", "regexp", "template"], "literal kind; a character and a text block are `string`"),
      col("value", "string", "its text (strings unquoted), truncated to 100 characters"),
    ],
  },
  {
    name: "floating_promise",
    layer: "quality",
    doc: "A call returning a Promise whose result is discarded — not awaited, returned, assigned or `void`ed. In Java, a call made as a statement returning a `Future` or `CompletionStage`.",
    columns: [
      col("call_site", "int", "`call_site.id`"),
      col("fn", "string", "enclosing declaration"),
      col("file", "string", FILE),
      col("line", "int", LINE),
    ],
  },
  {
    name: "ignored_error",
    layer: "quality",
    doc: "A call whose `error` result nothing reads — Go's unchecked error: the call made as a statement, the error assigned to `_`, or a deferred or `go` call's result.",
    columns: [
      col("call_site", "int", "`call_site.id`"),
      col("fn", "string", "enclosing declaration (as `ref.from`)"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      oneOf("how", ["discarded", "blank", "deferred", "go"], "how the error is dropped"),
    ],
  },
  {
    name: "compiler_directive",
    layer: "quality",
    doc: "A Go directive comment: `//go:generate`, `//go:embed`, `//go:build`, `//go:linkname` and the rest, cgo's `//export`, and `//line`.",
    columns: [
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("name", "string", "the directive without `//go:` (`generate`, `embed`), or `export`, `line`"),
      opt("text", "string", "the rest of the line — the command, the patterns — truncated to 200 characters"),
    ],
  },
  {
    name: "throw_site",
    layer: "quality",
    doc: "A `throw` statement, or a Go `panic(…)` call.",
    columns: [
      col("fn", "string", "enclosing declaration"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      opt("type", "string", "the class thrown, for `throw new X(…)`; in Go and Java, the thrown value's static type, whatever the expression"),
    ],
  },
  {
    name: "catch_site",
    layer: "quality",
    doc: "A `catch` clause, or a Go `recover()` call.",
    columns: [
      col("fn", "string", "enclosing declaration"),
      col("file", "string", FILE),
      col("line", "int", LINE),
      col("binds", "bool", "binds the error (`catch (e)`, and not Java's unnamed `catch (E _)`; the value `recover()` returns is used)"),
      col("empty", "bool", "has no statements (for `recover()`, the function calling it does nothing else)"),
      col("rethrows", "bool", "contains a `throw` (for `recover()`, the function calling it panics)"),
    ],
  },

  // ── git ─────────────────────────────────────────────────────────────────
  {
    name: "commit",
    layer: "git",
    doc: "A commit reachable from HEAD that touched the root (its subtree, when the root is below the repository's top level).",
    columns: [
      col("sha", "string", "full hash"),
      col("author", "string", "author name"),
      col("email", "string", "author email"),
      col("time", "timestamp", "author time, UTC"),
      col("parents", "int", "parent count (2+ is a merge)"),
      col("files", "int", "paths touched — filter bulk commits with this"),
      col("subject", "string", "first line of the message, truncated to 200 characters"),
    ],
  },
  {
    name: "touch",
    layer: "git",
    doc: "A commit changed a path. `path_now` follows renames to the path in today's tree, so history lines up with `file.path`.",
    columns: [
      col("sha", "string", "`commit.sha`"),
      col("path", "string", "repo-relative path in that commit"),
      opt("path_now", "string", "the same file's path at HEAD; absent if it no longer exists"),
      opt("old_path", "string", "for a rename or copy, the source path"),
      oneOf("change", ["added", "modified", "deleted", "renamed", "copied", "type_changed"], "what happened"),
      opt("added", "int", "lines added (absent for binary files)"),
      opt("deleted", "int", "lines deleted (absent for binary files)"),
    ],
  },
];

export const RELATION_BY_NAME: ReadonlyMap<string, Relation> = new Map(RELATIONS.map((r) => [r.name, r]));
