// A fake WebGPU device and canvas context, enough of the API for the runtime, that records what
// the GPU would execute. It models the queue's ordering (writes and submits run in call order, so
// a draw sees the bytes its uniform buffer holds when its submit runs), checks the rules the
// runtime could break (usages, row alignment, bound groups, balanced error scopes), and reports
// breaks the way WebGPU does: into the innermost matching error scope, else as uncaptured.

import { BufferUsage, TextureUsage } from "../src/gpu.ts";
import { hex } from "./programs.ts";

export interface ExecutedDraw {
  readonly pipeline: string;
  readonly vertexCount: number;
  readonly instanceCount: number;
  /** The uniform buffer's bytes when the draw ran, or undefined with no bind group. */
  readonly uniforms: string | undefined;
}

export interface ExecutedPass {
  readonly clear: readonly number[];
  readonly target: FakeTexture;
  readonly draws: ExecutedDraw[];
}

export class FakeBuffer {
  readonly data: Uint8Array;
  destroyed = false;
  mapped = false;
  constructor(
    readonly label: string,
    readonly size: number,
    readonly usage: number,
  ) {
    this.data = new Uint8Array(size);
  }
  async mapAsync(): Promise<void> {
    if ((this.usage & BufferUsage.MAP_READ) === 0) {
      throw new Error(`buffer ${this.label} can't be mapped`);
    }
    this.mapped = true;
  }
  getMappedRange(): ArrayBuffer {
    if (!this.mapped) {
      throw new Error(`buffer ${this.label} isn't mapped`);
    }
    return this.data.buffer as ArrayBuffer;
  }
  unmap(): void {
    this.mapped = false;
  }
  destroy(): void {
    this.destroyed = true;
  }
}

export class FakeTexture {
  readonly pixels: Uint8Array;
  constructor(
    readonly width: number,
    readonly height: number,
    readonly format: GPUTextureFormat,
    readonly usage: number,
  ) {
    this.pixels = new Uint8Array(width * height * 4);
  }
  createView() {
    return { texture: this };
  }
}

interface FakeModule {
  readonly label: string;
  readonly code: string;
  getCompilationInfo(): Promise<{ messages: GPUCompilationMessage[] }>;
}

interface FakePipeline {
  readonly label: string;
  readonly descriptor: GPURenderPipelineDescriptor | GPUComputePipelineDescriptor;
}

interface FakeBindGroup {
  readonly descriptor: GPUBindGroupDescriptor;
}

type PassCommand =
  | { readonly cmd: "pipeline"; readonly pipeline: FakePipeline }
  | { readonly cmd: "bind"; readonly index: number; readonly group: FakeBindGroup }
  | { readonly cmd: "draw"; readonly vertexCount: number; readonly instanceCount: number };

type Op =
  | {
      readonly op: "pass";
      readonly clear: readonly number[];
      readonly target: FakeTexture;
      readonly commands: PassCommand[];
    }
  | {
      readonly op: "copy";
      readonly texture: FakeTexture;
      readonly buffer: FakeBuffer;
      readonly bytesPerRow: number;
    };

class FakePass {
  readonly commands: PassCommand[] = [];
  ended = false;
  setPipeline(pipeline: FakePipeline): void {
    this.commands.push({ cmd: "pipeline", pipeline });
  }
  setBindGroup(index: number, group: FakeBindGroup): void {
    this.commands.push({ cmd: "bind", index, group });
  }
  draw(vertexCount: number, instanceCount = 1): void {
    this.commands.push({ cmd: "draw", vertexCount, instanceCount });
  }
  end(): void {
    this.ended = true;
  }
}

class FakeEncoder {
  readonly ops: Op[] = [];
  private open: FakePass | undefined;
  constructor(private readonly device: FakeDevice) {}

  beginRenderPass(descriptor: GPURenderPassDescriptor): FakePass {
    const attachment = [...descriptor.colorAttachments][0];
    if (attachment == null) {
      throw new Error("a pass with no colour attachment");
    }
    const target = (attachment.view as unknown as { texture: FakeTexture }).texture;
    if ((target.usage & TextureUsage.RENDER_ATTACHMENT) === 0) {
      this.device.error("the pass's target isn't a render attachment");
    }
    const c = attachment.clearValue as GPUColorDict;
    const pass = new FakePass();
    this.open = pass;
    this.ops.push({ op: "pass", clear: [c.r, c.g, c.b, c.a], target, commands: pass.commands });
    return pass;
  }

  copyTextureToBuffer(
    source: GPUTexelCopyTextureInfo,
    destination: GPUTexelCopyBufferInfo,
    size: GPUExtent3DDict,
  ): void {
    const texture = source.texture as unknown as FakeTexture;
    const buffer = destination.buffer as unknown as FakeBuffer;
    const bytesPerRow = destination.bytesPerRow ?? 0;
    if (this.open !== undefined && !this.open.ended) {
      this.device.error("a copy while a pass is open");
    }
    if ((texture.usage & TextureUsage.COPY_SRC) === 0) {
      this.device.error("the texture can't be copied from");
    }
    if ((buffer.usage & BufferUsage.COPY_DST) === 0) {
      this.device.error("the buffer can't be copied to");
    }
    if (bytesPerRow % 256 !== 0) {
      this.device.error(`bytesPerRow ${bytesPerRow} isn't a multiple of 256`);
    }
    if (size.width !== texture.width || size.height !== texture.height) {
      this.device.error("the copy's size isn't the texture's");
    }
    if (bytesPerRow * texture.height > buffer.size) {
      this.device.error("the copy runs past the buffer");
    }
    this.ops.push({ op: "copy", texture, buffer, bytesPerRow });
  }

  finish(): { ops: Op[] } {
    if (this.open !== undefined && !this.open.ended) {
      this.device.error("finish() with a pass open");
    }
    return { ops: this.ops };
  }
}

/** Bytes of a colour in a texture's format: rgba8unorm or bgra8unorm. */
function texel(color: readonly number[], format: GPUTextureFormat): number[] {
  const [r, g, b, a] = color.map((c) => Math.round(Math.min(1, Math.max(0, c)) * 255)) as [
    number,
    number,
    number,
    number,
  ];
  return format === "bgra8unorm" ? [b, g, r, a] : [r, g, b, a];
}

export class FakeDevice {
  readonly label = "fake";
  readonly limits = { maxTextureDimension2D: 8192 } as GPUSupportedLimits;
  /** What each submit executed: its passes, in order. */
  readonly submitted: ExecutedPass[][] = [];
  readonly buffers: FakeBuffer[] = [];
  readonly modules: FakeModule[] = [];
  readonly bindGroupLayouts: GPUBindGroupLayoutDescriptor[] = [];
  readonly pipelines: FakePipeline[] = [];
  readonly uncaptured: string[] = [];
  private readonly scopes: { filter: GPUErrorFilter; errors: string[] }[] = [];
  /** Called before each submit runs, with its index: a hook for injecting errors. */
  onSubmit: (index: number) => void = () => {};
  /** What `onSubmittedWorkDone` returns; tests replace it to hold work "on the GPU". */
  workDone: () => Promise<void> = () => Promise.resolve();

  readonly queue = {
    writeBuffer: (buffer: FakeBuffer, offset: number, data: Uint8Array) => {
      if (data.length % 4 !== 0 || offset + data.length > buffer.size) {
        this.error("a write outside the buffer or unaligned");
      } else if ((buffer.usage & BufferUsage.COPY_DST) === 0) {
        this.error("the buffer can't be written");
      } else {
        buffer.data.set(data, offset);
      }
    },
    submit: (commandBuffers: { ops: Op[] }[]) => {
      this.onSubmit(this.submitted.length);
      const passes: ExecutedPass[] = [];
      for (const { ops } of commandBuffers) {
        for (const op of ops) {
          this.execute(op, passes);
        }
      }
      this.submitted.push(passes);
    },
    onSubmittedWorkDone: () => this.workDone(),
  };

  /** Reports a WebGPU error, as the device would. */
  error(message: string, filter: GPUErrorFilter = "validation"): void {
    for (let i = this.scopes.length - 1; i >= 0; i--) {
      const scope = this.scopes[i];
      if (scope?.filter === filter) {
        scope.errors.push(message);
        return;
      }
    }
    this.uncaptured.push(message);
  }

  get openScopes(): number {
    return this.scopes.length;
  }

  pushErrorScope(filter: GPUErrorFilter): void {
    this.scopes.push({ filter, errors: [] });
  }

  async popErrorScope(): Promise<{ message: string } | null> {
    const scope = this.scopes.pop();
    if (scope === undefined) {
      throw new Error("popErrorScope with no scope open");
    }
    const first = scope.errors[0];
    return first === undefined ? null : { message: first };
  }

  createShaderModule({ label, code }: GPUShaderModuleDescriptor): FakeModule {
    // A line containing ERROR is a compile error, at the column where ERROR starts.
    const messages = code.split("\n").flatMap((line, i) => {
      const column = line.indexOf("ERROR");
      if (column < 0) {
        return [];
      }
      return [
        {
          type: "error",
          lineNum: i + 1,
          linePos: column + 1,
          offset: 0,
          length: 5,
          message: "unexpected ERROR",
        } as GPUCompilationMessage,
      ];
    });
    if (messages.length > 0) {
      this.error(`shader module ${label} has errors`);
    }
    const module = { label: label ?? "", code, getCompilationInfo: async () => ({ messages }) };
    this.modules.push(module);
    return module;
  }

  createBindGroupLayout(descriptor: GPUBindGroupLayoutDescriptor): GPUBindGroupLayoutDescriptor {
    this.bindGroupLayouts.push(descriptor);
    return descriptor;
  }

  createPipelineLayout(descriptor: GPUPipelineLayoutDescriptor): GPUPipelineLayoutDescriptor {
    return descriptor;
  }

  private entryPoint(module: unknown, name: string | undefined): void {
    const code = (module as FakeModule).code;
    if (name === undefined || !code.includes(`fn ${name}(`)) {
      throw new Error(`entry point \`${name}\` isn't in the module`);
    }
  }

  async createRenderPipelineAsync(descriptor: GPURenderPipelineDescriptor): Promise<FakePipeline> {
    this.entryPoint(descriptor.vertex.module, descriptor.vertex.entryPoint ?? undefined);
    this.entryPoint(descriptor.fragment?.module, descriptor.fragment?.entryPoint ?? undefined);
    const pipeline = { label: descriptor.label ?? "", descriptor };
    this.pipelines.push(pipeline);
    return pipeline;
  }

  async createComputePipelineAsync(
    descriptor: GPUComputePipelineDescriptor,
  ): Promise<FakePipeline> {
    this.entryPoint(descriptor.compute.module, descriptor.compute.entryPoint ?? undefined);
    const pipeline = { label: descriptor.label ?? "", descriptor };
    this.pipelines.push(pipeline);
    return pipeline;
  }

  createBuffer({ label, size, usage }: GPUBufferDescriptor): FakeBuffer {
    const buffer = new FakeBuffer(label ?? "", size, usage);
    this.buffers.push(buffer);
    return buffer;
  }

  createBindGroup(descriptor: GPUBindGroupDescriptor): FakeBindGroup {
    return { descriptor };
  }

  createCommandEncoder(): FakeEncoder {
    return new FakeEncoder(this);
  }

  private execute(op: Op, passes: ExecutedPass[]): void {
    if (op.op === "copy") {
      const { texture, buffer, bytesPerRow } = op;
      buffer.data.fill(0xaa); // row padding shows up if it leaks into a capture
      for (let y = 0; y < texture.height; y++) {
        const row = texture.pixels.subarray(y * texture.width * 4, (y + 1) * texture.width * 4);
        buffer.data.set(row, y * bytesPerRow);
      }
      return;
    }
    const { clear, target } = op;
    const bytes = texel(clear, target.format);
    for (let i = 0; i < target.pixels.length; i += 4) {
      target.pixels.set(bytes, i);
    }
    const draws: ExecutedDraw[] = [];
    let pipeline: FakePipeline | undefined;
    let group: FakeBindGroup | undefined;
    for (const command of op.commands) {
      if (command.cmd === "pipeline") {
        pipeline = command.pipeline;
      } else if (command.cmd === "bind") {
        group = command.group;
      } else {
        if (pipeline === undefined) {
          this.error("a draw with no pipeline");
          continue;
        }
        const layouts = (pipeline.descriptor.layout as unknown as GPUPipelineLayoutDescriptor)
          .bindGroupLayouts;
        const needsGroup = [...layouts].some(
          (l) => [...((l as unknown as GPUBindGroupLayoutDescriptor).entries ?? [])].length > 0,
        );
        if (needsGroup && group === undefined) {
          this.error("a draw with its bind group missing");
        }
        // A group left bound by an earlier pipeline means nothing to one whose layout has none.
        const entry =
          group === undefined || !needsGroup ? undefined : [...group.descriptor.entries][0];
        const buffer = (entry?.resource as { buffer: FakeBuffer } | undefined)?.buffer;
        draws.push({
          pipeline: pipeline.label,
          vertexCount: command.vertexCount,
          instanceCount: command.instanceCount,
          uniforms: buffer === undefined ? undefined : hex(buffer.data),
        });
      }
    }
    passes.push({ clear, target, draws });
  }
}

/** A canvas context whose current texture is a `FakeTexture` of the canvas's size. */
export class FakeContext {
  configuration: GPUCanvasConfiguration | undefined;
  private current: FakeTexture | undefined;
  constructor(
    public width: number,
    public height: number,
  ) {}

  configure(configuration: GPUCanvasConfiguration): void {
    this.configuration = configuration;
    this.current = undefined;
  }

  getCurrentTexture(): FakeTexture {
    const c = this.configuration;
    if (c === undefined) {
      throw new Error("getCurrentTexture before configure");
    }
    if (this.current?.width !== this.width || this.current.height !== this.height) {
      this.current = new FakeTexture(
        this.width,
        this.height,
        c.format,
        c.usage ?? TextureUsage.RENDER_ATTACHMENT,
      );
    }
    return this.current;
  }
}

/** The fakes, typed as what the runtime takes. */
export function fakes(width = 1920, height = 1080) {
  const device = new FakeDevice();
  const context = new FakeContext(width, height);
  return {
    device,
    context,
    gpuDevice: device as unknown as GPUDevice,
    gpuContext: context as unknown as GPUCanvasContext,
  };
}
