// The imports of another thread's instance of the program: a helper's, the ticker's, the voice's.
// It needs only the ABI, so the audio worklet's small bundle can have it too.

import { HOST_FUNCTIONS, IMPORT_CLOCK, IMPORT_MEMORY, IMPORT_MODULE } from "./abi.gen.ts";

/** `wrela.clock(ptr)`: the seconds on this host's clock, an `f64`, written at `ptr`. A page's
 * workers share a time origin's scale (`performance.timeOrigin` is each context's start on one
 * clock), so every thread reads the same clock. */
export function clockImport(memory: WebAssembly.Memory): (ptr: number) => void {
  return (ptr: number) => {
    const seconds = (performance.timeOrigin + performance.now()) / 1000;
    new DataView(memory.buffer).setFloat64(ptr >>> 0, seconds, true);
  };
}

/** The shared `memory`, and host functions that refuse to run: they throw a `Refusal` that says
 * `who` can't call the host. Another thread's code records no GPU work and makes no requests (its
 * effects are checked). */
export function refusingImports(
  memory: WebAssembly.Memory,
  who: string,
  Refusal: new (message: string) => Error,
): WebAssembly.Imports {
  const refuse = () => {
    throw new Refusal(`${who} can't call the host`);
  };
  // The clock answers on every thread (a voice may read it).
  return {
    [IMPORT_MODULE]: {
      [IMPORT_MEMORY]: memory,
      ...Object.fromEntries(HOST_FUNCTIONS.map(([name]) => [name, refuse])),
      [IMPORT_CLOCK]: clockImport(memory),
    },
  };
}
