#!/usr/bin/env python3
"""Negative control: an ambiguous generated grammar must fail the build."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="wrela-conflict-") as temporary:
    checkout = Path(temporary)
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        shutil.copy2(root / name, checkout / name)
    shutil.copytree(root / "frontend", checkout / "frontend")
    (checkout / "frontend/src/conflict.lalrpop").write_text(
        'grammar;\npub Expression: () = { <Expression> "+" <Expression> => (), "n" => () };\n',
        encoding="utf-8",
    )
    result = subprocess.run(["cargo", "build", "--locked", "--offline"], cwd=checkout,
        env=dict(os.environ, CARGO_TARGET_DIR=str(checkout / "target")), capture_output=True, text=True)
    output = result.stdout + result.stderr
    if (result.returncode == 0 or "conflict.lalrpop" not in output
            or not ("Conflict detected" in output or "Local ambiguity detected" in output)):
        print(output)
        raise SystemExit("conflict negative control did not fail for grammar ambiguity")
    print("ambiguous grammar rejected by generation/build")
