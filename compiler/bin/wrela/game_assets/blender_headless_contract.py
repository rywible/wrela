#!/usr/bin/env python3
"""Wrela Blender headless generation contract.

This script is intended to be executed by Blender in headless mode:
  blender --background --factory-startup --python blender_headless_contract.py -- <args>
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

CONTRACT_VERSION = "wrela-blender-v1"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Wrela Blender headless asset contract")
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--asset-id", required=True)
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--style-profile", required=True)
    parser.add_argument("--seed", required=True, type=int)
    parser.add_argument("--replay-hash", required=True)
    parser.add_argument("--negative-constraints", default="")
    return parser.parse_args(argv)


def deterministic_bytes(args: argparse.Namespace) -> bytes:
    payload = "\n".join(
        [
            f"asset_id={args.asset_id.strip()}",
            f"prompt={args.prompt.strip()}",
            f"style_profile={args.style_profile.strip()}",
            f"seed={args.seed}",
            f"replay_hash={args.replay_hash.strip()}",
            f"negative_constraints={args.negative_constraints.strip()}",
            f"contract={CONTRACT_VERSION}",
        ]
    )
    return payload.encode("utf-8")


def main() -> int:
    try:
        raw = sys.argv
        args_index = raw.index("--") + 1 if "--" in raw else len(raw)
        args = parse_args(raw[args_index:])
        output_dir = Path(args.output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)

        artifact_bytes = deterministic_bytes(args)
        provider_response_hash = hashlib.sha256(artifact_bytes).hexdigest()
        artifact_id = f"mesh-{provider_response_hash[:16]}"
        artifact_filename = f"{artifact_id}.glb"

        artifact_path = output_dir / artifact_filename
        artifact_path.write_bytes(artifact_bytes)

        manifest = {
            "contract_version": CONTRACT_VERSION,
            "provider_response_hash": provider_response_hash,
            "artifacts": [
                {
                    "artifact_id": artifact_id,
                    "file_name": artifact_filename,
                    "logical_path": f"generated/{args.asset_id}/{artifact_id}.mesh",
                    "metadata": {
                        "engine": "blender-headless",
                        "seed": str(args.seed),
                        "style_profile": args.style_profile,
                        "replay_hash": args.replay_hash,
                    },
                }
            ],
        }
        (output_dir / "manifest.json").write_text(
            json.dumps(manifest, sort_keys=True, separators=(",", ":")),
            encoding="utf-8",
        )
        print(json.dumps({"status": "ok", "artifact": artifact_filename}))
        return 0
    except Exception as exc:  # pragma: no cover
        print(f"wrela blender contract failure: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
