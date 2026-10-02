// The WebGPU side, against the fake device: pipelines from the manifest, the screen renderer, and
// whole test-mode and play-mode runs of the first-light fixture.

import { describe, expect, spyOn, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { inflateSync } from "node:zlib";
import { testMode } from "../src/abi.ts";
import {
  BufferUsage,
  createPipelines,
  ScreenRenderer,
  ShaderStage,
  TextureUsage,
} from "../src/gpu.ts";
import { type ComputePipeline, Manifest, type RenderPipeline } from "../src/manifest.ts";
import { encodePng } from "../src/png.ts";
import { HostError, Program } from "../src/program.ts";
import { play, runTestMode, startProgram } from "../src/runner.ts";
import { fakes } from "./fake-gpu.ts";
import { contractManifest, FIXTURE, rejected } from "./helpers.ts";
import { hex, RecordingSink } from "./programs.ts";

const manifestText = await readFile(join(FIXTURE, "manifest.json"), "utf8");
const wgsl = await readFile(join(FIXTURE, "program.0.wgsl"), "utf8");
const wasm = new Uint8Array(await readFile(join(FIXTURE, "program.wasm")));
const manifest = () => Manifest.fromJson(manifestText);
const sources = (code = wgsl) => new Map([["program.0.wgsl", code]]);
const files = () => ({ manifest: manifest(), wasm, modules: sources() });

/** The 16-byte uniform first light submits for frame `i`, as hex. */
function sceneHex(i: number): string {
  const bytes = new Uint8Array(16);
  const view = new DataView(bytes.buffer);
  view.setFloat32(0, testMode.WIDTH, true);
  view.setFloat32(4, testMode.HEIGHT, true);
  view.setFloat32(8, testMode.time(i), true);
  return hex(bytes);
}

describe("pipelines come from the manifest", () => {
  test("first light's render pipeline", async () => {
    const { device, gpuDevice } = fakes();
    const built = await createPipelines(gpuDevice, manifest(), sources());
    expect(device.openScopes).toBe(0);
    expect(device.modules.map((m) => m.label)).toEqual(["program.0.wgsl"]);
    expect(device.bindGroupLayouts.map((l) => [...l.entries])).toEqual([
      [
        {
          binding: 0,
          visibility: ShaderStage.FRAGMENT,
          buffer: { type: "uniform", minBindingSize: 16 },
        },
      ],
    ]);
    const descriptor = device.pipelines[0]?.descriptor as GPURenderPipelineDescriptor;
    expect(descriptor.vertex.entryPoint).toBe("cover");
    expect(descriptor.fragment?.entryPoint).toBe("shade");
    expect([...(descriptor.fragment?.targets ?? [])]).toEqual([{ format: "rgba8unorm" }]);
    expect(descriptor.primitive).toEqual({ topology: "triangle-list", cullMode: "none" });
    expect(descriptor.depthStencil).toBeUndefined();
    const pipeline = built.get(0);
    expect(pipeline?.kind === "render" && pipeline.uniform?.size).toBe(16);
  });

  test("bindings of every kind, in every group, for a compute pipeline", async () => {
    const m = manifest();
    const kernel: ComputePipeline = {
      kind: "compute",
      id: 5,
      module: "program.0.wgsl",
      compute: "cover",
      workgroup_size: [64, 1, 1],
      bindings: [
        { group: 0, binding: 0, kind: "uniform", visibility: ["compute"], size: 32 },
        { group: 1, binding: 2, kind: "storage_read", visibility: ["compute"], size: 64 },
        {
          group: 2,
          binding: 0,
          kind: "storage_read_write",
          visibility: ["compute"],
          size: 16,
          stride: 8,
        },
      ],
    };
    m.pipelines.push(kernel);
    m.validate();
    const { device, gpuDevice } = fakes();
    const built = await createPipelines(gpuDevice, m, sources());
    expect(built.get(5)?.kind).toBe("compute");
    expect(device.modules.length).toBe(1); // one module, shared
    expect(device.bindGroupLayouts.slice(1).map((l) => [...l.entries])).toEqual([
      [
        {
          binding: 0,
          visibility: ShaderStage.COMPUTE,
          buffer: { type: "uniform", minBindingSize: 32 },
        },
      ],
      [
        {
          binding: 2,
          visibility: ShaderStage.COMPUTE,
          buffer: { type: "read-only-storage", minBindingSize: 64 },
        },
      ],
      // A runtime-sized array must hold at least one element.
      [
        {
          binding: 0,
          visibility: ShaderStage.COMPUTE,
          buffer: { type: "storage", minBindingSize: 24 },
        },
      ],
    ]);
  });

  test("a shader that doesn't compile says where", async () => {
    const { device, gpuDevice } = fakes();
    const broken = wgsl.replace("let t = scene.time;", "let t = ERROR;");
    const error = await rejected(() => createPipelines(gpuDevice, manifest(), sources(broken)));
    expect(error).toBeInstanceOf(HostError);
    const line = broken.split("\n").findIndex((l) => l.includes("ERROR")) + 1;
    expect((error as Error).message).toBe(
      `the WGSL module \`program.0.wgsl\` doesn't compile: program.0.wgsl:${line}:13: unexpected ERROR`,
    );
    expect(device.openScopes).toBe(0);
  });

  test("a missing entry point or source", async () => {
    const { device, gpuDevice } = fakes();
    const m = manifest();
    const p = m.pipelines[0];
    if (p?.kind === "render") {
      p.fragment = "paint";
    }
    expect(
      ((await rejected(() => createPipelines(gpuDevice, m, sources()))) as Error).message,
    ).toBe(
      "pipeline 0 (`program.0.wgsl`) can't be created: entry point `paint` isn't in the module",
    );
    expect(
      ((await rejected(() => createPipelines(gpuDevice, manifest(), new Map()))) as Error).message,
    ).toBe("no source for the WGSL module `program.0.wgsl`");
    expect(device.openScopes).toBe(0);
  });

  test("a validation error while creating them", async () => {
    const { device, gpuDevice } = fakes();
    const create = device.createBindGroupLayout.bind(device);
    device.createBindGroupLayout = (descriptor) => {
      device.error("bad layout");
      return create(descriptor);
    };
    expect(
      ((await rejected(() => createPipelines(gpuDevice, manifest(), sources()))) as Error).message,
    ).toBe("creating the pipelines failed: bad layout");
  });
});

describe("the screen renderer", () => {
  async function renderer(width = 4, height = 2, format: GPUTextureFormat = "rgba8unorm") {
    const f = fakes(width, height);
    const m = manifest();
    // A second pipeline with no uniform.
    m.pipelines.push({ ...structuredClone(m.pipelines[0] as RenderPipeline), id: 1, bindings: [] });
    const pipelines = await createPipelines(f.gpuDevice, m, sources());
    f.context.configure({
      device: f.gpuDevice,
      format,
      usage: TextureUsage.RENDER_ATTACHMENT | TextureUsage.COPY_SRC,
    });
    return { ...f, screen: new ScreenRenderer(f.gpuDevice, f.gpuContext, pipelines) };
  }
  const bytes = (...values: number[]) => Uint8Array.from(values);

  test("each draw sees its own uniform bytes, and slots are reused frame to frame", async () => {
    const { device, screen } = await renderer();
    const a = bytes(...Array(16).fill(1));
    const b = bytes(...Array(16).fill(2));
    for (let frame = 0; frame < 3; frame++) {
      screen.beginScreenPass([0, 0, 0, 1]);
      screen.draw(0, 3, 1, a);
      screen.draw(1, 6, 2, new Uint8Array());
      screen.draw(0, 3, 1, b);
      screen.present();
    }
    const [pass] = device.submitted[2] ?? [];
    expect(pass?.draws).toEqual([
      { pipeline: "pipeline 0", vertexCount: 3, instanceCount: 1, uniforms: hex(a) },
      { pipeline: "pipeline 1", vertexCount: 6, instanceCount: 2, uniforms: undefined },
      { pipeline: "pipeline 0", vertexCount: 3, instanceCount: 1, uniforms: hex(b) },
    ]);
    expect(device.buffers.map((x) => [x.label, x.size, x.usage])).toEqual([
      ["pipeline 0, uniform 0", 16, BufferUsage.UNIFORM | BufferUsage.COPY_DST],
      ["pipeline 0, uniform 1", 16, BufferUsage.UNIFORM | BufferUsage.COPY_DST],
    ]);
    expect(device.uncaptured).toEqual([]);
  });

  test("the uniform bytes are copied when drawn, so the program may reuse its memory", async () => {
    const { device, screen } = await renderer();
    const memory = new Uint8Array(16).fill(7);
    screen.beginScreenPass([0, 0, 0, 1]);
    screen.draw(0, 3, 1, memory);
    memory.fill(9);
    screen.present();
    expect(device.submitted[0]?.[0]?.draws[0]?.uniforms).toBe("07".repeat(16));
  });

  test("the pass clears to the command's colour", async () => {
    const { device, screen } = await renderer();
    screen.beginScreenPass([0.25, 0.5, 0.75, 1]);
    screen.present();
    expect(device.submitted[0]?.[0]?.clear).toEqual([0.25, 0.5, 0.75, 1]);
  });

  test("a capture is tight RGBA rows, whatever the row padding or texture format", async () => {
    for (const format of ["rgba8unorm", "bgra8unorm"] as const) {
      const { device, screen } = await renderer(3, 2, format);
      screen.requestCapture();
      screen.beginScreenPass([1, 0.4, 0, 1]);
      screen.present();
      const capture = await screen.readCapture();
      expect([capture.width, capture.height, capture.format]).toEqual([3, 2, format]);
      expect([...capture.rgba]).toEqual(Array(6).fill([255, 102, 0, 255]).flat());
      expect(device.buffers.at(-1)?.destroyed).toBe(true);
      expect(device.uncaptured).toEqual([]);
    }
  });

  test("reading a capture that never happened is an error", async () => {
    const { screen } = await renderer();
    expect(((await rejected(() => screen.readCapture())) as Error).message).toBe(
      "no frame was captured: the capture's PRESENT never came",
    );
  });
});

describe("test mode", () => {
  test("first light: 60 frames, the reference hash, and the last frame captured", async () => {
    const { device, context, gpuDevice, gpuContext } = fakes();
    const { program, renderer } = await startProgram(gpuDevice, gpuContext, files(), {
      capture: true,
    });
    expect(context.configuration).toMatchObject({
      format: "rgba8unorm",
      alphaMode: "opaque",
      usage: TextureUsage.RENDER_ATTACHMENT | TextureUsage.COPY_SRC,
    });
    const seen: number[] = [];
    const run = await runTestMode(gpuDevice, program, renderer, new AbortController().signal, (i) =>
      seen.push(i),
    );
    expect(run.hash).toBe("ee6a915168bafdc0");
    expect(run.frames).toBe(60);
    expect(seen).toEqual([...Array(60).keys()]);
    expect(device.submitted.length).toBe(60);
    device.submitted.forEach((passes, i) => {
      expect(passes.map((p) => [p.clear, p.draws])).toEqual([
        [
          [0, 0, 0, 1],
          [{ pipeline: "pipeline 0", vertexCount: 3, instanceCount: 1, uniforms: sceneHex(i) }],
        ],
      ]);
    });
    expect([run.capture.width, run.capture.height, run.capture.format]).toEqual([
      1920,
      1080,
      "rgba8unorm",
    ]);
    expect(run.capture.rgba.every((v, i) => v === (i % 4 === 3 ? 255 : 0))).toBe(true);
    expect(device.openScopes).toBe(0);
    expect(device.uncaptured).toEqual([]);

    // And the capture makes a PNG of the same pixels.
    const png = await encodePng(run.capture.width, run.capture.height, run.capture.rgba);
    const idat = png.subarray(8 + 25 + 8, png.length - 12 - 4);
    expect(inflateSync(idat).length).toBe((1920 * 4 + 1) * 1080);
  });

  test("a WebGPU error in a frame fails the run at that frame", async () => {
    const { device, gpuDevice, gpuContext } = fakes();
    const { program, renderer } = await startProgram(gpuDevice, gpuContext, files(), {
      capture: true,
    });
    device.onSubmit = (i) => {
      if (i === 5) {
        device.error("boom");
      }
    };
    const error = await rejected(() =>
      runTestMode(gpuDevice, program, renderer, new AbortController().signal),
    );
    expect((error as Error).message).toBe("frame 5: WebGPU reported: boom");
    expect(device.submitted.length).toBe(6);
    expect(device.openScopes).toBe(0);
  });

  test("a frame over the test-mode limit ends the run there", async () => {
    const { device, gpuDevice, gpuContext } = fakes();
    const { program, renderer } = await startProgram(gpuDevice, gpuContext, files(), {
      capture: true,
    });
    // A clock the frames' GPU work moves: 1 ms a frame, then 401 ms for frame 3.
    let clock = 0;
    const now = spyOn(performance, "now").mockImplementation(() => clock);
    device.workDone = () => {
      clock += device.submitted.length === 4 ? testMode.FRAME_LIMIT_MS + 1 : 1;
      return Promise.resolve();
    };
    try {
      const error = await rejected(() =>
        runTestMode(gpuDevice, program, renderer, new AbortController().signal),
      );
      expect(error).toBeInstanceOf(HostError);
      expect((error as Error).message).toBe(
        "frame 3: its GPU work took 401 ms, over the 400 ms a test-mode frame may take; the run stopped there so the GPU isn't held",
      );
      expect(device.submitted.length).toBe(4);
    } finally {
      now.mockRestore();
    }
  });

  test("a program error fails the run, with every error scope popped", async () => {
    const { device, gpuDevice, gpuContext } = fakes();
    const m = manifest();
    const p = m.pipelines[0];
    if (p?.kind === "render") {
      p.id = 3; // the program draws pipeline 0, which this manifest lacks
    }
    const { program, renderer } = await startProgram(
      gpuDevice,
      gpuContext,
      { manifest: m, wasm, modules: sources() },
      { capture: true },
    );
    const error = await rejected(() =>
      runTestMode(gpuDevice, program, renderer, new AbortController().signal),
    );
    expect((error as Error).message).toBe(
      "frame 0, buffer 1, at byte 4: a DRAW names pipeline 0, which the manifest doesn't have",
    );
    expect(device.openScopes).toBe(0);
  });

  test("an abort, such as a lost device, stops the run", async () => {
    const { device, gpuDevice, gpuContext } = fakes();
    const { program, renderer } = await startProgram(gpuDevice, gpuContext, files(), {
      capture: true,
    });
    const stop = new AbortController();
    device.onSubmit = (i) => {
      if (i === 2) {
        stop.abort(new HostError("the GPU device was lost"));
      }
    };
    const error = await rejected(() => runTestMode(gpuDevice, program, renderer, stop.signal));
    expect((error as Error).message).toBe("the GPU device was lost");
    expect(device.submitted.length).toBe(3);
  });
});

describe("play mode", () => {
  /** A scheduler the test runs by hand. */
  function frames() {
    const queue: ((now: number) => void)[] = [];
    return {
      schedule: (callback: (now: number) => void) => queue.push(callback),
      async tick(now: number) {
        const callbacks = queue.splice(0);
        for (const callback of callbacks) {
          callback(now);
        }
        await Bun.sleep(0);
      },
    };
  }

  test("one frame per animation frame, with the time since the first and the screen's size", async () => {
    const { gpuDevice } = fakes();
    const sink = new RecordingSink();
    const program = await Program.instantiate(
      wasm,
      Manifest.fromJson(await contractManifest()),
      sink,
    );
    const clock = frames();
    let size = { width: 640, height: 480 };
    play(
      gpuDevice,
      program,
      () => size,
      clock.schedule,
      new AbortController().signal,
      (e) => {
        throw e;
      },
    );
    await clock.tick(5000);
    size = { width: 800, height: 600 };
    await clock.tick(5250);
    const draws = sink.log.filter((l) => l.startsWith("draw"));
    const scene = (w: number, h: number, t: number) => {
      const bytes = new Uint8Array(16);
      const v = new DataView(bytes.buffer);
      v.setFloat32(0, w, true);
      v.setFloat32(4, h, true);
      v.setFloat32(8, t, true);
      return `draw 0 3 1 ${hex(bytes)}`;
    };
    expect(draws).toEqual([scene(640, 480, 0), scene(800, 600, 0.25)]);
  });

  test("a frame is skipped while the last one is still on the GPU", async () => {
    const { device, gpuDevice } = fakes();
    const sink = new RecordingSink();
    const program = await Program.instantiate(
      wasm,
      Manifest.fromJson(await contractManifest()),
      sink,
    );
    let release = () => {};
    device.workDone = () => new Promise((resolve) => (release = resolve));
    const clock = frames();
    play(
      gpuDevice,
      program,
      () => ({ width: 1, height: 1 }),
      clock.schedule,
      new AbortController().signal,
      (e) => {
        throw e;
      },
    );
    await clock.tick(0);
    await clock.tick(16);
    await clock.tick(33);
    expect(sink.log.filter((l) => l === "present").length).toBe(1);
    release();
    await Bun.sleep(0);
    await clock.tick(50);
    expect(sink.log.filter((l) => l === "present").length).toBe(2);
  });

  test("the first error stops the loop and is reported once", async () => {
    const { gpuDevice } = fakes();
    const m = Manifest.fromJson(await contractManifest());
    m.pipelines = []; // every draw now names a missing pipeline
    const program = await Program.instantiate(wasm, m, new RecordingSink());
    const clock = frames();
    const errors: string[] = [];
    play(
      gpuDevice,
      program,
      () => ({ width: 1, height: 1 }),
      clock.schedule,
      new AbortController().signal,
      (e) => errors.push((e as Error).message),
    );
    for (let t = 0; t < 5; t++) {
      await clock.tick(t * 16);
    }
    expect(errors).toEqual([
      "frame 0, buffer 1, at byte 4: a DRAW names pipeline 0, which the manifest doesn't have",
    ]);
  });

  test("an abort stops the loop", async () => {
    const { gpuDevice } = fakes();
    const sink = new RecordingSink();
    const program = await Program.instantiate(
      wasm,
      Manifest.fromJson(await contractManifest()),
      sink,
    );
    const clock = frames();
    const stop = new AbortController();
    play(
      gpuDevice,
      program,
      () => ({ width: 1, height: 1 }),
      clock.schedule,
      stop.signal,
      () => {},
    );
    await clock.tick(0);
    stop.abort();
    await clock.tick(16);
    await clock.tick(33);
    expect(sink.log.filter((l) => l === "present").length).toBe(1);
  });
});
