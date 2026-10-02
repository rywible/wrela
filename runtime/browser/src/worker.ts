// The render worker: owns the GPU device and the canvas, runs the program, and reports back to
// the page. In test mode (the page's URL ends in `#run`) it runs the fixed 60 frames, then saves
// `results/frame.png`, `results/run.json` and `results/DONE` beside the page.

import { testMode } from "./abi.ts";
import type { FromWorker, InputEvent, ToWorker } from "./messages.ts";
import { encodePng } from "./png.ts";
import { errorMessage, HostError } from "./program.ts";
import {
  type AdapterInfo,
  loadProgram,
  play,
  requestDevice,
  runTestMode,
  type ScreenSize,
  startProgram,
} from "./runner.ts";

/** The parts of the dedicated worker's global scope used here (the DOM lib types the window). */
interface WorkerScope {
  postMessage(message: FromWorker): void;
  addEventListener(type: "message", listener: (event: MessageEvent<ToWorker>) => void): void;
  addEventListener(
    type: "unhandledrejection",
    listener: (event: PromiseRejectionEvent) => void,
  ): void;
  requestAnimationFrame?: (callback: (now: number) => void) => number;
  navigator: { gpu?: GPU; userAgent: string };
}

const scope = globalThis as unknown as WorkerScope;
const post = (message: FromWorker) => scope.postMessage(message);
const status = (text: string) => post({ type: "status", text });
const report = (error: unknown) => post({ type: "error", text: errorMessage(error) });

/**
 * The latest input. Version 0 of the program ABI has no way to read it; it's kept so the worker
 * knows the input state when a later version adds one.
 */
const input = { pointer: { x: 0, y: 0, buttons: 0 }, keys: new Set<string>(), focused: true };

function applyInput(event: InputEvent): void {
  switch (event.kind) {
    case "pointer":
      input.pointer = { x: event.x, y: event.y, buttons: event.buttons };
      break;
    case "key":
      if (event.action === "down") {
        input.keys.add(event.code);
      } else {
        input.keys.delete(event.code);
      }
      break;
    case "focus":
      input.focused = event.focused;
      if (!event.focused) {
        input.keys.clear();
      }
      break;
  }
}

let canvas: OffscreenCanvas | undefined;
let maxDimension = 8192;
let running = false;
/** Test mode's screen is always 1920×1080, whatever the page's size. */
let fixedSize = false;

/** Sizes the canvas, within what a texture can be. */
function resize(width: number, height: number): void {
  if (canvas === undefined) {
    return;
  }
  const clamp = (n: number) => Math.min(maxDimension, Math.max(1, Math.round(n) || 1));
  canvas.width = clamp(width);
  canvas.height = clamp(height);
}

async function save(base: string, path: string, body: Uint8Array | string): Promise<void> {
  const url = new URL(path, base);
  const response = await fetch(url, { method: "PUT", body: body as BodyInit });
  if (!response.ok) {
    throw new HostError(`couldn't save ${url.href}: ${response.status} ${response.statusText}`);
  }
}

async function start(message: Extract<ToWorker, { type: "start" }>): Promise<void> {
  const { base, test } = message;
  canvas = message.canvas;
  fixedSize = test;
  // The first error ends the run: aborting reports it, once.
  const stop = new AbortController();
  stop.signal.addEventListener("abort", () => report(stop.signal.reason));
  let adapter: AdapterInfo | undefined;
  try {
    status("loading");
    const files = await loadProgram(new URL("manifest.json", base), (url) => fetch(url));
    const gpu = scope.navigator.gpu;
    if (gpu === undefined) {
      throw new HostError("WebGPU isn't available in this browser");
    }
    const requested = await requestDevice(gpu);
    const device = requested.device;
    adapter = requested.adapter;
    maxDimension = device.limits.maxTextureDimension2D;
    // Errors outside a frame's error scopes, and a lost device, end the run.
    device.addEventListener("uncapturederror", (event) => {
      stop.abort(new HostError(`WebGPU reported: ${event.error.message}`));
    });
    void device.lost.then((info) => {
      stop.abort(new HostError(`the GPU device was lost (${info.reason}): ${info.message}`));
    });

    if (test) {
      resize(testMode.WIDTH, testMode.HEIGHT);
    } else {
      resize(message.width, message.height);
    }
    const context = canvas.getContext("webgpu");
    if (context === null) {
      throw new HostError("the canvas has no WebGPU context");
    }
    const { program, renderer } = await startProgram(device, context, files, { capture: test });

    if (!test) {
      status("");
      const screen = (): ScreenSize => ({ width: canvas?.width ?? 1, height: canvas?.height ?? 1 });
      const raf = scope.requestAnimationFrame?.bind(scope);
      const schedule =
        raf ??
        ((callback: (now: number) => void) => setTimeout(() => callback(performance.now()), 16));
      play(
        device,
        program,
        screen,
        (callback) => void schedule(callback),
        stop.signal,
        (e) => stop.abort(e),
      );
      return;
    }

    const run = await runTestMode(device, program, renderer, stop.signal, (i) => {
      status(`frame ${i + 1} of ${testMode.FRAMES}`);
    });
    const { width, height, format, rgba } = run.capture;
    const png = await encodePng(width, height, rgba);
    const summary = {
      hash: run.hash,
      frames: run.frames,
      width,
      height,
      format,
      userAgent: scope.navigator.userAgent,
      adapter,
      slowestFrameMs: Math.round(run.slowestFrameMs * 10) / 10,
      error: null,
    };
    await save(base, "results/frame.png", png);
    await save(base, "results/run.json", `${JSON.stringify(summary, null, 2)}\n`);
    await save(base, "results/DONE", "ok");
    post({ type: "done", ok: true, text: `ran ${run.frames} frames; state hash ${run.hash}` });
  } catch (e) {
    if (!stop.signal.aborted) {
      stop.abort(e);
    }
    const error: unknown = stop.signal.reason;
    if (!test) {
      return;
    }
    // Tell the headless harness the run is over, and why it failed.
    try {
      const summary = {
        hash: null,
        userAgent: scope.navigator.userAgent,
        adapter: adapter ?? null,
        error: errorMessage(error),
      };
      await save(base, "results/run.json", `${JSON.stringify(summary, null, 2)}\n`);
      await save(base, "results/DONE", "failed");
    } catch (saveError) {
      report(saveError);
    }
    post({ type: "done", ok: false, text: errorMessage(error) });
  }
}

scope.addEventListener("message", (event) => {
  const message = event.data;
  switch (message.type) {
    case "start":
      if (running) {
        report(new HostError("the render worker was started twice"));
        return;
      }
      running = true;
      void start(message);
      break;
    case "resize":
      if (!fixedSize) {
        resize(message.width, message.height);
      }
      break;
    case "input":
      applyInput(message.input);
      break;
  }
});

scope.addEventListener("unhandledrejection", (event) => report(event.reason));
