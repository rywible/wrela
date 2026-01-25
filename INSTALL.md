Wrela Install

Quick install (macOS/Linux, x86_64 + arm64):

```sh
curl -fsSL https://raw.githubusercontent.com/rywible/wrela/main/scripts/install.sh | bash
```

To customize the install location:

```sh
PREFIX="$HOME/.local/wrela" curl -fsSL https://raw.githubusercontent.com/rywible/wrela/main/scripts/install.sh | bash
```

Add to your PATH (default):

```sh
export PATH="$HOME/.local/wrela/bin:$PATH"
```

Manual install from a release asset:

```sh
curl -LO https://github.com/ryanwible/wrela/releases/latest/download/wrela-<target>.tar.gz
tar -xzf wrela-<target>.tar.gz -C "$HOME/.local/wrela"
```

Targets:
- macOS arm64: `aarch64-apple-darwin`
- macOS x86_64: `x86_64-apple-darwin`
- Linux arm64: `aarch64-unknown-linux-gnu`
- Linux x86_64: `x86_64-unknown-linux-gnu`

To install a specific release:

```sh
WRELA_TAG=v0.1.0-alpha.1 curl -fsSL https://raw.githubusercontent.com/rywible/wrela/main/scripts/install.sh | bash
```
