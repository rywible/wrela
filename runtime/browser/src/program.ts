// A running program: its WASM instance, the `wrela.submit` import, and the checks every submitted
// buffer goes through before it reaches the GPU (runtime/command-stream.md, "Program ABI" and
// "Errors").

import { FRAME, IMPORT_MODULE, MEMORY, SUBMIT } from "./abi.ts";
import { StateHash } from "./hash.ts";
import type { Manifest } from "./manifest.ts";
import { checkProgramModule } from "./module.ts";
import { type Clear, DecodeError, decodeLocated, FrameCheck, SequenceError } from "./stream.ts";

/** What a program's commands drive: the GPU side of a host. Commands arrive already checked. */
export interface CommandSink {
  beginScreenPass(clear: Clear): void;
  /** `uniforms` is a view of the program's memory, valid only during the call. */
  draw(pipeline: number, vertexCount: number, instanceCount: number, uniforms: Uint8Array): void;
  present(): void;
}

/** Something went wrong running a program. The message says where: the frame, and the buffer. */
export class HostError extends Error {
  override readonly name = "HostError";
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** A failed `submit`: thrown from the import, so it unwinds the program like a trap. */
class SubmitFailure extends Error {
  constructor(
    readonly buffer: number,
    /** Where the offending command starts, when the problem is one command. */
    readonly offset: number | undefined,
    readonly error: unknown,
  ) {
    super(errorMessage(error));
  }
}

type FrameExport = (time: number, width: number, height: number) => void;

export class Program {
  /** The CPU state hash: every byte submitted so far (AC7). */
  readonly hash = new StateHash();
  private check = new FrameCheck();
  private frameIndex = 0;
  private buffers = 0;
  private inFrame = false;
  /** The first submit that failed in this frame, kept even if the program catches it. */
  private failure: SubmitFailure | undefined;
  /** Once a frame fails the program is never called again. */
  private failed: HostError | undefined;

  private constructor(
    private readonly memory: WebAssembly.Memory,
    private readonly frameExport: FrameExport,
    private readonly manifest: Manifest,
    private readonly sink: CommandSink,
  ) {}

  /**
   * Compiles and instantiates a program, checking it against the ABI first: only `wrela.submit`
   * imported, `memory` and `frame` exported with the right types. Throws `HostError` or
   * `ModuleError`.
   */
  static async instantiate(
    bytes: Uint8Array,
    manifest: Manifest,
    sink: CommandSink,
  ): Promise<Program> {
    let module: WebAssembly.Module;
    try {
      module = await WebAssembly.compile(bytes as Uint8Array<ArrayBuffer>);
    } catch (e) {
      throw new HostError(`the program isn't a valid WASM module: ${errorMessage(e)}`, {
        cause: e,
      });
    }
    checkProgramModule(bytes);

    let program: Program | undefined;
    const submit = (ptr: number, len: number): void => {
      if (program === undefined) {
        throw new HostError("the program called `submit` while starting, outside `frame`");
      }
      program.submit(ptr, len);
    };
    let instance: WebAssembly.Instance;
    try {
      instance = await WebAssembly.instantiate(module, { [IMPORT_MODULE]: { [SUBMIT]: submit } });
    } catch (e) {
      throw new HostError(`the program failed to start: ${errorMessage(e)}`, { cause: e });
    }
    const memory = instance.exports[MEMORY];
    const frame = instance.exports[FRAME];
    // checkProgramModule has checked both; this only narrows the types.
    if (!(memory instanceof WebAssembly.Memory) || typeof frame !== "function") {
      throw new HostError(`the program must export \`${MEMORY}\` and \`${FRAME}\``);
    }
    program = new Program(memory, frame as FrameExport, manifest, sink);
    return program;
  }

  /**
   * Runs frame `index`: calls `frame(time, width, height)`, sending each submitted buffer's
   * commands to the sink, and checks the frame was presented. Throws `HostError`; after that the
   * program is never called again.
   */
  frame(index: number, time: number, width: number, height: number): void {
    if (this.failed !== undefined) {
      throw this.failed;
    }
    this.check = new FrameCheck();
    this.frameIndex = index;
    this.buffers = 0;
    this.failure = undefined;
    this.inFrame = true;
    try {
      this.frameExport(time, width, height);
      if (this.failure !== undefined) {
        throw this.failure;
      }
      this.check.finish();
    } catch (e) {
      this.failed = this.describe(this.failure ?? e);
      throw this.failed;
    } finally {
      this.inFrame = false;
    }
  }

  private submit(ptr: number, len: number): void {
    const buffer = this.buffers++;
    try {
      if (this.failure !== undefined) {
        throw new Error("`submit` was called again after it failed");
      }
      if (!this.inFrame) {
        throw new Error("`submit` was called outside `frame`");
      }
      const start = ptr >>> 0; // u32s cross as i32s with the same bits
      const length = len >>> 0;
      // Read the memory's extent afresh: a growth detaches the old buffer.
      const memory = this.memory.buffer;
      if (start + length > memory.byteLength) {
        throw new Error(
          `the buffer at ${start}..${start + length} is outside the program's ${memory.byteLength}-byte memory`,
        );
      }
      const bytes = new Uint8Array(memory, start, length);
      this.hash.update(bytes);
      const commands = decodeLocated(bytes);
      // Check the whole buffer before any of it reaches the GPU.
      for (const { command, offset } of commands) {
        try {
          this.check.command(command);
          this.manifest.checkCommand(command);
        } catch (e) {
          throw new SubmitFailure(buffer, offset, e);
        }
      }
      for (const { command, offset } of commands) {
        try {
          switch (command.kind) {
            case "BeginScreenPass":
              this.sink.beginScreenPass(command.clear);
              break;
            case "Draw":
              this.sink.draw(
                command.pipeline,
                command.vertexCount,
                command.instanceCount,
                command.uniforms,
              );
              break;
            case "Present":
              this.sink.present();
              break;
          }
        } catch (e) {
          throw new SubmitFailure(buffer, offset, e);
        }
      }
    } catch (e) {
      // A decode error's message already says where in the buffer it is.
      this.failure ??= e instanceof SubmitFailure ? e : new SubmitFailure(buffer, undefined, e);
      throw this.failure;
    }
  }

  private describe(error: unknown): HostError {
    const frame = `frame ${this.frameIndex}`;
    if (error instanceof SubmitFailure) {
      const at = error.offset === undefined ? "" : `, at byte ${error.offset}`;
      return new HostError(`${frame}, buffer ${error.buffer}${at}: ${error.message}`, {
        cause: error.error,
      });
    }
    if (error instanceof WebAssembly.RuntimeError) {
      return new HostError(`${frame}: the program trapped: ${error.message}`, { cause: error });
    }
    if (error instanceof SequenceError || error instanceof DecodeError) {
      return new HostError(`${frame}: ${error.message}`, { cause: error });
    }
    return new HostError(`${frame}: ${errorMessage(error)}`, { cause: error });
  }
}
