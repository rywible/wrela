#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <app-name>" >&2
  exit 1
fi

APP_NAME="$1"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MACHINES="${WRELADB_TARGET_VOTERS:-3}"
DEPLOY_TIMEOUT="${WRELA_FLY_DEPLOY_WAIT_TIMEOUT:-15m}"
DEPLOY_ORG="${WRELA_FLY_ORG:-personal}"
DEPLOY_USE_DEPOT="${WRELA_FLY_DEPLOY_USE_DEPOT:-true}"

resolve_deploy_app_path() {
  if [[ -n "${WRELA_FLY_DEPLOY_APP_PATH:-}" ]]; then
    printf '%s\n' "$WRELA_FLY_DEPLOY_APP_PATH"
    return 0
  fi
  local candidates=(
    "apps/wrela-http-db-smoke"
    "apps/ledger-lite"
  )
  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -d "$ROOT/$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo "no deploy app path found; set WRELA_FLY_DEPLOY_APP_PATH" >&2
  return 1
}

DEPLOY_APP_PATH="$(resolve_deploy_app_path)"
DEPLOY_CONFIG="$ROOT/$DEPLOY_APP_PATH/fly.toml"
DEPLOY_DOCKERFILE="$ROOT/$DEPLOY_APP_PATH/Dockerfile"

[[ -f "$DEPLOY_CONFIG" ]] || {
  echo "missing fly config: $DEPLOY_CONFIG" >&2
  exit 1
}
[[ -f "$DEPLOY_DOCKERFILE" ]] || {
  echo "missing dockerfile: $DEPLOY_DOCKERFILE" >&2
  exit 1
}

if ! flyctl status -a "$APP_NAME" >/dev/null 2>&1; then
  flyctl apps create "$APP_NAME" --org "$DEPLOY_ORG" >/dev/null
fi

flyctl deploy \
  --remote-only \
  --depot="$DEPLOY_USE_DEPOT" \
  --config "$DEPLOY_CONFIG" \
  --dockerfile "$DEPLOY_DOCKERFILE" \
  -a "$APP_NAME" \
  --strategy rolling \
  --yes \
  --wait-timeout "$DEPLOY_TIMEOUT"

flyctl scale count "$MACHINES" -a "$APP_NAME" --yes >/dev/null

echo "[deploy-safe] delegated to wrela deploy for app=$APP_NAME"
echo "[deploy-safe] smoke workflow: $ROOT/scripts/fly/wrela_deploy_smoke.sh"
