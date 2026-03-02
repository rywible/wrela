#!/usr/bin/env bash
set -euo pipefail

APP_NAME="${1:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ -z "$APP_NAME" && -f "$ROOT/artifacts/fly/last_lab_app.txt" ]]; then
  APP_NAME="$(cat "$ROOT/artifacts/fly/last_lab_app.txt")"
fi

if [[ -z "$APP_NAME" ]]; then
  echo "usage: $0 <app-name>" >&2
  exit 1
fi

flyctl apps destroy "$APP_NAME" --yes

echo "[destroy] destroyed $APP_NAME"
