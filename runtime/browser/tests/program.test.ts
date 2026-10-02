// A running program: what reaches the sink, and the errors every bad submit becomes.

import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { testMode } from "../src/abi.ts";
import { Manifest } from "../src/manifest.ts";
import { HostError, Program } from "../src/program.ts";
import { encode, header, opcode } from "../src/stream.ts";
import { contractManifest, FIXTURE, rejected, thrown } from "./helpers.ts";
import { begin, draw, hex, present, RecordingSink, submitting } from "./programs.ts";

const manifest = Manifest.fromJson(await contractManifest());

async function start(wasm: Uint8Array, sink = new RecordingSink()) {
  return { program: await Program.instantiate(wasm, manifest, sink), sink };
}

/** Runs frame 0 of a program and returns the error message. */
async function failure(wasm: Uint8Array, sink = new RecordingSink()): Promise<string> {
  const { program } = await start(wasm, sink);
  const error = thrown(() => program.frame(0, 0, 1920, 1080));
  expect(error).toBeInstanceOf(HostError);
  return (error as Error).message;
}

const words = (values: number[]) => {
  const bytes = new Uint8Array(values.length * 4);
  const view = new DataView(bytes.buffer);
  for (const [i, v] of values.entries()) {
    view.setUint32(i * 4, v >>> 0, true);
  }
  return bytes;
};
const concat = (...parts: Uint8Array[]) => Uint8Array.from(parts.flatMap((p) => [...p]));

describe("the first-light fixture", () => {
  test("runs 60 test-mode frames to wrela-abi's reference hash", async () => {
    const { program, sink } = await start(await readFile(join(FIXTURE, "program.wasm")));
    for (let i = 0; i < testMode.FRAMES; i++) {
      program.frame(i, testMode.time(i), testMode.WIDTH, testMode.HEIGHT);
    }
    expect(program.hash.hex()).toBe("ee6a915168bafdc0");
    expect(sink.log.length).toBe(3 * testMode.FRAMES);
    expect(sink.log.slice(0, 3)).toEqual([
      "begin 0 0 0 1",
      "draw 0 3 1 0000f0440000874400000000" + "00000000",
      "present",
    ]);
    expect(sink.log[sink.log.length - 2]).toBe("draw 0 3 1 0000f04400008744bcbb7b3f00000000");
  });
});

describe("good streams", () => {
  test("one buffer or many, frame after frame", async () => {
    const uniforms = Uint8Array.from({ length: 16 }, (_, i) => i);
    for (const buffers of [
      [encode([begin, draw(0, uniforms), present])],
      [
        encode([begin]),
        encode([]),
        encode([draw(0, uniforms), draw(0, uniforms)]),
        encode([present]),
      ],
    ]) {
      const { program, sink } = await start(await submitting(buffers));
      for (let i = 0; i < 3; i++) {
        program.frame(i, 0, 1, 1);
      }
      expect(sink.log.filter((l) => l === "present").length).toBe(3);
      expect(sink.log).toContain(`draw 0 3 1 ${hex(uniforms)}`);
    }
  });

  test("memory may grow between submits; the host reads it afresh", async () => {
    const buffer = encode([begin, draw(), present]);
    // Grow by a page, copy the buffer into it, and submit it from there.
    const wasm = await submitting([], {
      extra: `(data (i32.const 0) "${[...buffer].map((b) => `\\${b.toString(16).padStart(2, "0")}`).join("")}")`,
      before: `(drop (memory.grow (i32.const 1)))
        (memory.copy (i32.sub (i32.mul (memory.size) (i32.const 65536)) (i32.const 1024)) (i32.const 0) (i32.const ${buffer.length}))
        (call $submit (i32.sub (i32.mul (memory.size) (i32.const 65536)) (i32.const 1024)) (i32.const ${buffer.length}))`,
    });
    const { program, sink } = await start(wasm);
    program.frame(0, 0, 1, 1);
    program.frame(1, 0, 1, 1);
    expect(sink.log.filter((l) => l === "present").length).toBe(2);
  });
});

describe("bad streams are errors, with the frame, the buffer and the byte", () => {
  test("a buffer that doesn't decode", async () => {
    const bad = concat(header(0), words([9, 0]));
    const sink = new RecordingSink();
    expect(await failure(await submitting([encode([begin]), bad]), sink)).toBe(
      "frame 0, buffer 1: at byte 4: unknown opcode 9",
    );
    // The first buffer reached the sink; nothing after the failure did.
    expect(sink.log).toEqual(["begin 0 0 0 1"]);
  });

  test("no command of a bad buffer reaches the GPU", async () => {
    const sink = new RecordingSink();
    const buffer = concat(encode([begin, draw()]), words([opcode.PRESENT, 4, 0]));
    expect(await failure(await submitting([buffer]), sink)).toBe(
      "frame 0, buffer 0: at byte 64: PRESENT has a 4-byte payload; it takes exactly 0 bytes",
    );
    expect(sink.log).toEqual([]);
  });

  test("an empty or short buffer, or the wrong version", async () => {
    expect(await failure(await submitting([new Uint8Array()]))).toBe(
      "frame 0, buffer 0: a command buffer of 0 bytes has no room for the 4-byte header",
    );
    expect(await failure(await submitting([header(1)]))).toBe(
      "frame 0, buffer 0: command buffer version 1 isn't supported; this host reads version 0",
    );
    expect(await failure(await submitting([new Uint8Array(16)]))).toBe(
      "frame 0, buffer 0: a command buffer starts with 0x00 0x00, not `WR`",
    );
  });

  test("a break of the frame rules", async () => {
    expect(await failure(await submitting([encode([draw()])]))).toBe(
      "frame 0, buffer 0, at byte 4: a DRAW before BEGIN_SCREEN_PASS: draws go inside the screen pass",
    );
    expect(await failure(await submitting([encode([begin]), encode([present, present])]))).toBe(
      "frame 0, buffer 1, at byte 12: a PRESENT after PRESENT: PRESENT ends the frame",
    );
    expect(await failure(await submitting([encode([begin, draw()])]))).toBe(
      "frame 0: `frame` returned without PRESENT: every frame ends with one",
    );
    expect(await failure(await submitting([]))).toBe(
      "frame 0: `frame` returned without PRESENT: every frame ends with one",
    );
  });

  test("a draw the manifest doesn't allow", async () => {
    expect(await failure(await submitting([encode([begin, draw(9), present])]))).toBe(
      "frame 0, buffer 0, at byte 28: a DRAW names pipeline 9, which the manifest doesn't have",
    );
    expect(
      await failure(await submitting([encode([begin, draw(0, new Uint8Array(12)), present])])),
    ).toBe(
      "frame 0, buffer 0, at byte 28: a DRAW with pipeline 0 carries 12 bytes of uniforms; the pipeline takes 16",
    );
  });

  test("a range outside memory", async () => {
    const outside = (ptr: number, len: number) =>
      submitting([], { before: `(call $submit (i32.const ${ptr}) (i32.const ${len}))` });
    expect(await failure(await outside(65530, 100))).toBe(
      "frame 0, buffer 0: the buffer at 65530..65630 is outside the program's 65536-byte memory",
    );
    // A u32 crosses as the i32 with the same bits: -4 is 4294967292, far outside.
    expect(await failure(await outside(-4, 8))).toBe(
      "frame 0, buffer 0: the buffer at 4294967292..4294967300 is outside the program's 65536-byte memory",
    );
    expect(await failure(await outside(0, -1))).toContain(
      "outside the program's 65536-byte memory",
    );
  });

  test("a trap", async () => {
    expect(
      await failure(await submitting([encode([begin])], { before: "unreachable" })),
    ).toStartWith("frame 0: the program trapped: ");
  });

  test("a sink that fails, as a GPU call might", async () => {
    const sink = new RecordingSink();
    sink.draw = () => {
      throw new Error("the device is gone");
    };
    expect(await failure(await submitting([encode([begin, draw(), present])]), sink)).toBe(
      "frame 0, buffer 0, at byte 28: the device is gone",
    );
  });

  test("after a failure the program is never called again", async () => {
    let calls = 0;
    const sink = new RecordingSink();
    const original = sink.beginScreenPass.bind(sink);
    sink.beginScreenPass = (clear) => {
      calls++;
      original(clear);
    };
    const { program } = await start(await submitting([encode([begin])]), sink);
    const first = thrown(() => program.frame(0, 0, 1, 1));
    expect(thrown(() => program.frame(1, 0, 1, 1))).toBe(first);
    expect(calls).toBe(1);
  });

  test("submit outside frame, while starting", async () => {
    const wasm = await submitting([], {
      extra: `(func $start (call $submit (i32.const 0) (i32.const 4))) (start $start)`,
    });
    const error = await rejected(() => Program.instantiate(wasm, manifest, new RecordingSink()));
    expect((error as Error).message).toBe(
      "the program failed to start: the program called `submit` while starting, outside `frame`",
    );
  });

  test("bytes that aren't WASM", async () => {
    const error = await rejected(() =>
      Program.instantiate(Uint8Array.of(1, 2, 3), manifest, new RecordingSink()),
    );
    expect(error).toBeInstanceOf(HostError);
    expect((error as Error).message).toStartWith("the program isn't a valid WASM module: ");
  });
});
