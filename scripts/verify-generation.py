#!/usr/bin/env python3
"""Verify both composed grammars and parsers across clean source paths."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
parser_names = ("grammar", "type_probe")
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
        authored = {path: path.read_bytes() for path in (checkout / "frontend").rglob("*") if path.is_file()}
        subprocess.run(["python3", "frontend/tools/generate_unicode.py"], cwd=checkout, check=True)
        subprocess.run(["cargo", "build", "--locked", "--offline"], cwd=checkout, env=env, check=True)
        generated = {}
        for name in parser_names:
            for suffix in ("lalrpop", "rs"):
                filename = f"{name}.{suffix}"
                paths = list(target.glob(f"debug/build/wrela_frontend-*/out/{filename}"))
                if len(paths) != 1:
                    raise SystemExit(f"expected one generated {filename} in {label}, found {len(paths)}")
                generated[filename] = paths[0].read_bytes()
        if any(path.read_bytes() != content for path, content in authored.items()):
            raise SystemExit("generation changed an authored frontend file")
        if set(authored) != {path for path in (checkout / "frontend").rglob("*") if path.is_file()}:
            raise SystemExit("generation wrote a product into the authored frontend tree")
        products.append(generated)
    if products[0] != products[1]:
        raise SystemExit("generated Rust differs across source paths")
    for filename, content in products[0].items():
        print(f"{filename} identical across two clean source paths: {hashlib.sha256(content).hexdigest()}")
