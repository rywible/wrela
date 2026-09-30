#!/usr/bin/env python3
"""Negative controls: one authored rule mutation must reach its real consumers."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
controls = (
    (
        "lexical-region",
        "frontend/src/lexical_regions_policy.rs",
        "Token(Arrow) => Replace(Region { type_context: true, ..region }),",
        "Token(Arrow) => Replace(Region { type_context: false, ..region }),",
        "authored_arrow_policy_",
        ("authored_arrow_policy_controls_exported_type_punctuation",
         "authored_arrow_policy_controls_typed_function_result"),
    ),
    (
        "shared-type",
        "frontend/src/type-syntax.lalrpop.inc",
        '    <l:@L> <open:"["> <ty:Type> <close:"]"> <r:@R> => types::array(l, open, ty, close, r)\n',
        "",
        "shared_array_rule_",
        ("shared_array_rule_reaches_generic_selection",
         "shared_array_rule_reaches_typed_parsing"),
    ),
)
for label, filename, original, mutation, test_filter, contracts in controls:
    with tempfile.TemporaryDirectory(prefix=f"wrela-authority-{label}-") as temporary:
        checkout = Path(temporary)
        for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
            shutil.copy2(root / name, checkout / name)
        shutil.copytree(root / "frontend", checkout / "frontend")
        policy = checkout / filename
        authored = policy.read_text(encoding="utf-8")
        if authored.count(original) != 1:
            raise SystemExit(f"{label}: expected exactly one authored mutation target")
        policy.write_text(authored.replace(original, mutation), encoding="utf-8")
        result = subprocess.run(
            ["cargo", "test", "--locked", "--offline", "--test", "grammar_contract", test_filter],
            cwd=checkout, env=dict(os.environ, CARGO_TARGET_DIR=str(checkout / "target")),
            capture_output=True, text=True,
        )
        output = result.stdout + result.stderr
        if (result.returncode == 0 or "test result: FAILED. 0 passed; 2 failed;" not in output
                or any(f"test {contract} ... FAILED" not in output for contract in contracts)):
            print(output)
            raise SystemExit(f"{label}: mutation failed to break both real-consumer contracts")
        print(f"{label}: one authored rule mutation breaks both real-consumer contracts")
