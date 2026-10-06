#!/usr/bin/env python3
"""Spike 14's listening test (#48): four renders of the first Gymnopédie, for a person to rank
blind.

    tools/listening.py [out-dir]

The four are {wrela's piano, a sampled piano} x {the deadpan, the performance}. wrela's are the
piece's dry takes (examples/gymnopedie takes 2 and 3: no room, as the sampled piano has none);
the sampled piano plays each take's MIDI file (`wrela audio ... midi`): the Salamander Grand
Piano V3 (Alexander Holm, CC BY 3.0), as FreePats' SF2, played by FluidSynth with its reverb and
chorus off. Every render is scaled to the same RMS level (the loudest the four allow with no
peak past -1 dBFS), so loudness doesn't give one away.

Then they're shuffled and written as A.wav to D.wav; which is which goes in
key-open-after-ranking.json. With them, outside the test: the performance as the game plays it,
with the room (wrela-performance-with-room.wav).

Needs `wrela` built (`cargo build --release -p wrela`), FluidSynth, and the SF2: at
$WRELA_REFERENCE_SF2, or ~/.cache/wrela/salamander/. Run from the repository's root.
"""

import json, os, secrets, struct, subprocess, sys
from pathlib import Path

PIECE = "examples/gymnopedie"
WRELA = "target/release/wrela"
SF2 = os.environ.get(
    "WRELA_REFERENCE_SF2",
    str(Path.home() / ".cache/wrela/salamander/SalamanderGrandPiano-SF2-V3+20200602/SalamanderGrandPiano-V3+20200602.sf2"),
)


def run(*args):
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"{' '.join(args)} failed:\n{r.stdout}{r.stderr}")
    return r.stdout


def read_wav(path):
    """A WAV file's samples as floats (channels interleaved), its channels and its rate. Reads
    32-bit float and 16-bit PCM."""
    data = Path(path).read_bytes()
    at, fmt, channels, rate, bits = 12, None, 0, 0, 0
    while at + 8 <= len(data):
        chunk, size = data[at : at + 4], struct.unpack("<I", data[at + 4 : at + 8])[0]
        body = data[at + 8 : at + 8 + size]
        if chunk == b"fmt ":
            fmt, channels, rate = struct.unpack("<HHI", body[:8])
            bits = struct.unpack("<H", body[14:16])[0]
            if fmt == 0xFFFE:
                fmt = struct.unpack("<H", body[24:26])[0]
        elif chunk == b"data":
            if (fmt, bits) == (3, 32):
                return list(struct.unpack(f"<{len(body) // 4}f", body)), channels, rate
            if (fmt, bits) == (1, 16):
                return [x / 32768 for x in struct.unpack(f"<{len(body) // 2}h", body)], channels, rate
            sys.exit(f"{path}: WAV format {fmt} at {bits} bits isn't read")
        at += 8 + size + (size % 2)
    sys.exit(f"{path} has no data")


def write_wav(path, samples, channels, rate):
    data = struct.pack(f"<{len(samples)}f", *samples)
    fmt = struct.pack("<HHIIHH", 3, channels, rate, rate * 4 * channels, 4 * channels, 32)
    Path(path).write_bytes(b"RIFF" + struct.pack("<I", 4 + 8 + len(fmt) + 8 + len(data)) + b"WAVE" + b"fmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(data)) + data)


def rms(samples):
    return (sum(x * x for x in samples) / len(samples)) ** 0.5


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else f"{PIECE}/build/listening")
    work = out / "work"
    work.mkdir(parents=True, exist_ok=True)
    if not Path(SF2).is_file():
        sys.exit(f"no SF2 at {SF2}: set WRELA_REFERENCE_SF2")
    renders = {}
    for take, name in ((2, "deadpan"), (3, "performance")):
        wav = work / f"wrela-{name}.wav"
        run(WRELA, "audio", PIECE, "wav", str(wav), "--take", str(take))
        renders[f"wrela piano, {name}"] = wav
        mid = work / f"{name}.mid"
        run(WRELA, "audio", PIECE, "midi", str(mid), "--take", str(take))
        ref = work / f"sampled-{name}.wav"
        run("fluidsynth", "-ni", "-q", "-R", "0", "-C", "0", "-g", "1.0", "-r", "48000", "-O", "float", "-F", str(ref), SF2, str(mid))
        renders[f"sampled piano, {name}"] = ref
    run(WRELA, "audio", PIECE, "wav", str(out / "wrela-performance-with-room.wav"), "--take", "1")

    loaded = {k: read_wav(v) for k, v in renders.items()}
    # All as long as the longest, with silence: a file's length or size gives nothing away.
    longest = max(len(s) for s, _, _ in loaded.values())
    loaded = {k: (s + [0.0] * (longest - len(s)), c, r) for k, (s, c, r) in loaded.items()}
    levels = {k: rms(s) for k, (s, _, _) in loaded.items()}
    peaks = {k: max(abs(x) for x in s) for k, (s, _, _) in loaded.items()}
    # One RMS level for all four: as loud as it can be with every peak at -1 dBFS or below.
    ceiling = 10 ** (-1 / 20)
    target = min(levels[k] * ceiling / peaks[k] for k in loaded)
    order = list(loaded)
    secrets.SystemRandom().shuffle(order)
    key = {}
    for letter, k in zip("ABCD", order):
        samples, channels, rate = loaded[k]
        gain = target / levels[k]
        write_wav(out / f"{letter}.wav", [x * gain for x in samples], channels, rate)
        key[letter] = {"render": k, "gain_db": round(20 * __import__("math").log10(gain), 2), "seconds": round(len(samples) / channels / rate, 1)}
    (out / "key-open-after-ranking.json").write_text(json.dumps(key, indent=2) + "\n")
    print(json.dumps({"files": [f"{out}/{l}.wav" for l in "ABCD"], "rms_dbfs": round(20 * __import__("math").log10(target), 1), "key": str(out / "key-open-after-ranking.json")}, indent=2))


if __name__ == "__main__":
    main()
