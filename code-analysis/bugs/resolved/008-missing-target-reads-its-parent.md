---
id: 008
title: A Python or Java target that does not exist is read as its parent directory
severity: wrong-output
area: extractor
spec: [2026-09-14]
found: 2026-09-15
resolution: fixed 2026-09-15 — a Python, Go or Java target that does not exist is an error before any frontend runs
---

`code-facts --lang java <dir>/gone` extracted every Java file under `<dir>` and
exited 0; `--lang python` did the same. Go failed, but on `gone/go.mod`.

## Repro

```sh
mkdir -p /tmp/m/stray && echo 'class Stray {}' > /tmp/m/stray/Stray.java
node src/main.ts --lang java /tmp/m/gone -o /tmp/m-out --no-git   # writes stray/Stray.java
```

## Cause

Each frontend treats a target that is not a directory as a build file and reads
the directory holding it: the Java frontend's `Model.load` takes
`abs.getParent()` whenever `Files.isDirectory` is false, which a missing path
also is.

Found when a 200-run P1-java measurement lost its temporary project mid-run (what
removed it was not found); the frontend walked `/tmp` and failed on a systemd
directory it could not read.

## Fix

`run` checks that every Python, Go and Java target exists. `test/output.test.ts`
covers Python and Java; red without the check.
