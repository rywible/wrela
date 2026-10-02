// Command buffers: what a program passes to `submit`. The TypeScript mirror of wrela-abi's
// `stream` module (runtime/abi/src/stream.rs): the same checks in the same order, the same errors
// and the same messages, so both hosts reject the same buffers with the same words.
//
// A buffer is a 4-byte header (`WR` and the version) followed by commands, each an opcode, a
// payload length and the payload. Everything is little-endian. `decode` checks one buffer;
// `FrameCheck` checks the order of commands across all the buffers of one `frame` call.

import { VERSION } from "./abi.ts";

/** The first two bytes of every command buffer: `WR`. */
export const MAGIC: readonly [number, number] = [0x57, 0x52];

/** The header's length in bytes: the magic, then the version as a `u16`. */
export const HEADER_LEN = 4;

/** The bytes before each command's payload: its opcode and the payload's length, both `u32`. */
export const COMMAND_HEADER_LEN = 8;

/** The header of a buffer in `version`. For version 0 it reads as the `u32` `0x0000_5257`. */
export function header(version: number): Uint8Array {
  return Uint8Array.of(MAGIC[0], MAGIC[1], version & 0xff, (version >>> 8) & 0xff);
}

/** Command opcodes. 0 is never valid, so zeroed memory fails loudly; later versions take 4 up. */
export const opcode = {
  /** Begin a render pass on the screen target, cleared to a colour. */
  BEGIN_SCREEN_PASS: 1,
  /** Draw with a render pipeline in the open screen pass. */
  DRAW: 2,
  /** End the screen pass and show it; ends the frame. */
  PRESENT: 3,
} as const;

export type Clear = readonly [number, number, number, number];

/** One decoded command. */
export type Command =
  /** Begin a render pass on the screen target, cleared to `clear` (r, g, b, a), all finite. */
  | { readonly kind: "BeginScreenPass"; readonly clear: Clear }
  /**
   * Draw `vertexCount` vertices for each of `instanceCount` instances with render pipeline
   * `pipeline` (a manifest id). `uniforms` is the whole content of the pipeline's uniform at
   * group 0, binding 0. A decoded draw's `uniforms` is a view of the submitted bytes: it's valid
   * only until `submit` returns.
   */
  | {
      readonly kind: "Draw";
      readonly pipeline: number;
      readonly vertexCount: number;
      readonly instanceCount: number;
      readonly uniforms: Uint8Array;
    }
  /** End the screen pass and show it. Ends the frame. */
  | { readonly kind: "Present" };

export function commandOpcode(command: Command): number {
  switch (command.kind) {
    case "BeginScreenPass":
      return opcode.BEGIN_SCREEN_PASS;
    case "Draw":
      return opcode.DRAW;
    case "Present":
      return opcode.PRESENT;
  }
}

/** The name of a known opcode, as runtime/command-stream.md writes it. */
export function opcodeName(op: number): string | undefined {
  switch (op) {
    case opcode.BEGIN_SCREEN_PASS:
      return "BEGIN_SCREEN_PASS";
    case opcode.DRAW:
      return "DRAW";
    case opcode.PRESENT:
      return "PRESENT";
    default:
      return undefined;
  }
}

/** The command's name as runtime/command-stream.md writes it. */
export function commandName(command: Command): string {
  return opcodeName(commandOpcode(command)) ?? "unknown";
}

// ---- Encoding ----------------------------------------------------------------------------------

/** Why a command can't be encoded. */
export type EncodeErrorDetail =
  /** A clear colour component is NaN or infinite. */
  | { readonly kind: "NonFiniteClear"; readonly component: number; readonly value: number }
  /** Uniform bytes whose length isn't a multiple of 4. */
  | { readonly kind: "UnalignedUniforms"; readonly len: number }
  /** A payload longer than `u32::MAX` bytes. */
  | { readonly kind: "TooLong"; readonly len: number };

export class EncodeError extends Error {
  override readonly name = "EncodeError";
  constructor(readonly detail: EncodeErrorDetail) {
    super(encodeErrorMessage(detail));
  }
}

/** A float as Rust's `Display` writes the non-finite ones. */
function rustFloat(value: number): string {
  if (Number.isNaN(value)) {
    return "NaN";
  }
  if (value === Infinity) {
    return "inf";
  }
  if (value === -Infinity) {
    return "-inf";
  }
  return String(value);
}

function encodeErrorMessage(detail: EncodeErrorDetail): string {
  switch (detail.kind) {
    case "NonFiniteClear":
      return `clear colour component ${detail.component} is ${rustFloat(detail.value)}, which isn't finite`;
    case "UnalignedUniforms":
      return `${detail.len} bytes of uniforms isn't a multiple of 4`;
    case "TooLong":
      return `a ${detail.len}-byte payload doesn't fit a u32 length`;
  }
}

const U32_MAX = 0xffff_ffff;

/** Throws unless `value` is a `u32`: what Rust's types guarantee and TypeScript's don't. */
function checkU32(value: number, what: string): void {
  if (!Number.isInteger(value) || value < 0 || value > U32_MAX) {
    throw new RangeError(`${what} is ${value}, not a u32`);
  }
}

/** Builds one command buffer. It only produces buffers that `decode` accepts. */
export class Encoder {
  private buffer = new Uint8Array(64);
  private len = 0;

  /** A buffer holding just the header for `VERSION`. */
  constructor() {
    this.append(header(VERSION));
  }

  /** Appends a command, or throws `EncodeError` for one the decoder would reject. */
  push(command: Command): this {
    // Check before writing anything, so a refused command leaves the buffer as it was.
    let payload: number;
    switch (command.kind) {
      case "BeginScreenPass": {
        const component = command.clear.findIndex((c) => !Number.isFinite(c));
        if (component >= 0) {
          const value = command.clear[component] ?? NaN;
          throw new EncodeError({ kind: "NonFiniteClear", component, value });
        }
        payload = 16;
        break;
      }
      case "Draw":
        checkU32(command.pipeline, "pipeline");
        checkU32(command.vertexCount, "vertexCount");
        checkU32(command.instanceCount, "instanceCount");
        if (command.uniforms.length % 4 !== 0) {
          throw new EncodeError({ kind: "UnalignedUniforms", len: command.uniforms.length });
        }
        payload = 12 + command.uniforms.length;
        break;
      case "Present":
        payload = 0;
        break;
    }
    if (payload > U32_MAX) {
      throw new EncodeError({ kind: "TooLong", len: payload });
    }

    this.word(commandOpcode(command));
    this.word(payload);
    switch (command.kind) {
      case "BeginScreenPass":
        for (const c of command.clear) {
          this.f32(c);
        }
        break;
      case "Draw":
        this.word(command.pipeline);
        this.word(command.vertexCount);
        this.word(command.instanceCount);
        this.append(command.uniforms);
        break;
      case "Present":
        break;
    }
    return this;
  }

  /** The bytes so far: a complete buffer. A copy, so later pushes don't change it. */
  bytes(): Uint8Array {
    return this.buffer.slice(0, this.len);
  }

  private reserve(extra: number): void {
    if (this.len + extra <= this.buffer.length) {
      return;
    }
    let size = this.buffer.length * 2;
    while (size < this.len + extra) {
      size *= 2;
    }
    const grown = new Uint8Array(size);
    grown.set(this.buffer.subarray(0, this.len));
    this.buffer = grown;
  }

  private append(bytes: Uint8Array): void {
    this.reserve(bytes.length);
    this.buffer.set(bytes, this.len);
    this.len += bytes.length;
  }

  private word(value: number): void {
    this.reserve(4);
    new DataView(this.buffer.buffer).setUint32(this.len, value, true);
    this.len += 4;
  }

  private f32(value: number): void {
    this.reserve(4);
    new DataView(this.buffer.buffer).setFloat32(this.len, value, true);
    this.len += 4;
  }
}

/** Encodes `commands` as one buffer. */
export function encode(commands: readonly Command[]): Uint8Array {
  const encoder = new Encoder();
  for (const command of commands) {
    encoder.push(command);
  }
  return encoder.bytes();
}

// ---- Decoding ----------------------------------------------------------------------------------

/** Why a command buffer was rejected. `offset` is where the offending command starts. */
export type DecodeErrorDetail =
  /** Fewer than 4 bytes: no room for the header. */
  | { readonly kind: "MissingHeader"; readonly len: number }
  /** The buffer doesn't start with `WR`. */
  | { readonly kind: "BadMagic"; readonly found: readonly [number, number] }
  /** A version this host wasn't built for. */
  | { readonly kind: "UnsupportedVersion"; readonly found: number }
  /** Fewer than 8 bytes where a command starts. */
  | { readonly kind: "TruncatedCommand"; readonly offset: number; readonly remaining: number }
  /** A payload length that isn't a multiple of 4. */
  | {
      readonly kind: "UnalignedLength";
      readonly offset: number;
      readonly opcode: number;
      readonly length: number;
    }
  /** A payload that runs past the end of the buffer. */
  | {
      readonly kind: "PayloadPastEnd";
      readonly offset: number;
      readonly opcode: number;
      readonly length: number;
      readonly available: number;
    }
  /** Opcode 0, or one this version doesn't define. */
  | { readonly kind: "UnknownOpcode"; readonly offset: number; readonly opcode: number }
  /** A payload length the opcode doesn't allow. `expected` says what it allows, in bytes. */
  | {
      readonly kind: "WrongLength";
      readonly offset: number;
      readonly opcode: number;
      readonly length: number;
      readonly expected: string;
    }
  /** A clear colour with a NaN or infinite component. */
  | { readonly kind: "NonFiniteClear"; readonly offset: number; readonly component: number };

export class DecodeError extends Error {
  override readonly name = "DecodeError";
  constructor(readonly detail: DecodeErrorDetail) {
    super(decodeErrorMessage(detail));
  }

  /** Where in the buffer the problem is, in bytes. */
  offset(): number {
    const d = this.detail;
    switch (d.kind) {
      case "MissingHeader":
      case "BadMagic":
        return 0;
      case "UnsupportedVersion":
        return 2;
      default:
        return d.offset;
    }
  }
}

/** An opcode in a message: its name when it has one. */
function describe(op: number): string {
  return opcodeName(op) ?? `opcode ${op}`;
}

function hexByte(byte: number): string {
  return `0x${byte.toString(16).padStart(2, "0")}`;
}

function decodeErrorMessage(d: DecodeErrorDetail): string {
  switch (d.kind) {
    case "MissingHeader":
      return `a command buffer of ${d.len} bytes has no room for the ${HEADER_LEN}-byte header`;
    case "BadMagic":
      return `a command buffer starts with ${hexByte(d.found[0])} ${hexByte(d.found[1])}, not \`WR\``;
    case "UnsupportedVersion":
      return `command buffer version ${d.found} isn't supported; this host reads version ${VERSION}`;
    case "TruncatedCommand":
      return `at byte ${d.offset}: ${d.remaining} bytes left, too few for a command's ${COMMAND_HEADER_LEN}-byte header`;
    case "UnalignedLength":
      return `at byte ${d.offset}: ${describe(d.opcode)} has a ${d.length}-byte payload, not a multiple of 4`;
    case "PayloadPastEnd":
      return `at byte ${d.offset}: ${describe(d.opcode)} has a ${d.length}-byte payload but only ${d.available} bytes follow`;
    case "UnknownOpcode":
      return `at byte ${d.offset}: unknown opcode ${d.opcode}`;
    case "WrongLength":
      return `at byte ${d.offset}: ${describe(d.opcode)} has a ${d.length}-byte payload; it takes ${d.expected} bytes`;
    case "NonFiniteClear":
      return `at byte ${d.offset}: BEGIN_SCREEN_PASS clear colour component ${d.component} isn't finite`;
  }
}

/** A decoded command and the byte offset where it starts in its buffer. */
export interface Located {
  readonly command: Command;
  readonly offset: number;
}

/**
 * Decodes one command buffer, keeping where each command starts. Throws `DecodeError`.
 * Draws' uniforms are views of `bytes`, not copies.
 */
export function decodeLocated(bytes: Uint8Array): Located[] {
  if (bytes.length < HEADER_LEN) {
    throw new DecodeError({ kind: "MissingHeader", len: bytes.length });
  }
  const b0 = bytes[0] ?? 0;
  const b1 = bytes[1] ?? 0;
  if (b0 !== MAGIC[0] || b1 !== MAGIC[1]) {
    throw new DecodeError({ kind: "BadMagic", found: [b0, b1] });
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const version = view.getUint16(2, true);
  if (version !== VERSION) {
    throw new DecodeError({ kind: "UnsupportedVersion", found: version });
  }

  const commands: Located[] = [];
  let offset = HEADER_LEN;
  while (offset < bytes.length) {
    const remaining = bytes.length - offset;
    if (remaining < COMMAND_HEADER_LEN) {
      throw new DecodeError({ kind: "TruncatedCommand", offset, remaining });
    }
    const op = view.getUint32(offset, true);
    const length = view.getUint32(offset + 4, true);
    if (length % 4 !== 0) {
      throw new DecodeError({ kind: "UnalignedLength", offset, opcode: op, length });
    }
    const available = remaining - COMMAND_HEADER_LEN;
    if (length > available) {
      throw new DecodeError({ kind: "PayloadPastEnd", offset, opcode: op, length, available });
    }
    const start = offset + COMMAND_HEADER_LEN;
    const wrongLength = (expected: string) =>
      new DecodeError({ kind: "WrongLength", offset, opcode: op, length, expected });
    let command: Command;
    switch (op) {
      case opcode.BEGIN_SCREEN_PASS: {
        if (length !== 16) {
          throw wrongLength("exactly 16");
        }
        const clear = [0, 4, 8, 12].map((at) => view.getFloat32(start + at, true));
        const component = clear.findIndex((c) => !Number.isFinite(c));
        if (component >= 0) {
          throw new DecodeError({ kind: "NonFiniteClear", offset, component });
        }
        command = { kind: "BeginScreenPass", clear: clear as unknown as Clear };
        break;
      }
      case opcode.DRAW:
        if (length < 12) {
          throw wrongLength("at least 12");
        }
        command = {
          kind: "Draw",
          pipeline: view.getUint32(start, true),
          vertexCount: view.getUint32(start + 4, true),
          instanceCount: view.getUint32(start + 8, true),
          uniforms: bytes.subarray(start + 12, start + length),
        };
        break;
      case opcode.PRESENT:
        if (length !== 0) {
          throw wrongLength("exactly 0");
        }
        command = { kind: "Present" };
        break;
      default:
        throw new DecodeError({ kind: "UnknownOpcode", offset, opcode: op });
    }
    commands.push({ command, offset });
    offset = start + length;
  }
  return commands;
}

/** Decodes one command buffer: the bytes of one `submit` call. Throws `DecodeError`. */
export function decode(bytes: Uint8Array): Command[] {
  return decodeLocated(bytes).map((located) => located.command);
}

// ---- The frame rules ---------------------------------------------------------------------------

/** A break of the frame rules. */
export type SequenceErrorDetail =
  /** A `DRAW` before `BEGIN_SCREEN_PASS`. */
  | { readonly kind: "DrawOutsidePass" }
  /** A second `BEGIN_SCREEN_PASS` in one frame. */
  | { readonly kind: "SecondScreenPass" }
  /** `PRESENT` before `BEGIN_SCREEN_PASS`. */
  | { readonly kind: "PresentWithoutPass" }
  /** A command after `PRESENT`. */
  | { readonly kind: "AfterPresent"; readonly command: string }
  /** `frame` returned without `PRESENT`. */
  | { readonly kind: "NotPresented" }
  /**
   * The frame's `DRAW`s would draw more than `MAX_FRAME_VERTICES` vertices, counting up to and
   * including this one.
   */
  | { readonly kind: "TooManyVertices"; readonly vertices: bigint };

export class SequenceError extends Error {
  override readonly name = "SequenceError";
  constructor(readonly detail: SequenceErrorDetail) {
    super(sequenceErrorMessage(detail));
  }
}

function sequenceErrorMessage(d: SequenceErrorDetail): string {
  switch (d.kind) {
    case "DrawOutsidePass":
      return "a DRAW before BEGIN_SCREEN_PASS: draws go inside the screen pass";
    case "SecondScreenPass":
      return "a second BEGIN_SCREEN_PASS: a frame has one screen pass";
    case "PresentWithoutPass":
      return "a PRESENT before BEGIN_SCREEN_PASS: there's nothing to show";
    case "AfterPresent":
      return `a ${d.command} after PRESENT: PRESENT ends the frame`;
    case "NotPresented":
      return "`frame` returned without PRESENT: every frame ends with one";
    case "TooManyVertices":
      return `this frame's DRAWs draw ${d.vertices} vertices (vertex_count × instance_count, summed), over the ${MAX_FRAME_VERTICES} a frame may draw`;
  }
}

/**
 * The most vertices one frame's `DRAW`s may draw in all: each `DRAW`'s `vertex_count` times its
 * `instance_count`, summed. It stops a garbage count before it reaches the GPU; it doesn't bound
 * GPU time (test mode's frame limit does that). The same as wrela-abi's `MAX_FRAME_VERTICES`.
 */
export const MAX_FRAME_VERTICES = 1n << 20n;

type FrameState = "BeforePass" | "InPass" | "Presented";

/**
 * Checks the frame rules: the commands of one `frame` call, across all its buffers, are one
 * `BEGIN_SCREEN_PASS`, any number of `DRAW`s, then one `PRESENT`. Feed it every command in order,
 * then call `finish` when `frame` returns.
 */
export class FrameCheck {
  private state: FrameState = "BeforePass";
  /** The vertices the frame's draws have drawn so far. */
  private vertices = 0n;

  /**
   * Accepts the next command, or throws `SequenceError` saying which rule it breaks. After an
   * error the check is unchanged; the host stops anyway.
   */
  command(command: Command): void {
    if (this.state === "Presented") {
      throw new SequenceError({ kind: "AfterPresent", command: commandName(command) });
    }
    const inPass = this.state === "InPass";
    switch (command.kind) {
      case "BeginScreenPass":
        if (inPass) {
          throw new SequenceError({ kind: "SecondScreenPass" });
        }
        this.state = "InPass";
        break;
      case "Draw": {
        if (!inPass) {
          throw new SequenceError({ kind: "DrawOutsidePass" });
        }
        const vertices =
          this.vertices + BigInt(command.vertexCount) * BigInt(command.instanceCount);
        if (vertices > MAX_FRAME_VERTICES) {
          throw new SequenceError({ kind: "TooManyVertices", vertices });
        }
        this.vertices = vertices;
        break;
      }
      case "Present":
        if (!inPass) {
          throw new SequenceError({ kind: "PresentWithoutPass" });
        }
        this.state = "Presented";
        break;
    }
  }

  /** Whether the frame has been presented. */
  isPresented(): boolean {
    return this.state === "Presented";
  }

  /** Call when `frame` returns: the frame must have been presented. */
  finish(): void {
    if (!this.isPresented()) {
      throw new SequenceError({ kind: "NotPresented" });
    }
  }
}
