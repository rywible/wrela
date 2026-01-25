#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-"$HOME/.local/wrela"}"

os="$(uname -s)"
arch="$(uname -m)"

case "${os}" in
  Darwin)
    case "${arch}" in
      arm64) target="aarch64-apple-darwin" ;;
      x86_64) target="x86_64-apple-darwin" ;;
      *) echo "Unsupported architecture: ${arch}" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "${arch}" in
      aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
      x86_64) target="x86_64-unknown-linux-gnu" ;;
      *) echo "Unsupported architecture: ${arch}" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: ${os}" >&2
    exit 1
    ;;
esac

tag="${WRELA_TAG:-}"
if [ -z "${tag}" ]; then
  api="https://api.github.com/repos/rywible/wrela/releases"
  if command -v jq >/dev/null 2>&1; then
    tag="$(curl -fsSL "${api}" | jq -r '.[0].tag_name')"
  else
    tag="$(curl -fsSL "${api}" | sed -n 's/.*"tag_name": *"\\([^"]*\\)".*/\\1/p' | head -n 1)"
  fi
  if [ -z "${tag}" ] || [ "${tag}" = "null" ]; then
    echo "Failed to resolve a release tag. Set WRELA_TAG to install a specific release." >&2
    exit 1
  fi
fi

url="https://github.com/rywible/wrela/releases/download/${tag}/wrela-${target}.tar.gz"

mkdir -p "${PREFIX}"
curl -fsSL "${url}" | tar -xz -C "${PREFIX}"

cat <<EOF
Installed Wrela to: ${PREFIX}
Add this to your PATH:
  export PATH="${PREFIX}/bin:\$PATH"
EOF
