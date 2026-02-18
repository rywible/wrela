#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ALLOWLIST="$ROOT/runtime/unsafe_allowlist.txt"

if [[ ! -f "$ALLOWLIST" ]]; then
  echo "missing allowlist file: $ALLOWLIST" >&2
  exit 1
fi

ALLOWED="$(grep -v '^#' "$ALLOWLIST" | sed '/^[[:space:]]*$/d' | sort)"

# Find files containing Rust `unsafe` token.
UNSAFE_FILES="$(
  rg -l --glob '*.rs' '\bunsafe\s*(\{|\bfn\b|\bimpl\b|\bextern\b)' "$ROOT/compiler" "$ROOT/runtime" \
    | sed "s|$ROOT/||" \
    | sort
)"

# Ensure all unsafe files are explicitly allowlisted.
while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  if ! printf '%s\n' "$ALLOWED" | grep -qx "$file"; then
    echo "unsafe quarantine violation: $file is not in runtime/unsafe_allowlist.txt" >&2
    exit 1
  fi
done <<EOF
$UNSAFE_FILES
EOF

# Ensure allowlist does not contain stale entries.
while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  if ! printf '%s\n' "$UNSAFE_FILES" | grep -qx "$file"; then
    echo "unsafe allowlist stale entry: $file" >&2
    exit 1
  fi
done <<EOF
$ALLOWED
EOF

echo "unsafe allowlist check passed"
