#!/usr/bin/env bash
# Checks the agent instruction files:
#   - every AGENTS.md has a sibling CLAUDE.md (a symlink to it, or a file with "@AGENTS.md")
#   - every relative markdown link in AGENTS.md, docs/README.md and skills resolves
#   - each of those files carries "Up:" and "Verified:" lines
# Exits non-zero on any problem.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 2
fail=0
err() { echo "✗ $*"; fail=1; }

agents=$(git ls-files --cached --others --exclude-standard -- '*AGENTS.md' | grep -E '(^|/)AGENTS\.md$')
skills=$(git ls-files --cached --others --exclude-standard -- '.agents/skills/*/SKILL.md')
files="$agents $skills docs/README.md"

for a in $agents; do
  dir=$(dirname "$a"); c="$dir/CLAUDE.md"
  if [[ -L "$c" ]]; then
    [[ "$(readlink "$c")" == "AGENTS.md" ]] || err "$c is a symlink but not to AGENTS.md"
  elif [[ -f "$c" ]]; then
    grep -qx '@AGENTS.md' "$c" || err "$c does not import @AGENTS.md"
  else
    err "$a has no sibling CLAUDE.md"
  fi
done

[[ -L .claude/skills && -d .claude/skills ]] || err ".claude/skills is not a symlink to .agents/skills"

for f in $files; do
  [[ -f "$f" ]] || { err "$f missing"; continue; }
  grep -q '^Up: ' "$f" || err "$f has no 'Up:' line"
  grep -Eq '^Verified: [0-9]{4}-[0-9]{2}-[0-9]{2}' "$f" || err "$f has no 'Verified: YYYY-MM-DD' line"
  dir=$(dirname "$f")
  # Inline links [text](target); skip URLs, mailto and pure anchors.
  while IFS= read -r target; do
    target="${target%%#*}"
    [[ -z "$target" || "$target" =~ ^[a-z]+: ]] && continue
    [[ -e "$dir/$target" ]] || err "$f: broken link → $target"
  done < <(grep -oE '\]\([^)[:space:]]+\)' "$f" | sed -E 's/^\]\((.*)\)$/\1/')
done

if (( fail )); then echo "agent docs: FAILED"; exit 1; fi
echo "agent docs: OK ($(echo $files | wc -w | tr -d ' ') files)"
