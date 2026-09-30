#!/usr/bin/env python3
"""Negative controls: each actual composed grammar must reject ambiguity."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
for parser in ("grammar", "type_probe"):
    with tempfile.TemporaryDirectory(prefix=f"wrela-conflict-{parser}-") as temporary:
        checkout = Path(temporary)
        for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
            shutil.copy2(root / name, checkout / name)
        shutil.copytree(root / "frontend", checkout / "frontend")
        template = checkout / f"frontend/src/{parser}.lalrpop.in"
        with template.open("a", encoding="utf-8") as source:
            source.write('\npub AmbiguityControl: () = { <AmbiguityControl> "+" <AmbiguityControl> => (), "ident" => () };\n')
        result = subprocess.run(["cargo", "build", "--locked", "--offline"], cwd=checkout,
            env=dict(os.environ, CARGO_TARGET_DIR=str(checkout / "target")), capture_output=True, text=True)
        output = result.stdout + result.stderr
        if (result.returncode == 0 or f"{parser}.lalrpop" not in output
                or not ("Conflict detected" in output or "Local ambiguity detected" in output)):
            print(output)
            raise SystemExit(f"{parser} negative control did not fail for grammar ambiguity")
        print(f"{parser}: ambiguity in actual composed grammar rejected by generation/build")
