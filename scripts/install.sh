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

url="https://github.com/rywible/wrela/releases/latest/download/wrela-${target}.tar.gz"

mkdir -p "${PREFIX}"
curl -fsSL "${url}" | tar -xz -C "${PREFIX}"

cat <<EOF
Installed Wrela to: ${PREFIX}
Add this to your PATH:
  export PATH="${PREFIX}/bin:\$PATH"
EOF
