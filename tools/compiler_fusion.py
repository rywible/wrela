#!/usr/bin/env python3
"""Generate a bounded GPU dispatch-fusion experiment and an exact CPU oracle.

This is a fixed source-to-source prototype, not a general compiler pass. Each
stage reads only its invocation's preceding result. Intermediates are unobserved;
there are no atomics, barriers or cross-element dependencies. Both packages keep
two buffers, a then b. The final buffer is a for odd stage counts, b for even.
Every frame starts from the invocation ID. No build or GPU work runs here.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct


MASK = (1 << 32) - 1
SEED = "(id.x.wrapping_mul(747796405) ^ (id.x >> 4) ^ 2891336453)"


def parameters(stage):
    return 1664525 + stage * 2, (stage * 1013904223 + 17) & MASK, stage % 15 + 1


def expression(stage, incoming):
    mul, add, shift = parameters(stage)
    return (
        f"    let p{stage}: u32 = ({incoming}).wrapping_mul({mul}).wrapping_add({add})\n"
        f"    let v{stage}: u32 = p{stage} ^ (p{stage} >> {shift})\n"
    )


def generate(output, elements, stages):
    common = f"""// Generated dispatch-fusion experiment; exact u32 arithmetic.
use std::gpu::{{GlobalId, GpuBuffer, Slots, buffer, dispatch, label}}
const N: u32 = {elements}
pub struct State {{ a: GpuBuffer<u32>, b: GpuBuffer<u32> }}
pub fn init() -> State {{ State {{ a: buffer(N), b: buffer(N) }} }}

"""
    frame = (
        "pub fn frame(s: mut State, time: f32, width: u32, height: u32) {\n"
        '    label("fusion chain")\n'
    )
    before, commands = [common], []
    after = [common, "@compute(64)\nfn fused(out: mut Slots<u32>, id: GlobalId) {\n"]
    for stage in range(stages):
        args = "out: mut Slots<u32>, id: GlobalId"
        if stage:
            args = "src: [u32], " + args
        before.append(f"@compute(64)\nfn stage_{stage}({args}) {{\n")
        before.append(expression(stage, "src[id.x]" if stage else SEED))
        before.append(f"    out[id] = v{stage}\n}}\n\n")
        dst, src = ("a", "b") if stage % 2 == 0 else ("b", "a")
        binding = f"out: mut s.{dst}"
        if stage:
            binding = f"s.{src}, " + binding
        commands.append(f"    dispatch(stage_{stage}.bind({binding}), over: N)\n")
        after.append(expression(stage, f"v{stage - 1}" if stage else SEED))
    before.extend([frame, *commands, "}\n"])
    after.extend([
        f"    out[id] = v{stages - 1}\n}}\n\n", frame,
        f"    dispatch(fused.bind(out: mut s.{dst}), over: N)\n}}\n",
    ])
    for variant, source in [("before", before), ("after", after)]:
        package = output / variant
        package.mkdir(parents=True, exist_ok=True)
        (package / "main.wrela").write_text("".join(source))
        (package / "wrela.toml").write_text(f'[package]\nname = "fusion_{variant}"\n')
    expected = bytearray()
    for index in range(elements):
        value = ((index * 747796405) ^ (index >> 4) ^ 2891336453) & MASK
        for stage in range(stages):
            mul, add, shift = parameters(stage)
            value = (value * mul + add) & MASK
            value ^= value >> shift
        expected.extend(struct.pack("<I", value))
    (output / "expected.u32le").write_bytes(expected)
    print(json.dumps({
        "output": str(output), "elements": elements, "stages": stages,
        "result_buffer_index": (stages - 1) % 2,
        "sha256": hashlib.sha256(expected).hexdigest(),
        "logical_accesses_before": (2 * stages - 1) * elements,
        "logical_accesses_after": elements,
    }, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--elements", type=int, default=262144)
    parser.add_argument("--stages", type=int, default=32)
    args = parser.parse_args()
    if not 1 <= args.elements <= 1048576:
        parser.error("--elements must be between 1 and 1048576")
    if not 1 <= args.stages <= 64:
        parser.error("--stages must be between 1 and 64")
    generate(args.output, args.elements, args.stages)


if __name__ == "__main__":
    main()
