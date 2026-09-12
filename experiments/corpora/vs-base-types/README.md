# The `vs/base` corpus's type packages, and the tsconfig the pack owns

For `code_design` / H-CA1 (`../../notes/code-design-pack.md` step 2). **No
third-party code lives here** — only a manifest and a lockfile, for the same
reason `harness.corpus` fetches rather than vendors: a copied package puts
someone else's licence in a repo whose own licensing is deliberately unsettled.
`npm ci` against `package-lock.json` reproduces exactly the declarations the
answer key was computed against, integrity hashes and transitive pins included.

Versions follow `vscode-1.137.0`'s own `package.json`.

## The workspace a cell gets

```
<workspace>/
  package.json, package-lock.json   <- these, then `npm ci`
  node_modules/@types/…             <- so module resolution finds them by walking up
  src/tsconfig.base.json            <- VS Code's, from the corpus
  src/tsconfig.json                 <- `tsconfig.pack.json` here
  src/typings/*.d.ts                <- 9 files
  src/vs/base/**                    <- 485 files, 155,801 lines
  src/vs/{amdX,nls}.ts              <- and 9 under src/vs/platform/ (below)
```

`node_modules` at the **workspace root**, not beside the sources: resolution
walks up from `src/vs/base/**`, so nothing needs a `paths` mapping, and
`tsconfig.pack.json` stays a file anyone can read.

## `vs/base` is not self-contained

It reaches **11 files** outside itself, and the TypeScript program pulls them in
whatever `include` says. Shipping them is what makes the extraction equal the
one from the whole checkout:

```
src/vs/amdX.ts
src/vs/nls.ts
src/vs/platform/contextkey/common/{contextkey,scanner}.ts
src/vs/platform/instantiation/common/{descriptors,instantiation,serviceCollection}.ts
src/vs/platform/quickinput/browser/quickInputUtils.ts
src/vs/platform/quickinput/common/{quickAccess,quickInput}.ts
src/vs/platform/registry/common/platform.ts
```

Verified: the assembled workspace gives **1,363,422 facts** against the whole
checkout's 1,363,589, and `ref`, `call_site`, `extends`, `implements`,
`overrides`, `member_access`, `type_ref` and `symbol_type` are identical row for
row. The 167-fact difference is `dir` and `file_ancestor` — the workspace has
fewer directories, which is the point.

## What the types bought, and the blind spot that is left

**Unresolved names: 12,061 → 286** (`unresolved_ref`), `ref` 99,881 → 120,179,
`diagnostic` 3,134 → 62, `any_site` 13,225 → 1,369. `checks.dl` is clean.

The 286 that remain are a real blind spot, and **a question must not depend on
them**:

- **Electron** — `BrowserWindow`, `IpcMainEvent`, `MenuItem`, `WebContents`,
  `MessagePortMain`, `ProcessMemoryInfo` and the `electron` module itself. VS
  Code generates `.build/typings/electron.d.ts` in its build; there is no npm
  pin for it, so this is the one systematic gap.
- **`sqlite3`** (`Database`, `Statement`) and a few `ssh2` names its own types
  do not cover.
- **Index-signature reads** — `process.env.VSCODE_DEV`, `LOCALAPPDATA`,
  `ProgramW6432`, `windir`, `MUSL`. These resolve to no declaration by
  construction, not for want of a package.
- **Property reads on untyped values** — a tail of `a`, `b`, `el`, `r`,
  `length`, `apply`, `prototype`.

286 of 120,179 references is 0.24%, and none of it is in `vs/base`'s own
structure — but "nothing calls X" is still bounded by it, which is what
`orient.dl`'s blind-spot line exists to say.
