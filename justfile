# wrela development task runner

fast-rust-tests := "cargo test -p wrela --test repo_smoke"
fast-authored-tests := "cargo run -p wrela -- test language/spec --lane=fast"
full-rust-tests := "cargo test --workspace"
full-authored-tests := "cargo run -p wrela -- test language/spec --lane=full"
cleanroom-check-dir := ".artifacts/cargo-cleanroom/check"
cleanroom-test-dir := ".artifacts/cargo-cleanroom/test"
cleanroom-check := "CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/check cargo check --workspace"
cleanroom-test := "CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/test cargo test --workspace"
query-tests := "cargo test -p wrela --test query_contract_registry --test query_program_spine --test phase9_query_plan"
engine-frame-tests := "cargo test -p wrela --test engine_frame"
perf-smoke-cmd := "cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1"
perf-engine-closure-cmd := "cargo run --release -p wrela -- perf benchmarks/engine_frame --profile=1080p120 --query-backend=wgsl"
perf-engine-audit-cmd := "WRELA_PERF_ENGINE_AUDIT=1 cargo run --release -p wrela -- perf benchmarks/engine_frame --profile=1080p120 --query-backend=wgsl --perf-debug"

default:
    @just --list

# Workspace typecheck / fast compile signal.
check:
    cargo check --workspace

# Cleanroom workspace typecheck with incremental disabled and isolated artifacts.
check-clean:
    rm -rf {{cleanroom-check-dir}}
    {{cleanroom-check}}

# Workspace build without running tests.
build:
    cargo build --workspace

# Optimized workspace build artifact.
build-release:
    cargo build --workspace --release

# Bundle the macOS frame-live app for human/agent use.
bundle-frame-live-app:
    cargo build --release -p wrela_frame_live_app
    rm -rf ".artifacts/apps/Wrela Frame Live.app"
    mkdir -p ".artifacts/apps/Wrela Frame Live.app/Contents/MacOS"
    mkdir -p ".artifacts/apps/Wrela Frame Live.app/Contents/Resources"
    cp apps/frame_live_app/mac/Info.plist ".artifacts/apps/Wrela Frame Live.app/Contents/Info.plist"
    cp target/release/wrela_frame_live_app ".artifacts/apps/Wrela Frame Live.app/Contents/MacOS/Wrela Frame Live"
    chmod +x ".artifacts/apps/Wrela Frame Live.app/Contents/MacOS/Wrela Frame Live"

# Fast repo lane: repo smoke coverage plus the native authored fast lane.
test:
    {{fast-rust-tests}}
    {{fast-authored-tests}}

# Cleanroom Rust workspace verification with incremental disabled and isolated artifacts.
test-clean:
    rm -rf {{cleanroom-test-dir}}
    {{cleanroom-test}}

# Full repo lane: full Rust workspace verification plus the native authored full lane.
test-all:
    {{full-rust-tests}}
    {{full-authored-tests}}

# Focused runtime crate lane.
test-runtime:
    cargo test -p wrela_runtime

# Focused compiler crate lane.
test-compiler:
    cargo test -p wrela

# Focused CLI integration lane.
test-cli:
    cargo test -p wrela --test cli

# Focused query-contract and query-planning lane.
test-query:
    {{query-tests}}

# Focused engine-frame/reporting lane.
test-engine-frame:
    {{engine-frame-tests}}

# Cheap perf sanity lane.
perf-smoke:
    {{perf-smoke-cmd}}

# Canonical engine-frame closure lane.
perf-engine-closure:
    {{perf-engine-closure-cmd}}

# Non-canonical audit lane for live-vs-compatibility engine-frame measurements.
perf-engine-audit:
    {{perf-engine-audit-cmd}}

# Canonical closure alias.
perf-closure:
    {{perf-engine-closure-cmd}}

# Headless `LiveEngineHost` + motion-to-photon unit coverage (RFC 0011).
live-smoke:
    cargo test -p wrela --test live_host

# RFC 0011 M3 perf-latency lane: drives the reference host through the
# interactive `wrela live` entry, while keeping `live-smoke` as the cheap
# structural co-gate.
perf-latency:
    just live-smoke
    WRELA_TEST_OFFSCREEN=1 WRELA_REFERENCE_HOST_FRAMES=120 cargo run -p wrela --release -- live examples/surface_and_input/src/main.wr --frames=120

# Reference authored project smoke for the Phase 64 scaffold.
dev-smoke:
    cargo run -p wrela -- check examples/surface_and_input/src/main.wr

ship-interactive:
    WRELA_TEST_OFFSCREEN=1 WRELA_REF_HOST_SMOKE_SECS=60 cargo test -p wrela_reference_host --test smoke

# Workspace clippy gate.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    just lint-layering

# Fail if runtime crate imports the compiler package (RFC 0011 M2 layering).
# Forbids:
#   - any `use wrela::...` (named import)
#   - any inline `wrela::path::Item` reference
#   - any `extern crate wrela` (including aliases)
# inside runtime/src/**, runtime/tests/**, runtime/benches/**.
lint-layering:
    #!/usr/bin/env bash
    set -euo pipefail
    fail=0
    paths=(runtime/src runtime/tests runtime/benches)
    patterns=(
      '^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]+wrela(::|;|[[:space:]])'
      '\bwrela::[A-Za-z_]'
      '^[[:space:]]*extern[[:space:]]+crate[[:space:]]+wrela([[:space:]]+as[[:space:]]+(r#)?[A-Za-z_][A-Za-z0-9_]*)?[[:space:]]*;'
    )
    strip_rust_comments() {
      local file="$1"
      python3 - "$file" <<'PY'
    import sys

    src = open(sys.argv[1], encoding="utf-8").read()
    out = []
    i = 0
    n = len(src)
    block_depth = 0

    def mask(ch):
        out.append("\n" if ch == "\n" else " ")

    def mask_range(start, end):
        for ch in src[start:end]:
            mask(ch)

    def raw_string_end(pos):
        if src.startswith(("br", "cr"), pos):
            pos += 2
        elif src.startswith("r", pos):
            pos += 1
        else:
            return None
        hashes_start = pos
        while pos < n and src[pos] == "#":
            pos += 1
        if pos >= n or src[pos] != '"':
            return None
        closing = '"' + ("#" * (pos - hashes_start))
        end = src.find(closing, pos + 1)
        return n if end == -1 else end + len(closing)

    def quoted_string_end(pos):
        if src[pos] == '"':
            pos += 1
        elif pos + 1 < n and src[pos] in "bc" and src[pos + 1] == '"':
            pos += 2
        else:
            return None
        escaped = False
        while pos < n:
            ch = src[pos]
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                return pos + 1
            pos += 1
        return n

    def char_literal_end(pos):
        if src[pos] == "'":
            pos += 1
        elif pos + 1 < n and src[pos] == "b" and src[pos + 1] == "'":
            pos += 2
        else:
            return None
        if pos >= n or src[pos] in "\r\n":
            return None
        if src[pos] == "\\":
            pos += 1
            if pos < n and src[pos] == "u" and pos + 1 < n and src[pos + 1] == "{":
                pos += 2
                while pos < n and src[pos] != "}":
                    pos += 1
                if pos < n:
                    pos += 1
            elif pos < n:
                pos += 1
        else:
            pos += 1
        return pos + 1 if pos < n and src[pos] == "'" else None

    while i < n:
        two = src[i:i + 2]
        if block_depth:
            if two == "/*":
                mask_range(i, i + 2)
                block_depth += 1
                i += 2
            elif two == "*/":
                mask_range(i, i + 2)
                block_depth -= 1
                i += 2
            else:
                mask(src[i])
                i += 1
            continue

        end = raw_string_end(i) or quoted_string_end(i) or char_literal_end(i)
        if end is not None:
            mask_range(i, end)
            i = end
            continue

        if two == "//":
            while i < n and src[i] != "\n":
                mask(src[i])
                i += 1
        elif two == "/*":
            mask_range(i, i + 2)
            block_depth = 1
            i += 2
        else:
            out.append(src[i])
            i += 1

    sys.stdout.write("".join(out))
    PY
    }
    for path in "${paths[@]}"; do
      [[ -d "$path" ]] || continue
      while IFS= read -r file; do
        stripped="$(strip_rust_comments "$file")"
        for pat in "${patterns[@]}"; do
          matches="$(printf '%s\n' "$stripped" | rg -n "$pat" || true)"
          if [[ -n "$matches" ]]; then
            while IFS= read -r match; do
              printf '%s:%s\n' "$file" "$match"
            done <<< "$matches"
            echo "lint-layering: forbidden compiler reference in $file (pattern: $pat)" >&2
            fail=1
          fi
        done
      done < <(rg --files "$path" -g '*.rs' 2>/dev/null || true)
    done
    if [[ "$fail" -ne 0 ]]; then
      exit 1
    fi
    echo "lint-layering: ok"

# Format the Rust workspace.
fmt:
    cargo fmt --all

# Formatting verification gate.
fmt-check:
    cargo fmt --all -- --check

# Best-effort cargo fix followed by formatting.
fix:
    cargo fix --workspace --allow-dirty --allow-staged
    cargo fmt --all

# Authoritative local pre-ship gate.
ship:
    just fmt-check
    just test
    just test-all
    just perf-latency
    just perf-smoke
    just ship-interactive
