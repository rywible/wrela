// Loading a program and running its frames: test mode's fixed 60 frames with a capture, and play
// mode's frame loop. Everything outside comes in as arguments (fetch, the device, the canvas
// context, the scheduler), so tests drive it with fakes.

import { testMode } from "./abi.ts";
import {
  type Capture,
  createPipelines,
  SCREEN_FORMAT,
  ScreenRenderer,
  TextureUsage,
} from "./gpu.ts";
import { Manifest } from "./manifest.ts";
import { errorMessage, HostError, Program } from "./program.ts";

/** A program's build output, fetched: its manifest and the files it names. */
export interface ProgramFiles {
  readonly manifest: Manifest;
  readonly wasm: Uint8Array;
  /** WGSL source for each module the manifest names, keyed by its manifest path. */
  readonly modules: ReadonlyMap<string, string>;
}

export type Fetch = (url: URL) => Promise<Response>;

async function fetchOk(url: URL, fetchFn: Fetch): Promise<Response> {
  let response: Response;
  try {
    response = await fetchFn(url);
  } catch (e) {
    throw new HostError(`couldn't fetch ${url.href}: ${errorMessage(e)}`, { cause: e });
  }
  if (!response.ok) {
    throw new HostError(`couldn't fetch ${url.href}: ${response.status} ${response.statusText}`);
  }
  return response;
}

/** Fetches the manifest, then the WASM and WGSL modules it names (paths relative to it). */
export async function loadProgram(manifestUrl: URL, fetchFn: Fetch): Promise<ProgramFiles> {
  const manifest = Manifest.fromJson(await (await fetchOk(manifestUrl, fetchFn)).text());
  const paths = [...new Set(manifest.pipelines.map((p) => p.module))];
  const [wasm, ...sources] = await Promise.all([
    fetchOk(new URL(manifest.wasm, manifestUrl), fetchFn).then(
      async (r) => new Uint8Array(await r.arrayBuffer()),
    ),
    ...paths.map(async (path) => (await fetchOk(new URL(path, manifestUrl), fetchFn)).text()),
  ]);
  const modules = new Map(paths.map((path, i) => [path, sources[i] ?? ""]));
  return { manifest, wasm, modules };
}

/** The adapter's identity, for reports. */
export interface AdapterInfo {
  readonly vendor: string;
  readonly architecture: string;
  readonly device: string;
  readonly description: string;
}

/**
 * Requests a device with WebGPU's default limits, as the native host does: what runs in one host
 * must run in the other, and the manifest is validated against those limits.
 */
export async function requestDevice(
  gpu: GPU,
): Promise<{ device: GPUDevice; adapter: AdapterInfo }> {
  const adapter = await gpu.requestAdapter({ powerPreference: "high-performance" });
  if (adapter === null) {
    throw new HostError("no WebGPU adapter is available");
  }
  const device = await adapter.requestDevice({ label: "wrela" });
  const { vendor, architecture, device: name, description } = adapter.info;
  return { device, adapter: { vendor, architecture, device: name, description } };
}

/** The parts of a canvas context `startProgram` uses. */
export type CanvasContext = Pick<GPUCanvasContext, "configure" | "getCurrentTexture">;

/**
 * Configures the screen, creates every pipeline, then instantiates the program, in the contract's
 * order: all pipelines exist before the first frame.
 */
export async function startProgram(
  device: GPUDevice,
  context: CanvasContext,
  files: ProgramFiles,
  options: { capture: boolean },
): Promise<{ program: Program; renderer: ScreenRenderer }> {
  context.configure({
    device,
    format: SCREEN_FORMAT,
    alphaMode: "opaque",
    usage: TextureUsage.RENDER_ATTACHMENT | (options.capture ? TextureUsage.COPY_SRC : 0),
  });
  const pipelines = await createPipelines(device, files.manifest, files.modules);
  const renderer = new ScreenRenderer(device, context, pipelines);
  const program = await Program.instantiate(files.wasm, files.manifest, renderer);
  return { program, renderer };
}

const ERROR_SCOPES: readonly GPUErrorFilter[] = ["validation", "out-of-memory", "internal"];

/** Runs `body` inside error scopes for every filter; throws `HostError` if WebGPU reported any. */
async function withErrorScopes(device: GPUDevice, frame: number, body: () => void): Promise<void> {
  for (const filter of ERROR_SCOPES) {
    device.pushErrorScope(filter);
  }
  let thrown: { error: unknown } | undefined;
  try {
    body();
  } catch (error) {
    thrown = { error };
  }
  // Pop every scope even after a throw, so none leaks into later frames.
  const errors: string[] = [];
  for (let i = 0; i < ERROR_SCOPES.length; i++) {
    const error = await device.popErrorScope();
    if (error !== null) {
      errors.push(error.message);
    }
  }
  if (thrown !== undefined) {
    throw thrown.error;
  }
  if (errors.length > 0) {
    throw new HostError(`frame ${frame}: WebGPU reported: ${errors.join("; ")}`);
  }
}

export interface TestRun {
  /** The CPU state hash over every submitted byte. */
  readonly hash: string;
  readonly frames: number;
  readonly capture: Capture;
  /**
   * The longest wall-clock time from a frame's start to its GPU work finishing, in milliseconds:
   * an upper bound on that frame's GPU time, to watch against the ~100 ms per submission limit.
   */
  readonly slowestFrameMs: number;
}

/**
 * Test mode: frames 0..59 at 1920×1080 with `time = fround(i / 60)`, then the last frame captured.
 * Each frame's GPU work finishes before the next starts, so the GPU never holds more than one
 * frame (and one submission) at a time, and a frame over `testMode.FRAME_LIMIT_MS` ends the run.
 * Throws `HostError`, or `signal`'s reason once it aborts.
 */
export async function runTestMode(
  device: GPUDevice,
  program: Program,
  renderer: ScreenRenderer,
  signal: AbortSignal,
  onFrame: (index: number) => void = () => {},
): Promise<TestRun> {
  const { WIDTH, HEIGHT, FRAMES } = testMode;
  let slowestFrameMs = 0;
  for (let i = 0; i < FRAMES; i++) {
    signal.throwIfAborted();
    onFrame(i);
    if (i === FRAMES - 1) {
      renderer.requestCapture();
    }
    const start = performance.now();
    await withErrorScopes(device, i, () => program.frame(i, testMode.time(i), WIDTH, HEIGHT));
    await device.queue.onSubmittedWorkDone();
    const took = performance.now() - start;
    if (took > testMode.FRAME_LIMIT_MS) {
      throw new HostError(
        `frame ${i}: its GPU work took ${Math.round(took)} ms, over the ${testMode.FRAME_LIMIT_MS} ms a test-mode frame may take; the run stopped there so the GPU isn't held`,
      );
    }
    slowestFrameMs = Math.max(slowestFrameMs, took);
  }
  signal.throwIfAborted();
  const capture = await renderer.readCapture();
  signal.throwIfAborted();
  return { hash: program.hash.hex(), frames: FRAMES, capture, slowestFrameMs };
}

/** The screen's size in physical pixels, read each frame. */
export interface ScreenSize {
  readonly width: number;
  readonly height: number;
}

/** Calls back on the next animation frame with a timestamp in milliseconds. */
export type Schedule = (callback: (now: number) => void) => void;

/**
 * Play mode: one frame per animation frame, with the time in seconds since the first. A frame is
 * skipped while the previous one is still on the GPU, so work never piles up there. Stops at the
 * first error (reported through `onError`) or when `signal` aborts.
 */
export function play(
  device: GPUDevice,
  program: Program,
  screen: () => ScreenSize,
  schedule: Schedule,
  signal: AbortSignal,
  onError: (error: unknown) => void,
): void {
  let first: number | undefined;
  let index = 0;
  let busy = false;
  let stopped = false;
  const fail = (error: unknown) => {
    if (stopped || signal.aborted) {
      return;
    }
    stopped = true;
    onError(error);
  };
  const tick = (now: number): void => {
    if (stopped || signal.aborted) {
      return;
    }
    schedule(tick);
    if (busy) {
      return;
    }
    first ??= now;
    const time = Math.fround((now - first) / 1000);
    const { width, height } = screen();
    const frame = index++;
    busy = true;
    withErrorScopes(device, frame, () => program.frame(frame, time, width, height))
      .then(() => device.queue.onSubmittedWorkDone())
      .then(() => {
        busy = false;
      }, fail);
  };
  schedule(tick);
}
