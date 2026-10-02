// WAT to WASM for tests and fixtures, with wabt (a pinned dev dependency, never shipped).

import { readFile } from "node:fs/promises";
import wabtInit from "wabt";

let wabt: Awaited<ReturnType<typeof wabtInit>> | undefined;

/** WebAssembly features to enable when assembling, beyond wabt's defaults. */
export type Features = Parameters<Awaited<ReturnType<typeof wabtInit>>["parseWat"]>[2];

/** Assembles WAT text. Deterministic: the same text gives the same bytes. */
export async function wat(
  text: string,
  name = "test.wat",
  features?: Features,
): Promise<Uint8Array> {
  wabt ??= await wabtInit();
  const module = wabt.parseWat(name, text, features);
  try {
    module.resolveNames();
    // wabt.js validates with its default features whatever the parse enabled, so a module that
    // asks for more (to test that a host refuses it) is left to the engine to validate.
    if (features === undefined) {
      module.validate();
    }
    return new Uint8Array(module.toBinary({ log: false, write_debug_names: false }).buffer);
  } finally {
    module.destroy();
  }
}

export async function watToWasm(path: string): Promise<Uint8Array> {
  return wat(await readFile(path, "utf8"), path);
}
