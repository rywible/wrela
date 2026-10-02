// The cases of wrela-abi's stream tests (runtime/abi/src/stream/tests.rs), run against the
// TypeScript mirror: both hosts must agree on every one.

import { describe, expect, test } from "bun:test";
import { testMode, VERSION } from "../src/abi.ts";
import { StateHash } from "../src/hash.ts";
import {
  type Command,
  commandName,
  commandOpcode,
  DecodeError,
  type DecodeErrorDetail,
  decode,
  decodeLocated,
  EncodeError,
  Encoder,
  encode,
  FrameCheck,
  HEADER_LEN,
  header,
  MAX_FRAME_VERTICES,
  opcode,
  opcodeName,
  SequenceError,
  type SequenceErrorDetail,
} from "../src/stream.ts";
import { thrown } from "./helpers.ts";

const draw = (uniforms: Uint8Array): Command => ({
  kind: "Draw",
  pipeline: 7,
  vertexCount: 3,
  instanceCount: 1,
  uniforms,
});
const begin = (): Command => ({ kind: "BeginScreenPass", clear: [0.0, 0.25, 0.5, 1.0] });
const present: Command = { kind: "Present" };
const range = (n: number) => Uint8Array.from({ length: n }, (_, i) => i);

/** One of each command, as one buffer. */
const sample = (): Command[] => [begin(), draw(range(16)), draw(new Uint8Array()), present];

/** Little-endian words, for writing expected bytes. */
function words(values: readonly number[]): Uint8Array {
  const bytes = new Uint8Array(values.length * 4);
  const view = new DataView(bytes.buffer);
  for (const [i, v] of values.entries()) {
    view.setUint32(i * 4, v >>> 0, true);
  }
  return bytes;
}

const f32Bits = (x: number) => {
  const view = new DataView(new ArrayBuffer(4));
  view.setFloat32(0, x, true);
  return view.getUint32(0, true);
};

function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** Commands compared by value: a decoded draw's uniforms are a view, so compare their bytes. */
function plain(commands: readonly Command[]): unknown[] {
  return commands.map((c) => {
    switch (c.kind) {
      case "Draw":
        return { ...c, uniforms: [...c.uniforms] };
      case "BeginScreenPass":
        return { ...c, clear: [...c.clear] };
      default:
        return c;
    }
  });
}

function decodeError(bytes: Uint8Array): DecodeErrorDetail {
  const error = thrown(() => decode(bytes));
  expect(error).toBeInstanceOf(DecodeError);
  return (error as DecodeError).detail;
}

const hex = (bytes: Uint8Array) => [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");

describe("command buffers", () => {
  test("the header is WR then the version", () => {
    expect([...header(0)]).toEqual([0x57, 0x52, 0x00, 0x00]);
    expect(new DataView(header(0).buffer).getUint32(0, true)).toBe(0x0000_5257);
    expect([...header(0x0102)]).toEqual([0x57, 0x52, 0x02, 0x01]);
    expect(new Encoder().bytes()).toEqual(header(VERSION));
  });

  test("each command has exactly these bytes", () => {
    const expected = concat(
      header(0),
      words([opcode.BEGIN_SCREEN_PASS, 16, f32Bits(0), f32Bits(0.25), f32Bits(0.5), f32Bits(1)]),
      words([opcode.DRAW, 12 + 16, 7, 3, 1]),
      range(16),
      words([opcode.DRAW, 12, 7, 3, 1]),
      words([opcode.PRESENT, 0]),
    );
    expect(encode(sample())).toEqual(expected);
    expect([...expected.subarray(4, 12)]).toEqual([1, 0, 0, 0, 16, 0, 0, 0]);
  });

  test("round trips", () => {
    const cases: Command[][] = [
      [],
      [present],
      [begin()],
      [draw(new Uint8Array(4))],
      [draw(new Uint8Array(65536).fill(0xff))],
      sample(),
      [
        {
          kind: "BeginScreenPass",
          clear: [-0.0, 3.4028234663852886e38, 1.1754943508222875e-38, Math.fround(-1e30)],
        },
        {
          kind: "Draw",
          pipeline: 0xffff_ffff,
          vertexCount: 0,
          instanceCount: 0xffff_ffff,
          uniforms: range(9).subarray(1),
        },
      ],
    ];
    for (const commands of cases) {
      expect(plain(decode(encode(commands)))).toEqual(plain(commands));
    }
  });

  test("negative zero survives bit for bit", () => {
    const [command] = decode(encode([{ kind: "BeginScreenPass", clear: [-0.0, 0, 0, 0] }]));
    expect(command?.kind === "BeginScreenPass" && Object.is(command.clear[0], -0)).toBe(true);
  });

  test("the encoder refuses what the decoder would reject", () => {
    for (const bad of [NaN, Infinity, -Infinity]) {
      for (let component = 0; component < 4; component++) {
        const clear: [number, number, number, number] = [0, 0, 0, 0];
        clear[component] = bad;
        const encoder = new Encoder();
        const error = thrown(() => encoder.push({ kind: "BeginScreenPass", clear }));
        expect(error).toBeInstanceOf(EncodeError);
        expect((error as EncodeError).detail).toMatchObject({ kind: "NonFiniteClear", component });
        // A refused command leaves the buffer as it was.
        expect(encoder.bytes()).toEqual(header(VERSION));
      }
    }
    for (const len of [1, 2, 3, 5, 17]) {
      const encoder = new Encoder().push(present);
      const before = encoder.bytes();
      const error = thrown(() => encoder.push(draw(new Uint8Array(len))));
      expect((error as EncodeError).detail).toEqual({ kind: "UnalignedUniforms", len });
      expect(encoder.bytes()).toEqual(before);
    }
    // What Rust's u32 guarantees, the TypeScript encoder checks.
    for (const pipeline of [-1, 1.5, 2 ** 32, NaN]) {
      const encoder = new Encoder();
      expect(
        thrown(() => encoder.push({ ...draw(new Uint8Array()), pipeline } as Command)),
      ).toBeInstanceOf(RangeError);
      expect(encoder.bytes()).toEqual(header(VERSION));
    }
  });

  test("a header alone is an empty buffer", () => {
    expect(decode(header(0))).toEqual([]);
  });

  test("short or foreign headers are rejected", () => {
    for (let len = 0; len < 4; len++) {
      expect(decodeError(header(0).subarray(0, len))).toEqual({ kind: "MissingHeader", len });
    }
    expect(decodeError(Uint8Array.of(0x52, 0x57, 0, 0))).toEqual({
      kind: "BadMagic",
      found: [0x52, 0x57],
    });
    expect(decodeError(new Uint8Array(4))).toEqual({ kind: "BadMagic", found: [0, 0] });
    expect(decodeError(header(1))).toEqual({ kind: "UnsupportedVersion", found: 1 });
    expect(decodeError(header(0xffff))).toEqual({ kind: "UnsupportedVersion", found: 0xffff });
  });

  test("every truncation is an error or a whole prefix of commands", () => {
    const commands = sample();
    const bytes = encode(commands);
    const ends = [HEADER_LEN];
    const encoder = new Encoder();
    for (const command of commands) {
      ends.push(encoder.push(command).bytes().length);
    }
    for (let cut = 0; cut < bytes.length; cut++) {
      const n = ends.indexOf(cut);
      if (n >= 0) {
        expect(plain(decode(bytes.subarray(0, cut)))).toEqual(plain(commands.slice(0, n)));
      } else {
        expect(thrown(() => decode(bytes.subarray(0, cut)))).toBeInstanceOf(DecodeError);
      }
    }
  });

  test("a command header cut short is truncated", () => {
    expect(decodeError(concat(header(0), words([opcode.PRESENT])))).toEqual({
      kind: "TruncatedCommand",
      offset: 4,
      remaining: 4,
    });
  });

  test("payload lengths must be aligned and in bounds", () => {
    for (const length of [1, 2, 3, 13, 15]) {
      expect(decodeError(concat(header(0), words([opcode.DRAW, length, 0, 0, 0, 0])))).toEqual({
        kind: "UnalignedLength",
        offset: 4,
        opcode: opcode.DRAW,
        length,
      });
    }
    for (const length of [4, 16, 0xffff_fffc]) {
      expect(
        decodeError(concat(header(0), words([opcode.PRESENT, 0, opcode.DRAW, length]))),
      ).toEqual({
        kind: "PayloadPastEnd",
        offset: 12,
        opcode: opcode.DRAW,
        length,
        available: 0,
      });
    }
  });

  test("unknown opcodes are errors, never skipped", () => {
    for (const op of [0, 4, 5, 255, 0x0100, 0xffff_ffff]) {
      expect(decodeError(concat(header(0), words([op, 0]), words([opcode.PRESENT, 0])))).toEqual({
        kind: "UnknownOpcode",
        offset: 4,
        opcode: op,
      });
    }
  });

  test("each opcode takes only its payload lengths", () => {
    const cases: [number, number[], string][] = [
      [opcode.BEGIN_SCREEN_PASS, [0, 4, 12, 20, 32], "exactly 16"],
      [opcode.DRAW, [0, 4, 8], "at least 12"],
      [opcode.PRESENT, [4, 8, 16], "exactly 0"],
    ];
    for (const [op, lengths, expected] of cases) {
      for (const length of lengths) {
        expect(decodeError(concat(header(0), words([op, length]), new Uint8Array(length)))).toEqual(
          {
            kind: "WrongLength",
            offset: 4,
            opcode: op,
            length,
            expected,
          },
        );
      }
    }
  });

  test("a non-finite clear is rejected at its command", () => {
    for (const bits of [0x7fc0_0000, 0x7f80_0000, 0xff80_0000, 0x7f80_0001]) {
      for (let component = 0; component < 4; component++) {
        const clear = [0, 0, 0, 0];
        clear[component] = bits;
        const bytes = concat(
          encode([present]),
          words([opcode.BEGIN_SCREEN_PASS, 16]),
          words(clear),
        );
        expect(decodeError(bytes)).toEqual({ kind: "NonFiniteClear", offset: 12, component });
      }
    }
  });

  test("single-byte corruptions never throw anything else and never decode to other bytes", () => {
    const bytes = encode(sample());
    for (let at = 0; at < bytes.length; at++) {
      for (const value of [0, 1, 2, 3, 4, 0x7f, 0x80, 0xff]) {
        const corrupt = bytes.slice();
        corrupt[at] = value;
        let commands: Command[] | undefined;
        try {
          commands = decode(corrupt);
        } catch (e) {
          expect(e).toBeInstanceOf(DecodeError);
        }
        if (commands !== undefined) {
          expect(encode(commands)).toEqual(corrupt);
        }
      }
    }
  });

  test("decodeLocated says where each command starts", () => {
    expect(decodeLocated(encode(sample())).map((c) => c.offset)).toEqual([4, 28, 64, 84]);
  });

  test("errors report their offset and read as messages", () => {
    const details: DecodeErrorDetail[] = [
      { kind: "MissingHeader", len: 2 },
      { kind: "BadMagic", found: [0x58, 0x59] },
      { kind: "UnsupportedVersion", found: 3 },
      { kind: "TruncatedCommand", offset: 12, remaining: 4 },
      { kind: "UnalignedLength", offset: 12, opcode: 2, length: 3 },
      { kind: "PayloadPastEnd", offset: 12, opcode: 9, length: 8, available: 4 },
      { kind: "UnknownOpcode", offset: 12, opcode: 9 },
      { kind: "WrongLength", offset: 12, opcode: 3, length: 4, expected: "exactly 0" },
      { kind: "NonFiniteClear", offset: 12, component: 1 },
    ];
    const errors = details.map((d) => new DecodeError(d));
    expect(errors.map((e) => e.offset())).toEqual([0, 0, 2, 12, 12, 12, 12, 12, 12]);
    for (const { message } of errors) {
      expect(message.endsWith(".")).toBe(false);
      expect(message[0]).toBe(message[0]?.toLowerCase());
    }
    // The same words as wrela-abi's Display.
    expect(
      new DecodeError({
        kind: "WrongLength",
        offset: 4,
        opcode: 3,
        length: 4,
        expected: "exactly 0",
      }).message,
    ).toBe("at byte 4: PRESENT has a 4-byte payload; it takes exactly 0 bytes");
    expect(new DecodeError({ kind: "UnknownOpcode", offset: 4, opcode: 0 }).message).toBe(
      "at byte 4: unknown opcode 0",
    );
    expect(new DecodeError({ kind: "BadMagic", found: [0x58, 0x59] }).message).toBe(
      "a command buffer starts with 0x58 0x59, not `WR`",
    );
    expect(
      new EncodeError({ kind: "NonFiniteClear", component: 2, value: -Infinity }).message,
    ).toBe("clear colour component 2 is -inf, which isn't finite");
  });
});

describe("the frame rules", () => {
  const run = (commands: Command[]): SequenceErrorDetail | undefined => {
    const check = new FrameCheck();
    try {
      for (const command of commands) {
        check.command(command);
      }
      check.finish();
      return undefined;
    } catch (e) {
      expect(e).toBeInstanceOf(SequenceError);
      return (e as SequenceError).detail;
    }
  };

  test("a frame is one pass, some draws and present", () => {
    for (let draws = 0; draws < 3; draws++) {
      const check = new FrameCheck();
      check.command(begin());
      for (let i = 0; i < draws; i++) {
        check.command(draw(new Uint8Array()));
      }
      expect(check.isPresented()).toBe(false);
      check.command(present);
      expect(check.isPresented()).toBe(true);
      check.finish();
    }
  });

  test("frames that break the rules are errors", () => {
    const none = new Uint8Array();
    expect(run([])).toEqual({ kind: "NotPresented" });
    expect(run([begin()])).toEqual({ kind: "NotPresented" });
    expect(run([begin(), draw(none)])).toEqual({ kind: "NotPresented" });
    expect(run([draw(none)])).toEqual({ kind: "DrawOutsidePass" });
    expect(run([present])).toEqual({ kind: "PresentWithoutPass" });
    expect(run([begin(), begin()])).toEqual({ kind: "SecondScreenPass" });
    for (const after of [begin(), draw(none), present]) {
      expect(run([begin(), present, after])).toEqual({
        kind: "AfterPresent",
        command: commandName(after),
      });
    }
  });

  test("a frame draws at most its vertex budget", () => {
    const counted = (vertexCount: number, instanceCount: number): Command => ({
      kind: "Draw",
      pipeline: 0,
      vertexCount,
      instanceCount,
      uniforms: new Uint8Array(),
    });
    const draws = (pairs: [number, number][]) => [
      begin(),
      ...pairs.map(([v, i]) => counted(v, i)),
      present,
    ];
    const max = 2 ** 32 - 1;
    expect(MAX_FRAME_VERTICES).toBe(1048576n);
    expect(run(draws([[1 << 20, 1]]))).toBeUndefined();
    expect(run(draws([[1 << 10, 1 << 10]]))).toBeUndefined();
    expect(
      run(
        draws([
          [3, 0],
          [0, max],
          [1 << 20, 1],
        ]),
      ),
    ).toBeUndefined();
    expect(
      run(
        draws([
          [1 << 20, 1],
          [1, 1],
        ]),
      ),
    ).toEqual({ kind: "TooManyVertices", vertices: 1048577n });
    expect(run(draws([[max, max]]))).toEqual({
      kind: "TooManyVertices",
      vertices: 18446744065119617025n,
    });
    expect(run(draws([[3, 1 << 19]]))).toEqual({ kind: "TooManyVertices", vertices: 1572864n });
    expect(new SequenceError({ kind: "TooManyVertices", vertices: 1572864n }).message).toBe(
      "this frame's DRAWs draw 1572864 vertices (vertex_count × instance_count, summed), over the 1048576 a frame may draw",
    );
    // A rejected draw doesn't count.
    const check = new FrameCheck();
    check.command(begin());
    expect(thrown(() => check.command(counted(max, 2)))).toBeInstanceOf(SequenceError);
    check.command(counted(1 << 20, 1));
  });

  test("a rejected command leaves the check unchanged", () => {
    const check = new FrameCheck();
    check.command(begin());
    expect(thrown(() => check.command(begin()))).toBeInstanceOf(SequenceError);
    check.command(present);
    expect(check.isPresented()).toBe(true);
  });

  test("names match the spec", () => {
    expect(commandName(begin())).toBe("BEGIN_SCREEN_PASS");
    expect(commandName(draw(new Uint8Array()))).toBe("DRAW");
    expect(commandName(present)).toBe("PRESENT");
    expect(commandOpcode(present)).toBe(3);
    expect(opcodeName(0)).toBeUndefined();
    expect(opcodeName(4)).toBeUndefined();
    for (const message of [
      new SequenceError({ kind: "DrawOutsidePass" }).message,
      new SequenceError({ kind: "NotPresented" }).message,
      new EncodeError({ kind: "UnalignedUniforms", len: 3 }).message,
    ]) {
      expect(message.endsWith(".")).toBe(false);
    }
    expect(new SequenceError({ kind: "AfterPresent", command: "DRAW" }).message).toBe(
      "a DRAW after PRESENT: PRESENT ends the frame",
    );
  });
});

/**
 * wrela-abi's reference stream: 60 test-mode frames shaped like first light's, each a pass
 * cleared to opaque black, one 3-vertex draw of pipeline 0 whose 16-byte uniform is
 * `[width, height, time, 0]` as f32s, and a present, each command in its own buffer.
 */
test("the reference stream hashes as in wrela-abi", () => {
  const hash = new StateHash();
  let frame59Draw: Uint8Array = new Uint8Array();
  for (let i = 0; i < testMode.FRAMES; i++) {
    const uniforms = new Uint8Array(16);
    const view = new DataView(uniforms.buffer);
    view.setFloat32(0, testMode.WIDTH, true);
    view.setFloat32(4, testMode.HEIGHT, true);
    view.setFloat32(8, testMode.time(i), true);
    const check = new FrameCheck();
    const commands: Command[] = [
      { kind: "BeginScreenPass", clear: [0, 0, 0, 1] },
      { kind: "Draw", pipeline: 0, vertexCount: 3, instanceCount: 1, uniforms },
      present,
    ];
    for (const command of commands) {
      check.command(command);
      const buffer = encode([command]);
      if (i === 59 && command.kind === "Draw") {
        frame59Draw = buffer;
      }
      hash.update(buffer);
    }
    check.finish();
  }
  expect(hex(frame59Draw)).toBe(
    "57520000020000001c0000000000000003000000010000000000f04400008744bcbb7b3f00000000",
  );
  expect(hash.hex()).toBe("ee6a915168bafdc0");
});
