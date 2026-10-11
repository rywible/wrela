import { expect, test } from "bun:test";
import { Checker } from "../src/check.ts";
import { NONE } from "../src/abi.gen.ts";
import type { Binding, Command } from "../src/stream.ts";
import { shapes } from "./fixtures.ts";

for (const op of ["Dispatch", "Draw"] as const) {
  test(`writable bindings are checked per ${op.toLowerCase()}, including after a rejection`, () => {
    const m = shapes();
    const pipeline = op === "Dispatch" ? 1 : 0;
    for (const b of m.pipelines[pipeline]!.bindings) b.kind = "read_write";
    const checker = new Checker(m);
    for (const handle of [1, 2]) checker.check({ op: "CreateBuffer", handle, size: 64 });
    if (op === "Draw") checker.check({ op: "BeginScreenPass", clear: [0, 0, 0, 1] });
    const command = (bindings: Binding[]): Command => op === "Dispatch"
      ? { op, pipeline, groups: [1, 1, 1], bindings, uniforms: new Uint8Array(16) }
      : { op, pipeline, vertices: 3, instances: 1, bindings, uniforms: new Uint8Array(16) };
    const a = { handle: 1, offset: 0, size: 64 };
    const b = { handle: 2, offset: 0, size: 64 };
    checker.check(command([a, b]));
    checker.check(command([a, b]));
    expect(() => checker.check(command([a, a]))).toThrow(`buffer 1 is bound read-write twice in one ${op.toLowerCase()}`);
    expect(() => checker.check(command([a, b]))).not.toThrow();
  });
}

test("usage conflicts persist within a pass and reset at the next pass", () => {
  const checker = new Checker(shapes());
  for (const handle of [1, 2]) checker.check({ op: "CreateBuffer", handle, size: 64 });
  const a = { handle: 1, offset: 0, size: 64 };
  const b = { handle: 2, offset: 0, size: 64 };
  const draw = (bindings: Binding[]): Command => ({ op: "Draw", pipeline: 0, vertices: 3, instances: 1, bindings, uniforms: new Uint8Array(16) });
  checker.check({ op: "BeginScreenPass", clear: [0, 0, 0, 1] });
  checker.check(draw([a, b]));
  checker.check(draw([a, b]));
  expect(() => checker.check(draw([b, a]))).toThrow("buffer 2 is used both read-only and read-write in one pass");
  // The rejected reversed draw leaves the previous pass scope intact.
  expect(() => checker.check(draw([a, b]))).not.toThrow();
  checker.check({ op: "Present" });
  checker.check({ op: "BeginScreenPass", clear: [0, 0, 0, 1] });
  expect(() => checker.check(draw([b, a]))).not.toThrow();
});


test("rejected pass targets keep the previous attachments and pipeline targets", () => {
  const m = shapes();
  const draw = m.pipelines[0]!;
  if (draw.kind !== "render") throw new Error("render fixture");
  draw.targets = [{ color: "rgba8unorm", depth: false }];
  draw.bindings = [
    { binding: 1, kind: "texture", stage: "both", format: "rgba8unorm" },
    { binding: 2, kind: "read_write", stage: "both", format: null },
  ];
  const checker = new Checker(m);
  checker.check({ op: "CreateBuffer", handle: 2, size: 64 });
  for (const handle of [10, 13]) checker.check({ op: "CreateTexture", handle, width: 4, height: 4, depth: 0, format: "rgba8unorm", writable: false });
  checker.check({ op: "CreateTexture", handle: 11, width: 4, height: 4, depth: 0, format: "rgba16float", writable: false });
  checker.check({ op: "CreateTexture", handle: 12, width: 8, height: 8, depth: 0, format: "depth32float", writable: false });
  const pass = { color: 10, depth: NONE, keepColor: false, keepDepth: false, join: false, clear: [0, 0, 0, 1] as [number, number, number, number], clearDepth: 1 };
  checker.check({ op: "BeginPass", pass });
  expect(() => checker.check({ op: "BeginPass", pass: { ...pass, color: 11, depth: 12 } })).toThrow("the pass's colour and depth targets differ in size");
  const command = (texture: number): Command => ({ op: "Draw", pipeline: 0, vertices: 3, instances: 1, bindings: [
    { handle: texture, offset: 0, size: 0 }, { handle: 2, offset: 0, size: 64 },
  ], uniforms: new Uint8Array(16) });
  expect(() => checker.check(command(10))).toThrow("texture 10 is the pass's target");
  expect(() => checker.check(command(13))).not.toThrow();
});

test("a rejected indexed draw restores aliased bindings and indirect reads once", () => {
  const m = shapes();
  const reversed = structuredClone(m.pipelines[0]!);
  reversed.bindings[0]!.kind = "read_write";
  reversed.bindings[1]!.kind = "read";
  m.pipelines.push(reversed);
  const checker = new Checker(m);
  for (const handle of [1, 2]) checker.check({ op: "CreateBuffer", handle, size: 64 });
  checker.check({ op: "BeginScreenPass", clear: [0, 0, 0, 1] });
  const bindings = [{ handle: 1, offset: 0, size: 64 }, { handle: 2, offset: 0, size: 64 }];
  expect(() => checker.check({ op: "DrawIndexedIndirect", pipeline: 0, indices: 1, index_offset: 0, index_size: 4, arguments: 1, offset: 60, bindings, uniforms: new Uint8Array(16) })).toThrow("reading arguments: 20 bytes at offset 60 overruns buffer 1");
  expect(() => checker.check({ op: "Draw", pipeline: 2, vertices: 3, instances: 1, bindings, uniforms: new Uint8Array(16) })).not.toThrow();
});
