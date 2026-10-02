// Test programs: small WASM modules that submit given buffers, and a sink that records commands.

import type { CommandSink } from "../src/program.ts";
import type { Clear, Command } from "../src/stream.ts";
import { wat } from "./wat.ts";

export const hex = (bytes: Uint8Array) =>
  [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");

/** `bytes` as a WAT string literal. */
const literal = (bytes: Uint8Array) =>
  `"${[...bytes].map((b) => `\\${b.toString(16).padStart(2, "0")}`).join("")}"`;

/**
 * A program whose every frame submits `buffers` in order, each from its own place in memory.
 * `before` is WAT run at the start of each frame.
 */
export function submitting(
  buffers: readonly Uint8Array[],
  options: { before?: string; extra?: string } = {},
): Promise<Uint8Array> {
  let offset = 0;
  const data: string[] = [];
  const calls: string[] = [];
  for (const buffer of buffers) {
    data.push(`(data (i32.const ${offset}) ${literal(buffer)})`);
    calls.push(`(call $submit (i32.const ${offset}) (i32.const ${buffer.length}))`);
    offset += Math.ceil(buffer.length / 8) * 8 + 8;
  }
  return wat(`(module
    (import "wrela" "submit" (func $submit (param i32 i32)))
    (memory (export "memory") 1)
    ${data.join("\n")}
    ${options.extra ?? ""}
    (func (export "frame") (param $time f32) (param $width i32) (param $height i32)
      ${options.before ?? ""}
      ${calls.join("\n")}))`);
}

/** Records the commands a program sends, as readable lines. */
export class RecordingSink implements CommandSink {
  readonly log: string[] = [];

  beginScreenPass(clear: Clear): void {
    this.log.push(`begin ${clear.join(" ")}`);
  }

  draw(pipeline: number, vertexCount: number, instanceCount: number, uniforms: Uint8Array): void {
    this.log.push(`draw ${pipeline} ${vertexCount} ${instanceCount} ${hex(uniforms)}`);
  }

  present(): void {
    this.log.push("present");
  }
}

export const begin: Command = { kind: "BeginScreenPass", clear: [0, 0, 0, 1] };
export const present: Command = { kind: "Present" };

/** A draw of first light's pipeline 0 with a 16-byte uniform. */
export function draw(pipeline = 0, uniforms = new Uint8Array(16)): Command {
  return { kind: "Draw", pipeline, vertexCount: 3, instanceCount: 1, uniforms };
}
