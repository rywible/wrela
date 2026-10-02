// The WebGPU side: pipelines built from the manifest, and the screen renderer that turns checked
// commands into WebGPU calls (runtime/command-stream.md, "The screen target and render state").

import { type Binding, inlineUniformSize, type Manifest, type Stage } from "./manifest.ts";
import { type CommandSink, errorMessage, HostError } from "./program.ts";
import type { Clear } from "./stream.ts";

// WebGPU's flag values, fixed by the spec. Spelled out here rather than read from the GPU* globals
// so the code runs (and is tested) where those globals don't exist.
export const BufferUsage = { MAP_READ: 0x1, COPY_DST: 0x8, UNIFORM: 0x40 } as const;
export const TextureUsage = { COPY_SRC: 0x1, RENDER_ATTACHMENT: 0x10 } as const;
export const ShaderStage = { VERTEX: 0x1, FRAGMENT: 0x2, COMPUTE: 0x4 } as const;
export const MapMode = { READ: 0x1 } as const;

/** The screen target's format: version 0 has only this one, stored as it is (no sRGB encoding). */
export const SCREEN_FORMAT: GPUTextureFormat = "rgba8unorm";

/** WebGPU requires a texture-to-buffer copy's rows to be a multiple of this many bytes. */
const COPY_ROW_ALIGNMENT = 256;

export type BuiltPipeline =
  | {
      readonly kind: "render";
      readonly pipeline: GPURenderPipeline;
      /** Group 0's layout and the uniform's size, when the pipeline has an inline uniform. */
      readonly uniform: { readonly layout: GPUBindGroupLayout; readonly size: number } | undefined;
    }
  | { readonly kind: "compute"; readonly pipeline: GPUComputePipeline };

function stageFlags(stages: readonly Stage[]): number {
  let flags = 0;
  for (const stage of stages) {
    flags |=
      stage === "vertex"
        ? ShaderStage.VERTEX
        : stage === "fragment"
          ? ShaderStage.FRAGMENT
          : ShaderStage.COMPUTE;
  }
  return flags;
}

function bufferLayout(binding: Binding): GPUBufferBindingLayout {
  const type: GPUBufferBindingType =
    binding.kind === "uniform"
      ? "uniform"
      : binding.kind === "storage_read"
        ? "read-only-storage"
        : "storage";
  // A runtime-sized array must hold at least one element.
  return { type, minBindingSize: binding.size + (binding.stride ?? 0) };
}

/** Pulls the error messages out of a shader module's compilation info, with their positions. */
async function shaderErrors(path: string, module: GPUShaderModule): Promise<string[]> {
  const info = await module.getCompilationInfo();
  return info.messages
    .filter((m) => m.type === "error")
    .map((m) => `${path}:${m.lineNum}:${m.linePos}: ${m.message}`);
}

/**
 * Creates every pipeline the manifest names, from the WGSL `sources` (keyed by manifest path),
 * before the first frame. Bind group layouts come from the manifest, never from the shader.
 * Throws `HostError` for a shader that doesn't compile or a pipeline WebGPU rejects.
 */
export async function createPipelines(
  device: GPUDevice,
  manifest: Manifest,
  sources: ReadonlyMap<string, string>,
): Promise<Map<number, BuiltPipeline>> {
  device.pushErrorScope("validation");
  const modules = new Map<string, GPUShaderModule>();
  const built = new Map<number, BuiltPipeline>();
  let failure: unknown;
  try {
    for (const pipeline of manifest.pipelines) {
      let module = modules.get(pipeline.module);
      if (module === undefined) {
        const code = sources.get(pipeline.module);
        if (code === undefined) {
          throw new HostError(`no source for the WGSL module \`${pipeline.module}\``);
        }
        module = device.createShaderModule({ label: pipeline.module, code });
        const errors = await shaderErrors(pipeline.module, module);
        if (errors.length > 0) {
          throw new HostError(
            `the WGSL module \`${pipeline.module}\` doesn't compile: ${errors.join("\n")}`,
          );
        }
        modules.set(pipeline.module, module);
      }

      const label = `pipeline ${pipeline.id}`;
      const groupCount = Math.max(0, ...pipeline.bindings.map((b) => b.group + 1));
      const groups = Array.from({ length: groupCount }, (_, group) =>
        device.createBindGroupLayout({
          label: `${label}, group ${group}`,
          entries: pipeline.bindings
            .filter((b) => b.group === group)
            .map((b) => ({
              binding: b.binding,
              visibility: stageFlags(b.visibility),
              buffer: bufferLayout(b),
            })),
        }),
      );
      const layout = device.createPipelineLayout({ label, bindGroupLayouts: groups });
      try {
        if (pipeline.kind === "render") {
          const render = await device.createRenderPipelineAsync({
            label,
            layout,
            vertex: { module, entryPoint: pipeline.vertex },
            fragment: {
              module,
              entryPoint: pipeline.fragment,
              targets: [{ format: pipeline.color_target }],
            },
            primitive: { topology: "triangle-list", cullMode: "none" },
          });
          const size = inlineUniformSize(pipeline);
          const group0 = groups[0];
          built.set(pipeline.id, {
            kind: "render",
            pipeline: render,
            uniform:
              size === undefined || group0 === undefined ? undefined : { layout: group0, size },
          });
        } else {
          const compute = await device.createComputePipelineAsync({
            label,
            layout,
            compute: { module, entryPoint: pipeline.compute },
          });
          built.set(pipeline.id, { kind: "compute", pipeline: compute });
        }
      } catch (e) {
        const problem = errorMessage(e);
        throw new HostError(`${label} (\`${pipeline.module}\`) can't be created: ${problem}`, {
          cause: e,
        });
      }
    }
  } catch (e) {
    failure = e;
  }
  // Pop the scope whatever happened, so it never leaks into the frames.
  const error = await device.popErrorScope();
  if (failure !== undefined) {
    throw failure;
  }
  if (error !== null) {
    throw new HostError(`creating the pipelines failed: ${error.message}`);
  }
  return built;
}

/** One draw's uniform: its own buffer and bind group, so no later draw overwrites its bytes. */
interface UniformSlot {
  readonly buffer: GPUBuffer;
  readonly bindGroup: GPUBindGroup;
}

/** A captured screen: tightly packed RGBA8 rows, top row first. */
export interface Capture {
  readonly width: number;
  readonly height: number;
  /** The screen texture's format, which the bytes were converted from. */
  readonly format: GPUTextureFormat;
  readonly rgba: Uint8Array;
}

interface PendingCapture {
  readonly buffer: GPUBuffer;
  readonly width: number;
  readonly height: number;
  readonly bytesPerRow: number;
  readonly format: GPUTextureFormat;
}

/** The parts of a canvas context the renderer uses. */
export type ScreenContext = Pick<GPUCanvasContext, "getCurrentTexture">;

/**
 * Turns checked commands into WebGPU calls on the screen target: `BEGIN_SCREEN_PASS` opens a
 * render pass on the canvas's current texture, `DRAW` draws in it, `PRESENT` ends the pass and
 * submits the frame (the canvas shows it when the worker yields).
 */
export class ScreenRenderer implements CommandSink {
  private encoder: GPUCommandEncoder | undefined;
  private pass: GPURenderPassEncoder | undefined;
  private texture: GPUTexture | undefined;
  /** Each pipeline's uniform slots, and how many this frame has used. */
  private readonly slots = new Map<number, UniformSlot[]>();
  private readonly used = new Map<number, number>();
  private captureRequested = false;
  private pending: PendingCapture | undefined;

  constructor(
    private readonly device: GPUDevice,
    private readonly context: ScreenContext,
    private readonly pipelines: ReadonlyMap<number, BuiltPipeline>,
  ) {}

  beginScreenPass(clear: Clear): void {
    if (this.pass !== undefined) {
      throw new HostError("internal: a screen pass is already open");
    }
    this.texture = this.context.getCurrentTexture();
    this.encoder = this.device.createCommandEncoder({ label: "frame" });
    this.pass = this.encoder.beginRenderPass({
      label: "screen pass",
      colorAttachments: [
        {
          view: this.texture.createView(),
          clearValue: { r: clear[0], g: clear[1], b: clear[2], a: clear[3] },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
  }

  draw(pipeline: number, vertexCount: number, instanceCount: number, uniforms: Uint8Array): void {
    const pass = this.pass;
    const built = this.pipelines.get(pipeline);
    if (pass === undefined || built?.kind !== "render") {
      throw new HostError(
        `internal: a DRAW of pipeline ${pipeline} reached the renderer unchecked`,
      );
    }
    pass.setPipeline(built.pipeline);
    if (built.uniform !== undefined) {
      const slot = this.nextSlot(pipeline, built.uniform);
      // writeBuffer copies now, so `uniforms` (a view of WASM memory) may change after this.
      this.device.queue.writeBuffer(slot.buffer, 0, uniforms as Uint8Array<ArrayBuffer>);
      pass.setBindGroup(0, slot.bindGroup);
    }
    pass.draw(vertexCount, instanceCount);
  }

  present(): void {
    const { encoder, pass, texture } = this;
    if (encoder === undefined || pass === undefined || texture === undefined) {
      throw new HostError("internal: PRESENT reached the renderer with no screen pass open");
    }
    pass.end();
    if (this.captureRequested) {
      this.captureRequested = false;
      this.pending = this.copyForCapture(encoder, texture);
    }
    this.device.queue.submit([encoder.finish()]);
    this.encoder = undefined;
    this.pass = undefined;
    this.texture = undefined;
    this.used.clear();
  }

  /** Captures the screen target at the next `PRESENT`; `readCapture` then returns it. */
  requestCapture(): void {
    this.captureRequested = true;
  }

  /** The screen as captured at the last `PRESENT` after `requestCapture`, as RGBA8. */
  async readCapture(): Promise<Capture> {
    const pending = this.pending;
    if (pending === undefined) {
      throw new HostError("no frame was captured: the capture's PRESENT never came");
    }
    this.pending = undefined;
    const { buffer, width, height, bytesPerRow, format } = pending;
    await buffer.mapAsync(MapMode.READ);
    const mapped = new Uint8Array(buffer.getMappedRange());
    const rgba = new Uint8Array(width * height * 4);
    for (let y = 0; y < height; y++) {
      rgba.set(mapped.subarray(y * bytesPerRow, y * bytesPerRow + width * 4), y * width * 4);
    }
    buffer.unmap();
    buffer.destroy();
    if (format === "bgra8unorm") {
      for (let i = 0; i < rgba.length; i += 4) {
        const b = rgba[i] ?? 0;
        rgba[i] = rgba[i + 2] ?? 0;
        rgba[i + 2] = b;
      }
    } else if (format !== "rgba8unorm") {
      throw new HostError(`can't capture a screen of format ${format}`);
    }
    return { width, height, format, rgba };
  }

  private copyForCapture(encoder: GPUCommandEncoder, texture: GPUTexture): PendingCapture {
    const { width, height, format } = texture;
    const bytesPerRow = Math.ceil((width * 4) / COPY_ROW_ALIGNMENT) * COPY_ROW_ALIGNMENT;
    const buffer = this.device.createBuffer({
      label: "capture",
      size: bytesPerRow * height,
      usage: BufferUsage.COPY_DST | BufferUsage.MAP_READ,
    });
    encoder.copyTextureToBuffer(
      { texture },
      { buffer, bytesPerRow, rowsPerImage: height },
      { width, height },
    );
    return { buffer, width, height, bytesPerRow, format };
  }

  private nextSlot(
    pipeline: number,
    uniform: { layout: GPUBindGroupLayout; size: number },
  ): UniformSlot {
    const index = this.used.get(pipeline) ?? 0;
    this.used.set(pipeline, index + 1);
    let slots = this.slots.get(pipeline);
    if (slots === undefined) {
      slots = [];
      this.slots.set(pipeline, slots);
    }
    let slot = slots[index];
    if (slot === undefined) {
      const label = `pipeline ${pipeline}, uniform ${index}`;
      const buffer = this.device.createBuffer({
        label,
        size: uniform.size,
        usage: BufferUsage.UNIFORM | BufferUsage.COPY_DST,
      });
      const bindGroup = this.device.createBindGroup({
        label,
        layout: uniform.layout,
        entries: [{ binding: 0, resource: { buffer } }],
      });
      slot = { buffer, bindGroup };
      slots.push(slot);
    }
    return slot;
  }
}
