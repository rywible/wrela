#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
V2_ROOT="$ROOT/wrela-v2"

if [[ ! -d "$V2_ROOT" ]]; then
  echo "missing v2 root: $V2_ROOT" >&2
  exit 1
fi

# Purity means implementation code is pure .wr, while allowing fixture data/docs
# and generated artifacts in explicit locations.
DISALLOWED_IMPLEMENTATION="$({
  find "$V2_ROOT/src" -type f ! -name '*.wr' ! -name '*.md' ! -name '.gitkeep' 2>/dev/null
  find "$V2_ROOT/tools" -type f ! -name '*.wr' ! -name '*.md' ! -name '.gitkeep' 2>/dev/null
} | sort)"

if [[ -n "$DISALLOWED_IMPLEMENTATION" ]]; then
  echo "v2 purity violation: non-.wr implementation artifacts found under src/tools" >&2
  echo "$DISALLOWED_IMPLEMENTATION" >&2
  exit 1
fi

# Root-level hygiene: only source/docs and explicit fixture data are allowed.
DISALLOWED_MISC="$(
  find "$V2_ROOT" -type f \
    ! -path "$V2_ROOT/src/*" \
    ! -path "$V2_ROOT/tools/*" \
    ! -path "$V2_ROOT/fixtures/*" \
    ! -path "$V2_ROOT/tests/fixtures/*" \
    ! -path "$V2_ROOT/tests/.artifacts/*" \
    ! -path "$V2_ROOT/target/*" \
    ! -name '*.wr' \
    ! -name '*.md' \
    ! -name '.gitkeep' \
    | sort
)"

if [[ -n "$DISALLOWED_MISC" ]]; then
  echo "v2 purity violation: unexpected non-.wr files outside allowed fixture/artifact zones" >&2
  echo "$DISALLOWED_MISC" >&2
  exit 1
fi

# Fixture zones allow data files, but only a tight extension set.
FIXTURE_DISALLOWED="$({
  if [[ -d "$V2_ROOT/fixtures" ]]; then
    find "$V2_ROOT/fixtures" -type f ! -name '*.wr' ! -name '*.txt' ! -name '*.json' ! -name '*.md' ! -name '.gitkeep'
  fi
  if [[ -d "$V2_ROOT/tests/fixtures" ]]; then
    find "$V2_ROOT/tests/fixtures" -type f ! -name '*.wr' ! -name '*.txt' ! -name '*.json' ! -name '*.md' ! -name '.gitkeep'
  fi
} | sort)"

if [[ -n "$FIXTURE_DISALLOWED" ]]; then
  echo "v2 purity violation: fixture zones contain disallowed file types" >&2
  echo "$FIXTURE_DISALLOWED" >&2
  exit 1
fi

echo "v2 purity check passed"
