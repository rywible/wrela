#!/usr/bin/env python3
"""Verify LALRPOP output identity across independent grammar source paths."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="wrela-regeneration-") as temporary:
    products = []
    for label in ("first", "second"):
        checkout = Path(temporary) / label
        checkout.mkdir()
        for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
            shutil.copy2(root / name, checkout / name)
        shutil.copytree(root / "frontend", checkout / "frontend")
        target = checkout / "target"
        env = dict(os.environ, CARGO_TARGET_DIR=str(target))
        subprocess.run(["cargo", "build", "--locked", "--offline"], cwd=checkout, env=env, check=True)
        generated = list(target.glob("debug/build/wrela_frontend-*/out/grammar.rs"))
        if len(generated) != 1:
            raise SystemExit(f"expected one generated grammar in {label}, found {len(generated)}")
        products.append(generated[0].read_bytes())
    if products[0] != products[1]:
        raise SystemExit("generated Rust differs across source paths")
    print("generated grammar identical across two source paths:", hashlib.sha256(products[0]).hexdigest())
