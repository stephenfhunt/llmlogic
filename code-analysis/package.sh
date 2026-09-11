#!/usr/bin/env bash
# Build the standalone code-analysis skill bundle: dist/code-analysis-skill/
# (and a .tar.gz), dropped into a `.claude/skills/code-analysis` to install.
#
# The engine and the two docs single-homed in the datalog skill (its SKILL.md,
# the language guide; its source-analysis recipe, the bring-your-own method)
# come from `cargo package-skill`'s bundle, which has already stripped their
# `<!-- block: … -->` markers — so this script never reimplements that rule.
# The extractor ships without its tests, fixtures or dev packages, with its
# TypeScript vendored by `npm ci --omit=dev`.
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

# 3. Reference docs. The two from the datalog skill lose their frontmatter: a
#    skill reads only its own SKILL.md's, and a second `name: datalog` block in a
#    reference file would read as a second skill's header.
strip_frontmatter() { awk 'NR == 1 && $0 == "---" { skip = 1; next } skip && $0 == "---" { skip = 0; next } !skip' "$1"; }
strip_frontmatter "$ENGINE/SKILL.md" > "$BUNDLE/reference/datalog.md"
cp "$ENGINE/recipes/source-analysis.md" "$BUNDLE/reference/bring-your-own.md"
for doc in "$HERE"/skill/reference/*.md; do
  [[ -L "$doc" ]] && continue # the symlinked two are handled above
  cp "$doc" "$BUNDLE/reference/"
done

# 4. The extractor, without what exists only to develop it.
TOOL="$HERE/tools/code-facts"
cp -r "$TOOL/src" "$TOOL/lib" "$BUNDLE/tools/code-facts/"
cp "$TOOL/package.json" "$TOOL/package-lock.json" "$TOOL/tsconfig.json" "$BUNDLE/tools/code-facts/"
if (cd "$BUNDLE/tools/code-facts" && npm ci --omit=dev --no-audit --no-fund >&2); then
  VENDORED="TypeScript vendored (npm ci)"
else
  VENDORED="npm unavailable — ./code-facts installs TypeScript on first use"
fi

cat > "$BUNDLE/INSTALL.md" <<'MD'
# Installing the code-analysis skill

A self-contained Claude Code skill: `SKILL.md` (the playbook), the compiled
`datalog` engine, the `code-facts` extractor, and `reference/`.

```sh
cp -r code-analysis-skill ~/.claude/skills/code-analysis        # every project
cp -r code-analysis-skill <project>/.claude/skills/code-analysis  # one project
```

Start a new Claude Code session so the skill is discovered.

`./code-facts` needs Node.js 22.18 or later on `PATH`; reading Python needs
Python 3.11 or later too. The `datalog` binary is built for the platform the
bundle was packaged on.

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
echo "tarball:      $TARBALL"
