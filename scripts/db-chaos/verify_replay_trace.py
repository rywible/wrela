#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def find_trace_files(root: Path) -> list[Path]:
    if not root.exists():
        return []
    return sorted(root.rglob("*.json"))


def validate_trace(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        return [f"{path}: invalid json ({exc})"]

    if payload.get("version") != 1:
        errors.append(f"{path}: unsupported version {payload.get('version')!r}")
    lane = payload.get("lane")
    seed = payload.get("seed")
    events = payload.get("events")
    if not isinstance(events, list) or not events:
        errors.append(f"{path}: missing events")
        return errors
    expected_seq = 0
    for event in events:
        seq = event.get("seq")
        if seq != expected_seq:
            errors.append(f"{path}: non-monotonic seq at {seq}, expected {expected_seq}")
            break
        route = event.get("route", {})
        if route.get("lane") != lane:
            errors.append(f"{path}: route lane mismatch at seq {seq}")
        if route.get("scheduler_seed") != seed:
            errors.append(f"{path}: route seed mismatch at seq {seq}")
        expected_seq += 1
    return errors


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("tests/.artifacts")
    lanes = [root / "sim", root / "model"]
    files: list[Path] = []
    for lane_dir in lanes:
        files.extend(find_trace_files(lane_dir))
    all_errors: list[str] = []
    for path in files:
        all_errors.extend(validate_trace(path))
    if all_errors:
        print("replay trace verification failed:")
        for error in all_errors:
            print(f"- {error}")
        return 1
    print(f"replay trace verification ok ({len(files)} artifacts)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
