---
id: 002
title: an unresolvable bare specifier's first path segment becomes a package name
severity: wrong-answer
area: extractor
spec: ["decisions.md 2026-09-12"]
found: 2026-09-12
resolution:
---

When TypeScript cannot resolve a bare specifier, `code-facts` still fills
`imports.target_package` from the specifier's first segment. If the specifier is
a *bundler alias* rather than a package, the fact base gains a package that does
not exist, and `packages.dl` reports it as an undeclared dependency.

## Repro

Grafana, `public/app/index.ts:5`:

```ts
import 'vendor/css/font_awesome.css';
```

`vendor/` is a webpack `resolve.alias` onto `public/vendor`. The facts carry
`target_package: "vendor"`, and `lib/packages.dl` answers

```
undeclared("grafana", "vendor", "public/app/index.ts").
```

There is no npm package called `vendor`. The same shape fires for any
`resolve.alias`, `tsconfig.paths` entry the compiler did not apply, or
`webpack.resolve.modules` root — all common in application repositories.

## Root cause

`src/context.ts`, `packageOfSpecifier`, is applied to a specifier the checker
failed to resolve. The asset-import work (`decisions.md` 2026-09-11 later ii)
established the principle for the resolved case — a wildcard `declare module`
target is named `target_ambient` rather than faked into `target_file` — and the
unresolved case never got the same treatment.

## Acceptance criteria

A fixture with an unresolvable bare specifier, asserting that
`imports.target_package` is **absent** and `resolved` is `false`. Deciding what
the row *should* say is the design part: leaving `target_package` absent is the
conservative fix and costs the genuine "this package is not installed" signal,
which is why the alternative — a `--alias` flag, or reading a webpack/vite
config — is worth weighing rather than assuming.

## Fallout

Bounds every `undeclared` reading on an application repository. Does not affect
`unused`, `only_in_tests` or `types_only`, which key on declared names.

## Resolution

**Fixed 2026-09-13** (the user chose absent + a separate relation over reading
bundler configs). An unresolved bare specifier now leaves `target_package` absent
and puts its first segment in a new column, `imports.unresolved_package`, named as
a guess. `packages.dl` gains `guessed` and `unresolved_bare`. A guess **confirmed by
a declaration** (or an `@types/` one) counts as `imported`. `checks.dl` pins that a
guess appears only on an unresolved row and never beside a `target_package`. The
diagnosis held.

**Not taken:** reading webpack/vite aliases. The extractor mirrors tsconfig, and
parsing JavaScript configs is a different tool. **Absent only** was not taken
either: on an extraction without `node_modules`, every package import is
unresolved, and `unused` would have named every dependency.

**What it cost:** the first version fed the guess to `used` alone. The `design`
fixture, which has nothing installed, showed `dev_in_production`, `only_in_tests`
and `types_only` all go silent. That was a wider loss than this file's Fallout
allows, and the confirmed-guess rule is the fix. The one intended change is that
an undeclared, uninstalled package (`chalk` there) is now `unresolved_bare`, not
`undeclared`. The Python frontend is unchanged: it has no aliases, so a top-level
name outside the project and the stdlib is a distribution by definition.
*Mutations*: restore the old `target_package` fallback → two tests red; drop
`imported :- confirmed` → the design and library tests red.
