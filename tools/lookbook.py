#!/usr/bin/env python3
"""The floor's lookbook (#54 §9): each shot of examples/last-green/lookbook.wrela rendered settled
at 1080p on the native host, as a still, into a directory.

    tools/lookbook.py <out-dir> [shot-number ...]

The floor must be built (`wrela build examples/last-green`). A shot is chosen as a player would
(L, its number, Enter); the still is the frame its move starts at (`lookbook::SETTLE` seconds in),
when the tiles round it have streamed in and its caches have settled.
"""

import json, os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD = os.path.join(ROOT, "examples/last-green/build")
HOST = os.path.join(ROOT, "target/release/wrela-host")
SETTLE_FRAMES = 300


def names():
    text = open(os.path.join(ROOT, "examples/last-green/lookbook.wrela")).read()
    return re.findall(r'Shot \{\s*name: "([^"]+)"', text)


def still(i, name, out):
    events = [{"frame": 1, "type": "key", "key": "KeyL"}]
    events += [{"frame": 1, "type": "key", "key": "Digit" + d} for d in str(i)]
    events.append({"frame": 1, "type": "key", "key": "Enter"})
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(events, f)
        script = f.name
    png = os.path.join(out, f"{i:02d}-{name.replace(' ', '-').replace(chr(39), '')}.png")
    r = subprocess.run([HOST, BUILD, "--frames", str(SETTLE_FRAMES + 1), "--size", "1920x1080",
                        "--input", script, "--png", png], capture_output=True, text=True)
    os.unlink(script)
    if r.returncode != 0:
        sys.exit(f"shot {i} ({name}) failed:\n{r.stderr[-2000:]}")
    return png


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    shots = names()
    pick = [int(a) for a in sys.argv[2:]] or range(len(shots))
    for i in pick:
        print(still(i, shots[i], out))


if __name__ == "__main__":
    main()
