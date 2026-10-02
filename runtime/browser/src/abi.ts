// The program ABI's names and test mode: the TypeScript mirror of the constants in wrela-abi
// (runtime/abi/src/lib.rs). runtime/command-stream.md is normative for both.

/** The contract's version. It's in every command buffer's header and in the manifest. */
export const VERSION = 0;

/** The module a program imports host functions from. */
export const IMPORT_MODULE = "wrela";

/** `submit(ptr: u32, len: u32)`: hands the host one complete command buffer. */
export const SUBMIT = "submit";

/** The program's exported linear memory. */
export const MEMORY = "memory";

/** `frame(time: f32, width: u32, height: u32)`: called once per frame. */
export const FRAME = "frame";

/** The fixed inputs of test mode, so every host renders the same frames. */
export const testMode = {
  /** The screen's width in pixels. */
  WIDTH: 1920,
  /** The screen's height in pixels. */
  HEIGHT: 1080,
  /** Frames run before the capture. */
  FRAMES: 60,
  /**
   * A frame whose GPU work takes longer than this, in milliseconds, ends the run with an error:
   * four times the ~100 ms a submission may take, so a slow program can't hold the GPU for all its
   * frames.
   */
  FRAME_LIMIT_MS: 400,
  /**
   * The time passed to frame `index` (from 0): `index / 60` seconds, computed in f64 and rounded
   * once to the nearest f32, as `(index as f64 / 60.0) as f32` does in Rust.
   */
  time(index: number): number {
    return Math.fround(index / 60);
  },
} as const;
