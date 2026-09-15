#!/usr/bin/env bash
# Build the standalone code-analysis skill bundle: dist/code-analysis-skill/
# (and a .tar.gz), dropped into a `.claude/skills/code-analysis` to install.
#
# The engine binary comes from `cargo package-skill`'s bundle. Every document is
# this skill's own (skill/SKILL.md, skill/reference/): the datalog skill's guide
# and recipe are written for a different reader, so they are not reused.
# The extractor ships without its tests, fixtures or dev packages, with its
# TypeScript vendored by `npm ci --omit=dev` and its Go frontend's modules by
# `go mod vendor`.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="$(cd "$HERE/.." && pwd -P)"
DATALOG="$REPO/datalog"
DIST="$HERE/dist"
BUNDLE="$DIST/code-analysis-skill"

# 1. The engine bundle (offline: a networked release build stalls in some sandboxes).
(cd "$DATALOG" && CARGO_NET_OFFLINE=true cargo package-skill >&2)
ENGINE="$DATALOG/target/dist/datalog-skill"
[[ -x "$ENGINE/datalog" ]] || { echo "package.sh: no engine at $ENGINE/datalog" >&2; exit 1; }

rm -rf "$DIST"
mkdir -p "$BUNDLE/reference" "$BUNDLE/tools/code-facts"

# 2. The skill: playbook, engine, wrapper.
cp "$HERE/skill/SKILL.md" "$BUNDLE/SKILL.md"
cp "$ENGINE/datalog" "$BUNDLE/datalog"
cp "$HERE/skill/code-facts" "$BUNDLE/code-facts"
chmod +x "$BUNDLE/datalog" "$BUNDLE/code-facts"

# 3. Reference docs.
cp "$HERE"/skill/reference/*.md "$BUNDLE/reference/"

# 4. The extractor, without what exists only to develop it.
TOOL="$HERE/tools/code-facts"
cp -r "$TOOL/src" "$TOOL/lib" "$BUNDLE/tools/code-facts/"
cp "$TOOL/package.json" "$TOOL/package-lock.json" "$TOOL/tsconfig.json" "$BUNDLE/tools/code-facts/"
if (cd "$BUNDLE/tools/code-facts" && npm ci --omit=dev --no-audit --no-fund >&2); then
  VENDORED="TypeScript vendored (npm ci)"
else
  VENDORED="npm unavailable — ./code-facts installs TypeScript on first use"
fi
if (cd "$BUNDLE/tools/code-facts/src/frontends/go" && go mod vendor >&2); then
  GO_VENDORED="Go modules vendored (go mod vendor)"
else
  GO_VENDORED="go unavailable — reading Go downloads its modules on first use"
fi

cat > "$BUNDLE/INSTALL.md" <<'MD'
# Installing the code-analysis skill

A self-contained Claude Code skill: `SKILL.md` (the instructions), the compiled
`datalog` engine, the `code-facts` extractor, and `reference/`.

```sh
cp -r code-analysis-skill ~/.claude/skills/code-analysis        # every project
cp -r code-analysis-skill <project>/.claude/skills/code-analysis  # one project
```

Start a new Claude Code session so the skill is discovered.

`./code-facts` needs Node.js 22.18 or later on `PATH`; reading Python needs
Python 3.11 or later too, and reading Go needs Go 1.26 or later. The `datalog`
binary is built for the platform the bundle was packaged on.

```sh
./code-facts path/to/tsconfig.json -o /tmp/facts
./datalog /tmp/facts/lib/checks.dl          # exit 1: no violations
```
MD

# 5. A tarball, best effort.
if tar -czf "$DIST/code-analysis-skill.tar.gz" -C "$DIST" code-analysis-skill 2>/dev/null; then
  TARBALL="$DIST/code-analysis-skill.tar.gz"
else
  TARBALL="(tar not available — directory bundle only)"
fi
echo "skill bundle: $BUNDLE"
echo "code-facts:   $VENDORED"
echo "go frontend:  $GO_VENDORED"
echo "tarball:      $TARBALL"
