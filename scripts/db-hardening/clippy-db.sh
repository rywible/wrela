#!/usr/bin/env bash
set -euo pipefail

tmp_json="$(mktemp)"
trap 'rm -f "$tmp_json"' EXIT

set +e
cargo clippy -p wrela_runtime --all-targets --message-format=json >"$tmp_json"
clippy_status=$?
set -e

python3 - "$tmp_json" "$clippy_status" <<'PY'
import json
import os
import sys

path = sys.argv[1]
clippy_status = int(sys.argv[2])

default_scope = [
    "runtime/src/db/mod.rs",
    "runtime/src/db/api.rs",
    "runtime/src/db/sql/mod.rs",
    "runtime/src/db/invariant_history.rs",
    "runtime/src/db/raft/append.rs",
    "runtime/src/db/raft/persistence.rs",
    "runtime/src/db/raft/state.rs",
    "runtime/src/db/wal/segment.rs",
    "runtime/tests/db_invariant_history.rs",
]
scope = os.environ.get("DB_CLIPPY_SCOPE")
scope_paths = [p.strip() for p in scope.split(",") if p.strip()] if scope else default_scope
scope_set = set(scope_paths)

scoped_issues = []

with open(path, "r", encoding="utf-8") as fh:
    for raw in fh:
        line = raw.strip()
        if not line:
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("reason") != "compiler-message":
            continue
        message = event.get("message", {})
        if message.get("level") not in {"warning", "error"}:
            continue
        spans = message.get("spans", [])
        if not spans:
            continue
        for span in spans:
            file_name = span.get("file_name", "")
            if file_name in scope_set:
                scoped_issues.append(
                    (
                        message.get("level", "warning"),
                        file_name,
                        span.get("line_start", 0),
                        message.get("message", ""),
                    )
                )
                break

if scoped_issues:
    print("DB clippy hygiene failed: scoped warnings/errors detected")
    for level, file_name, line_no, message in scoped_issues[:100]:
        print(f"{level}: {file_name}:{line_no}: {message}")
    sys.exit(1)

if clippy_status != 0:
    print(
        "DB clippy hygiene passed for scoped files (non-scoped clippy issues exist; tracked as debt)."
    )
else:
    print("DB clippy hygiene passed for scoped files.")
PY
