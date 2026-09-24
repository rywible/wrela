import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type RenderSurface } from "@wrela/model";

import {
  defaultPhysicalAtmosphere,
  PHYSICAL_AERIAL_SIZE,
  PHYSICAL_ATMOSPHERE_FRAME_FLOATS,
  PHYSICAL_CLOUD_VIEW_SIZE,
  PHYSICAL_DIFFUSE_SIZE,
  PHYSICAL_SKY_SIZE,
} from "./atmosphere";
import {
  GLOBAL_FLOATS,
  IncompleteRenderError,
  QUALITY_PROFILES,
  type RendererOptions,
  VERTEX_FLOATS,
  WebGPURenderer,
} from "./index";
import { RELIEF_VERTEX_FLOATS } from "./packing";
import { meshVertexBufferLayouts } from "./vertex-layout";
import { WaterBodyBuffers } from "./water-body";

const mesh = {
  positions: new Float32Array([-1, -1, 0, 1, -1, 0, 1, 1, 0, -1, 1, 0]),
  normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]),
  indices: new Uint32Array([0, 1, 2, 0, 2, 3]),
  bounds: { min: [-1, -1, 0] as [number, number, number], max: [1, 1, 0] as [number, number, number] },
};
const surface: RenderSurface = {
  id: "required",
  source: "required",
  mesh,
  matrix: identityMatrix(),
  material: {
    color: [1, 1, 1],
    secondary: [1, 1, 1],
    roughness: 1,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  },
};
const scene: EvaluatedScene = {
  surfaces: [surface],
  camera: { position: [0, 0, 5], target: [0, 0, 0], fov: 50 },
  environment: {
    sunDirection: [0, 1, 1],
    sunColor: [1, 1, 1],
    sunIntensity: 1,
    ambient: 1,
    skyColor: [1, 1, 1],
    horizonColor: [1, 1, 1],
    groundColor: [1, 1, 1],
    fogDensity: 0,
    wind: [0, 0, 0],
    exposure: 1,
  },
  time: 0,
  mode: "beauty",
  grid: false,
};
/** Records actual renderer allocation/submission decisions without pretending to validate WGSL or a GPU. */
function harness(options: RendererOptions = {}) {
  Object.assign(globalThis, {
    GPUBufferUsage: { VERTEX: 1, INDEX: 2, COPY_DST: 4, UNIFORM: 8, STORAGE: 16, COPY_SRC: 32, INDIRECT: 64 },
    GPUTextureUsage: {
      RENDER_ATTACHMENT: 1,
      TEXTURE_BINDING: 2,
      STORAGE_BINDING: 4,
      COPY_SRC: 8,
      COPY_DST: 16,
    },
    GPUShaderStage: { VERTEX: 1, FRAGMENT: 2, COMPUTE: 4 },
  });
  const buffers: { label: string; size: number; destroyed?: boolean }[] = [];
  const writes: { label: string; offset: number; bytes: number }[] = [];
  const textureWrites: number[] = [];
  const bindings: { slot: number; label: string; pipeline: string; stride: number | undefined }[] = [];
  type Pipeline = { label: string; buffers: GPUVertexBufferLayout[] };
  const reliefPipelines = new Map<Pipeline, Pipeline>();
  const meshPipeline = (label: string): Pipeline => {
    const ordinary = { label, buffers: meshVertexBufferLayouts() };
    reliefPipelines.set(ordinary, {
      label: `${label} / compact relief`,
      buffers: meshVertexBufferLayouts(true),
    });
    return ordinary;
  };
  const meshPipelines = (label: string) =>
    new Map(["1:false", "1:true", "4:false", "4:true"].map((key) => [key, meshPipeline(`${label} ${key}`)]));
  const draws: { count: number; first: number }[] = [];
  const texture = (descriptor: { size?: number[]; sampleCount?: number; format?: string } = {}) => ({
    width: descriptor.size?.[0] ?? 64,
    height: descriptor.size?.[1] ?? 64,
    sampleCount: descriptor.sampleCount ?? 1,
    format: descriptor.format ?? "rgba16float",
    createView: () => ({}),
    destroy: () => {},
  });
  const pass = () => {
    let pipeline: Pipeline | undefined;
    return {
      setPipeline: (next: Pipeline) => {
        pipeline = next;
      },
      setBindGroup: () => {},
      setVertexBuffer: (slot: number, buffer: { label?: string }) => {
        bindings.push({
          slot,
          label: buffer.label ?? "",
          pipeline: pipeline?.label ?? "",
          stride: pipeline?.buffers?.[slot]?.arrayStride,
        });
      },
      setIndexBuffer: () => {},
      draw: () => {},
      drawIndexed: (count: number, _instances: number, first: number) => {
        draws.push({ count, first });
      },
      end: () => {},
    };
  };
  const device = {
    destroy: () => {},
    limits: { maxTextureDimension2D: 4096 },
    queue: {
      writeBuffer: (
        buffer: { label?: string } | undefined,
        offset: number,
        data: { byteLength: number },
        _start?: number,
        bytes?: number,
      ) => {
        writes.push({ label: buffer?.label ?? "", offset, bytes: bytes ?? data.byteLength });
      },
      writeTexture: (
        _destination: unknown,
        _data: unknown,
        _layout: unknown,
        extent: { width: number; height: number },
      ) => {
        textureWrites.push(extent.width * extent.height);
      },
      submit: () => {},
    },
    createTexture: texture,
    createSampler: () => ({}),
    createBindGroupLayout: () => ({}),
    createPipelineLayout: () => ({}),
    createShaderModule: () => ({}),
    createRenderPipeline: () => ({ getBindGroupLayout: () => ({}) }),
    createComputePipeline: () => ({ getBindGroupLayout: () => ({}) }),
    createBindGroup: () => ({}),
    createBuffer: (descriptor: { label: string; size: number }) => {
      buffers.push(descriptor);
      return {
        size: descriptor.size,
        label: descriptor.label,
        destroy: () => {
          (descriptor as { destroyed?: boolean }).destroyed = true;
        },
      };
    },
    createCommandEncoder: () => ({
      beginRenderPass: pass,
      beginComputePass: () => ({
        setPipeline: () => {},
        setBindGroup: () => {},
        dispatchWorkgroups: () => {},
        end: () => {},
      }),
      copyTextureToTexture: () => {},
      finish: () => ({}),
    }),
  };
  const Constructor = WebGPURenderer as unknown as new (
    canvas: HTMLCanvasElement,
    options: RendererOptions,
  ) => WebGPURenderer;
  const renderer = new Constructor({ clientWidth: 64, clientHeight: 64 } as HTMLCanvasElement, {
    pixelRatio: 1,
    ...options,
    renderCompiler: { visibility: true, ...options.renderCompiler },
  });
  const atmosphere = options.renderCompiler?.atmosphere ?? defaultPhysicalAtmosphere();
  const atmosphereGpuBytes =
    (PHYSICAL_DIFFUSE_SIZE[0] * PHYSICAL_DIFFUSE_SIZE[1] +
      2 * PHYSICAL_SKY_SIZE[0] * PHYSICAL_SKY_SIZE[1] +
      PHYSICAL_CLOUD_VIEW_SIZE[0] * PHYSICAL_CLOUD_VIEW_SIZE[1] +
      2 * PHYSICAL_AERIAL_SIZE[0] * PHYSICAL_AERIAL_SIZE[1] * PHYSICAL_AERIAL_SIZE[2]) *
      8 +
    PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4 +
    9 * 16;
  Object.assign(renderer, {
    device,
    context: { getCurrentTexture: texture, unconfigure: () => {} },
    shadowTexture: texture(),
    sunBuffer: { size: 272, destroy: () => {} },
    atmosphereBuffer: { size: atmosphere.byteLength, destroy: () => {} },
    atmosphereKey: atmosphere.key,
    materialLattice: { bytes: 32, texture: texture(), bounds: {}, update: () => 0, destroy: () => {} },
    pointShadows: {
      byteLength: 0,
      bufferBytes: 0,
      view: {},
      parameters: {},
      lastPasses: 0,
      lastDrawCalls: 0,
      prepare: () => [],
      encode: () => 0,
      destroy: () => {},
    },
    indirectLighting: {
      bytes: 64,
      buffer: { size: 64 },
      update: () => ({ uploaded: 0, changed: false }),
      dispose: () => {},
      encodeRelight: () => {},
    },
    thinFallback: { bytes: 1, texture: texture(), view: {} },
    thinSampler: {},
    waterBodyBuffers: new WaterBodyBuffers(device as unknown as GPUDevice),
    waterSpectrumPrograms: { bytes: 0, textureBytes: 0, fallbackView: {}, sampler: {}, destroy: () => {} },
    atmosphereGpu: {
      byteLength: atmosphereGpuBytes,
      bufferByteLength: PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4 + 9 * 16,
      frameUploadBytes: PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4,
      skyView: {},
      reflection: { view: {}, lutView: {} },
      lightingRevision: 0,
      cloudView: {},
      irradianceBuffer: { size: 9 * 16 },
      aerialRadianceView: {},
      aerialTransmissionView: {},
      sampler: {},
      encode: () => 0,
      destroy: () => {},
    },
    skyPipelines: new Map([
      [1, {}],
      [4, {}],
    ]),
    reliefPipelines,
    shadowPipeline: meshPipeline("Shadow"),
    main: meshPipelines("Color"),
    environmentMain: meshPipelines("Color"),
    thinDepth: new Map([
      [1, meshPipeline("Thin depth 1")],
      [4, meshPipeline("Thin depth 4")],
    ]),
    thinMain: meshPipelines("Thin color"),
    visibilityPipelines: new Map([
      [1, meshPipeline("Visibility 1")],
      [4, meshPipeline("Visibility 4")],
    ]),
    waterMain: new Map(
      [1, 4].flatMap((samples) =>
        [false, true].flatMap((procedural) =>
          [false, true].map((coherent) => {
            const key = `${samples}:${procedural}:${coherent}`;
            return [key, meshPipeline(`Water ${key}`)];
          }),
        ),
      ),
    ),
    status: "ready",
    bytes:
      QUALITY_PROFILES[options.quality ?? "balanced"].shadowSize ** 2 * 4 +
      GLOBAL_FLOATS * 4 +
      272 +
      atmosphere.byteLength +
      atmosphereGpuBytes +
      32 +
      64 +
      1,
  });
  return { renderer, buffers, draws, writes, bindings, textureWrites };
}
test("GPU budget refusal is typed incompleteness and capture refuses to certify it", async () => {
  const { renderer } = harness({ maxGpuBytes: 0 });
  renderer.render(scene);
  expect(renderer.completeness.complete).toBe(false);
  expect(renderer.completeness.rejected.map(({ id }) => id)).toEqual(["required"]);
  expect(renderer.needsRender).toBe(false);
  await expect(renderer.capture()).rejects.toBeInstanceOf(IncompleteRenderError);
});
test("bounded uploads transition to complete only after all required geometry is resident", () => {
  const { renderer } = harness({ maxUploadBytesPerFrame: 4 });
  renderer.render(scene);
  expect(renderer.completeness.uploading).toEqual(["required"]);
  expect(renderer.completeness.complete).toBe(false);
  const uploadFrames = Math.ceil(
    ((mesh.positions.length / 3) * VERTEX_FLOATS * 4 + mesh.indices.byteLength) / 4,
  );
  for (let frame = 0; frame <= uploadFrames && renderer.needsRender; frame++) renderer.render(scene);
  expect(renderer.completeness.complete).toBe(true);
  expect(renderer.completeness.rendered).toEqual(["required"]);
  expect(renderer.completeness.rejected).toEqual([]);
});
test("material ranges allocate shared vertices once and submit their actual index ranges", () => {
  const { renderer, buffers, draws } = harness({ antialiasing: "spatial" });
  renderer.render({
    ...scene,
    surfaces: [
      { ...surface, id: "first-material", drawRange: { start: 0, count: 3 } },
      {
        ...surface,
        id: "second-material",
        drawRange: { start: 3, count: 3 },
        material: { ...surface.material, metallic: 1 },
      },
    ],
  });
  expect(buffers.filter(({ label }) => label.startsWith("Vertices"))).toHaveLength(1);
  expect(buffers.filter(({ label }) => label.startsWith("Indices"))).toHaveLength(1);
  expect(draws).toEqual([
    { count: 3, first: 0 },
    { count: 3, first: 3 },
    { count: 3, first: 0 },
    { count: 3, first: 3 },
  ]);
  expect(renderer.measurements.triangles).toBe(2);
});
test("offscreen surfaces allocate nothing when neither camera nor shadow needs them", () => {
  const { renderer, buffers } = harness({ antialiasing: "spatial" });
  const matrix = identityMatrix();
  matrix[12] = 10000;
  renderer.render({ ...scene, surfaces: [{ ...surface, matrix }] });
  expect(renderer.completeness.culled).toEqual(["required"]);
  expect(renderer.completeness.complete).toBe(true);
  expect(buffers).toHaveLength(0);
});
test("out-of-bounds material draw ranges are rejected before buffer allocation", () => {
  const { renderer, buffers } = harness({ antialiasing: "spatial" });
  renderer.render({ ...scene, surfaces: [{ ...surface, drawRange: { start: 3, count: 6 } }] });
  expect(renderer.completeness.rejected[0].reason).toContain("draw range");
  expect(buffers).toHaveLength(0);
});

test("owned texture and buffer memory are reported separately and large targets reserve residency capacity", () => {
  const { renderer } = harness();
  Object.assign(renderer, { canvas: { clientWidth: 3840, clientHeight: 2160 } });
  renderer.render(scene);
  const measurements = renderer.measurements;
  expect(measurements.gpuBytes).toBe((measurements.ownedTextureBytes ?? 0) + (measurements.bufferBytes ?? 0));
  expect(measurements.ownedTextureBytes).toBeGreaterThan(100 * 1024 * 1024);
  expect(measurements.gpuBytes).toBeLessThan(QUALITY_PROFILES.balanced.maxGpuBytes);
  expect(measurements.outputResolution).toEqual([3840, 2160]);
  expect(measurements.renderResolution?.[0]).toBeLessThan(3840);
  expect(renderer.completeness.complete).toBe(true);
});

test("ordinary meshes reuse their immutable stream while relief alone pays for exact reference vertices", () => {
  const ordinary = harness({ antialiasing: "spatial" });
  ordinary.renderer.render(scene);
  const referenceMesh = {
    ...mesh,
    reliefCoordinates: mesh.positions.slice(),
    reliefNormals: mesh.normals.slice(),
  };
  const relief = harness({ antialiasing: "spatial" });
  relief.renderer.render({ ...scene, surfaces: [{ ...surface, mesh: referenceMesh }] });
  const streamBytes = (mesh.positions.length / 3) * VERTEX_FLOATS * 4;
  const sourceBytes = (mesh.positions.length / 3) * RELIEF_VERTEX_FLOATS * 4;
  expect(VERTEX_FLOATS).toBe(23);
  expect(ordinary.buffers.filter(({ label }) => label.startsWith("Relief source"))).toHaveLength(0);
  expect(ordinary.buffers.find(({ label }) => label.startsWith("Vertices"))?.size).toBe(streamBytes);
  expect(relief.buffers.filter(({ label }) => label.startsWith("Relief source"))).toHaveLength(1);
  expect(relief.renderer.measurements.gpuBytes - ordinary.renderer.measurements.gpuBytes).toBe(sourceBytes);
  expect(
    ordinary.bindings.filter(({ slot }) => slot === 1).every(({ label }) => label === "Vertices required"),
  ).toBe(true);
  expect(
    relief.bindings
      .filter(({ slot }) => slot === 1)
      .every(({ label }) => label === "Relief source vertices required"),
  ).toBe(true);
  expect(relief.buffers.find(({ label }) => label.startsWith("Relief source"))?.size).toBe(sourceBytes);
  expect(ordinary.bindings.filter(({ slot }) => slot === 1).every(({ stride }) => stride === 92)).toBe(true);
  const sourceBindings = relief.bindings.filter(({ slot }) => slot === 1);
  expect(sourceBindings.every(({ stride }) => stride === 24)).toBe(true);
  expect(sourceBindings.map(({ pipeline }) => pipeline)).toEqual([
    "Shadow / compact relief",
    "Color 1:false / compact relief",
  ]);
  expect(relief.renderer.completeness.complete).toBe(true);
  relief.renderer.dispose();
  expect(relief.buffers.find(({ label }) => label === "Relief source vertices required")?.destroyed).toBe(
    true,
  );
  ordinary.renderer.dispose();
});

test("relief reference streams obey incremental upload budgets and are released on eviction", () => {
  const referenceMesh = {
    ...mesh,
    reliefCoordinates: mesh.positions.slice(),
    reliefNormals: mesh.normals.slice(),
  };
  const reliefScene = { ...scene, surfaces: [{ ...surface, mesh: referenceMesh }] };
  const { renderer, buffers, writes } = harness({ maxUploadBytesPerFrame: 12, antialiasing: "spatial" });
  const streamBytes = (mesh.positions.length / 3) * VERTEX_FLOATS * 4;
  const sourceBytes = (mesh.positions.length / 3) * RELIEF_VERTEX_FLOATS * 4;
  const geometryBytes = streamBytes + sourceBytes + mesh.indices.byteLength;
  for (let frame = 0; frame < Math.ceil(geometryBytes / 12); frame++) {
    const before = writes.length;
    renderer.render(reliefScene);
    const uploaded = writes
      .slice(before)
      .filter(
        ({ label }) =>
          label.startsWith("Vertices") || label.startsWith("Indices") || label.startsWith("Relief source"),
      );
    expect(uploaded.reduce((sum, write) => sum + write.bytes, 0)).toBeLessThanOrEqual(12);
    if (frame < Math.ceil(geometryBytes / 12) - 1) expect(renderer.completeness.complete).toBe(false);
  }
  expect(renderer.completeness.complete).toBe(true);
  const sourceWrites = writes.filter(({ label }) => label.startsWith("Relief source"));
  expect(sourceWrites.reduce((sum, write) => sum + write.bytes, 0)).toBe(sourceBytes);
  for (let i = 1; i < sourceWrites.length; i++)
    expect(sourceWrites[i].offset).toBe(sourceWrites[i - 1].offset + sourceWrites[i - 1].bytes);
  const internals = renderer as unknown as { frame: number; evict(bytes: number): boolean };
  internals.frame++;
  internals.evict(QUALITY_PROFILES.balanced.maxGpuBytes);
  expect(buffers.find(({ label }) => label === "Relief source vertices required")?.destroyed).toBe(true);
  expect(buffers.find(({ label }) => label === "Vertices required")?.destroyed).toBe(true);
  renderer.dispose();
});

test("mixed relief draws select compact layouts in shadow, thin depth, color and water passes", () => {
  const reliefMesh = {
    ...mesh,
    reliefCoordinates: mesh.positions.slice(),
    reliefNormals: mesh.normals.slice(),
  };
  const coveredMesh = {
    ...reliefMesh,
    thinCoverage: {
      version: 1 as const,
      key: "relief-thin",
      width: 1,
      height: 1,
      uv: new Float32Array(8),
      levels: [new Uint8Array([255])],
    },
  };
  const { renderer, bindings } = harness({ antialiasing: "spatial" });
  renderer.render({
    ...scene,
    surfaces: [
      surface,
      { ...surface, id: "relief", source: "relief", mesh: reliefMesh },
      { ...surface, id: "thin", source: "thin", mesh: coveredMesh },
      {
        ...surface,
        id: "water",
        source: "water",
        mesh: reliefMesh,
        water: {
          id: "water",
          name: "Water",
          schemaVersion: 1,
          kind: "water",
          dependencies: [],
          level: 0,
          color: [0, 0.2, 0.3],
          roughness: 0.2,
          waves: [],
        },
      },
    ],
  });
  expect(renderer.completeness.complete).toBe(true);
  const sourceBindings = bindings.filter(({ slot }) => slot === 1);
  for (const binding of sourceBindings)
    expect(binding.stride).toBe(binding.label.startsWith("Relief source") ? 24 : 92);
  const pipelines = sourceBindings.map(({ pipeline }) => pipeline);
  expect(pipelines).toContain("Shadow");
  expect(pipelines).toContain("Shadow / compact relief");
  expect(pipelines).toContain("Thin depth 1 / compact relief");
  expect(pipelines).toContain("Thin color 1:false / compact relief");
  expect(pipelines).toContain("Color 1:false");
  expect(pipelines).toContain("Color 1:false / compact relief");
  expect(pipelines).toContain("Water 1:false:false / compact relief");
  renderer.dispose();
});

test("live deformation keeps compact rest data immutable while updating current and previous positions", () => {
  const input = {
    ...surface,
    mesh: { ...mesh, reliefCoordinates: mesh.positions.slice(), reliefNormals: mesh.normals.slice() },
    deformation: {
      revision: "pose-1",
      vertexIndices: new Uint32Array([1]),
      positionDeltas: new Float32Array([0, 0, 0.1]),
      maxDisplacement: 0.11,
    },
  };
  const { renderer, bindings, writes } = harness({ antialiasing: "spatial" });
  renderer.render({ ...scene, surfaces: [input] });
  renderer.render({
    ...scene,
    time: 1 / 60,
    surfaces: [
      {
        ...input,
        deformation: {
          ...input.deformation,
          revision: "pose-2",
          positionDeltas: new Float32Array([0, 0, 0.05]),
        },
      },
    ],
  });
  expect(renderer.completeness.complete).toBe(true);
  expect(
    bindings.filter(({ slot }) => slot === 0).every(({ label }) => label.startsWith("Creature deformation")),
  ).toBe(true);
  expect(
    bindings
      .filter(({ slot }) => slot === 1)
      .every(({ label, stride }) => label === "Relief source vertices required" && stride === 24),
  ).toBe(true);
  expect(
    bindings
      .filter(({ slot }) => slot === 2)
      .every(({ label }) => label.startsWith("Previous creature deformation")),
  ).toBe(true);
  const sourceWrites = writes.filter(({ label }) => label.startsWith("Relief source"));
  expect(sourceWrites).toHaveLength(1);
  expect(sourceWrites[0].bytes).toBe(4 * 24);
  renderer.dispose();
});

test("a relief mesh fits the exact compact residency allowance without reserving a duplicate full stream", () => {
  const ordinary = harness({ antialiasing: "spatial" });
  // Diagnostic pixels stay fixed so automatic resolution budgeting cannot hide an over-allocation.
  const fixedScene: EvaluatedScene = { ...scene, mode: "normals" };
  ordinary.renderer.render(fixedScene);
  const allowance = ordinary.renderer.measurements.gpuBytes + (mesh.positions.length / 3) * 24;
  const compact = harness({ antialiasing: "spatial", maxGpuBytes: allowance });
  compact.renderer.render({
    ...fixedScene,
    surfaces: [
      {
        ...surface,
        mesh: {
          ...mesh,
          reliefCoordinates: mesh.positions.slice(),
          reliefNormals: mesh.normals.slice(),
        },
      },
    ],
  });
  expect(compact.renderer.completeness.complete).toBe(true);
  expect(compact.renderer.measurements.gpuBytes).toBe(allowance);
  expect(
    Number(compact.renderer.measurements.bufferBytes) - Number(ordinary.renderer.measurements.bufferBytes),
  ).toBe(4 * 24);
  compact.renderer.dispose();
  ordinary.renderer.dispose();
});

test("pending coverage uploads stay retryable and complete when each frame cannot fit a whole mip chain", () => {
  const coverage = {
    version: 1 as const,
    key: "streamed-leaf",
    width: 8,
    height: 4,
    uv: new Float32Array(8),
    levels: [
      new Uint8Array(32).fill(255),
      new Uint8Array(8).fill(128),
      new Uint8Array(2).fill(64),
      new Uint8Array(1).fill(32),
    ],
  };
  const covered = { ...scene, surfaces: [{ ...surface, mesh: { ...mesh, thinCoverage: coverage } }] };
  const { renderer, buffers, writes, textureWrites } = harness({
    maxUploadBytesPerFrame: 16,
    antialiasing: "spatial",
  });
  const expectedBytes = (mesh.positions.length / 3) * VERTEX_FLOATS * 4 + mesh.indices.byteLength + 43;
  let pendingTextureObserved = false;
  for (let frame = 0; frame < Math.ceil(expectedBytes / 16); frame++) {
    const beforeBuffers = writes.length,
      beforeTextures = textureWrites.length;
    renderer.render(covered);
    expect(renderer.completeness.rejected).toEqual([]);
    const bufferBytes = writes
      .slice(beforeBuffers)
      .filter(({ label }) => label.startsWith("Vertices") || label.startsWith("Indices"))
      .reduce((sum, write) => sum + write.bytes, 0);
    const textureBytes = textureWrites.slice(beforeTextures).reduce((sum, bytes) => sum + bytes, 0);
    expect(bufferBytes + textureBytes).toBeLessThanOrEqual(16);
    if (!renderer.completeness.complete && buffers.some(({ label }) => label === "Surface required")) {
      pendingTextureObserved = true;
      expect(renderer.completeness.uploading).toEqual(["required"]);
      expect(renderer.needsRender).toBe(true);
    }
  }
  expect(pendingTextureObserved).toBe(true);
  expect(renderer.completeness.complete).toBe(true);
  expect(renderer.needsRender).toBe(false);
  expect(textureWrites.reduce((sum, bytes) => sum + bytes, 0)).toBe(43);
  renderer.dispose();
});
