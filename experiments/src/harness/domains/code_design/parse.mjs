// The `code_design` oracle's parser: TypeScript's syntax tree, and nothing else.
//
//   ORACLE_PARSER_ROOT=<dir> node parse.mjs < files.json > facts.json
//
// `ORACLE_PARSER_ROOT` is where `harness corpus fetch` installed the pinned
// TypeScript (`corpora/oracle-parser/`); resolving it from there rather than
// from this file is what keeps the version the lockfile's and not whatever
// happens to be installed beside the harness.
//
// `files.json` is `[{path, text}]` — the **bytes the fixture ships**, not paths
// to read. That is `static_analysis`'s reasoning at this scale: the oracle must
// parse exactly what the subject was given, and neither may touch the
// filesystem to disagree about it. The output is one object per file, carrying
// only what the questions are *defined over* — never a judgement, and never
// anything the checker would have to answer.
//
// Why the real parser: `static_analysis`'s answer key is computed with Python's
// `ast`, and the extractor it grades uses `ast` too. Control 1 forbids an oracle
// that calls the thing under test (`code-facts`, `lib/`, the engine); it has
// never meant re-implementing parsing, and a bespoke scanner would take on its
// own bugs while buying no independence. What stays independent here is
// everything above the syntax: module resolution, the graphs, and every answer.
//
// `ts.createSourceFile` only. No Program, no checker, no module resolution —
// so nothing in this file can agree with `code-facts` by construction.

import { createRequire } from "node:module";
import * as fs from "node:fs";
import * as path from "node:path";

const parserRoot = process.env.ORACLE_PARSER_ROOT;
if (parserRoot === undefined) {
  process.stderr.write("parse.mjs: ORACLE_PARSER_ROOT is not set\n");
  process.exit(2);
}
const require = createRequire(path.join(parserRoot, "package.json"));
const ts = require("typescript");

/** The name a declaration binds, or null for one that binds nothing. */
function declaredName(node) {
  if (node.name !== undefined && ts.isIdentifier(node.name)) return node.name.text;
  return null;
}

/** Every identifier written anywhere inside `node`, as a set of names.
 *
 * Deliberately *every* identifier, including property names and type
 * references: the questions that use this say so in as many words, because a
 * rule the subject cannot apply the same way is a rule that grades a guess. */
function identifiersIn(node) {
  const names = new Set();
  const visit = (n) => {
    if (ts.isIdentifier(n)) names.add(n.text);
    ts.forEachChild(n, visit);
  };
  ts.forEachChild(node, visit);
  return names;
}

/** `this.x` and `this.x()` — the member names a method touches through `this`. */
function thisMembers(node) {
  const names = new Set();
  const visit = (n) => {
    if (ts.isPropertyAccessExpression(n) && n.expression.kind === ts.SyntaxKind.ThisKeyword) {
      names.add(n.name.text);
    }
    ts.forEachChild(n, visit);
  };
  ts.forEachChild(node, visit);
  return names;
}

function isExported(node) {
  const modifiers = ts.canHaveModifiers(node) ? ts.getModifiers(node) : undefined;
  return (modifiers ?? []).some((m) => m.kind === ts.SyntaxKind.ExportKeyword);
}

function isStatic(node) {
  const modifiers = ts.canHaveModifiers(node) ? ts.getModifiers(node) : undefined;
  return (modifiers ?? []).some((m) => m.kind === ts.SyntaxKind.StaticKeyword);
}

/** The kind word the questions use for a top-level declaration. */
function declKind(node) {
  if (ts.isFunctionDeclaration(node)) return "function";
  if (ts.isClassDeclaration(node)) return "class";
  if (ts.isInterfaceDeclaration(node)) return "interface";
  if (ts.isTypeAliasDeclaration(node)) return "type";
  if (ts.isEnumDeclaration(node)) return "enum";
  if (ts.isVariableStatement(node)) return "variable";
  return null;
}

function classShape(node) {
  const methods = [];
  const fields = [];
  for (const member of node.members) {
    const name = declaredName(member);
    if (name === null || isStatic(member)) continue;
    if (ts.isMethodDeclaration(member) || ts.isGetAccessor(member) || ts.isSetAccessor(member)) {
      methods.push({ name, touches: [...thisMembers(member)].sort() });
    } else if (ts.isPropertyDeclaration(member)) {
      fields.push(name);
    }
  }
  return { methods, fields: fields.sort() };
}

function parseFile({ path: relative, text }) {
  const sf = ts.createSourceFile(relative, text, ts.ScriptTarget.ES2024, true, ts.ScriptKind.TS);

  const imports = [];
  const topLevel = [];
  const classes = [];

  const addImport = (specifier, typeOnly, names) => {
    imports.push({ specifier, typeOnly, names });
  };

  for (const statement of sf.statements) {
    if (ts.isImportDeclaration(statement)) {
      const clause = statement.importClause;
      const specifier = statement.moduleSpecifier.text;
      // `import type { X }` is type-only whole; `import { type X }` is per name.
      const typeOnly = clause?.isTypeOnly === true;
      const names = [];
      if (clause?.name !== undefined) names.push({ name: clause.name.text, typeOnly });
      const bindings = clause?.namedBindings;
      if (bindings !== undefined && ts.isNamedImports(bindings)) {
        for (const element of bindings.elements) {
          names.push({ name: element.name.text, typeOnly: typeOnly || element.isTypeOnly === true });
        }
      } else if (bindings !== undefined && ts.isNamespaceImport(bindings)) {
        names.push({ name: bindings.name.text, typeOnly, namespace: true });
      }
      // No import clause at all is a side-effect import: it binds nothing and
      // always survives to run time.
      addImport(specifier, typeOnly && names.length > 0, names);
      continue;
    }
    if (ts.isExportDeclaration(statement) && statement.moduleSpecifier !== undefined) {
      const names = [];
      if (statement.exportClause !== undefined && ts.isNamedExports(statement.exportClause)) {
        for (const element of statement.exportClause.elements) {
          names.push({ name: element.name.text, typeOnly: statement.isTypeOnly || element.isTypeOnly === true });
        }
      }
      addImport(statement.moduleSpecifier.text, statement.isTypeOnly, names);
      continue;
    }

    const kind = declKind(statement);
    if (kind === null) continue;
    const exported = isExported(statement);
    if (ts.isVariableStatement(statement)) {
      for (const decl of statement.declarationList.declarations) {
        if (!ts.isIdentifier(decl.name)) continue;
        topLevel.push({ name: decl.name.text, kind, exported, mentions: [...identifiersIn(decl)].sort() });
      }
      continue;
    }
    const name = declaredName(statement);
    if (name === null) continue;
    topLevel.push({ name, kind, exported, mentions: [...identifiersIn(statement)].sort() });
    if (ts.isClassDeclaration(statement)) {
      classes.push({ name, exported, ...classShape(statement) });
    }
  }

  // Two module edges are not import *statements* and so are not in
  // `sf.statements`: `await import('./x.js')`, which runs, and
  // `import('./x.js').T` in a type position, which does not. Both name a file
  // and both are edges; missing them would be a silent hole in the graph, and
  // each was found by diffing this parser against an independent extractor.
  const sweep = (n) => {
    if (ts.isCallExpression(n) && n.expression.kind === ts.SyntaxKind.ImportKeyword) {
      const first = n.arguments[0];
      if (first !== undefined && ts.isStringLiteralLike(first)) {
        imports.push({ specifier: first.text, typeOnly: false, names: [], dynamic: true });
      }
    } else if (ts.isImportTypeNode(n) && ts.isLiteralTypeNode(n.argument)) {
      const literal = n.argument.literal;
      if (ts.isStringLiteralLike(literal)) {
        imports.push({ specifier: literal.text, typeOnly: true, names: [], typeQuery: true });
      }
    }
    ts.forEachChild(n, sweep);
  };
  ts.forEachChild(sf, sweep);

  return { path: relative, imports, topLevel, classes };
}

const requested = JSON.parse(fs.readFileSync(0, "utf8"));
process.stdout.write(JSON.stringify(requested.map(parseFile)));
