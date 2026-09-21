import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type RenderSurface } from "@wrela/model";
import {
  GLOBAL_FLOATS,
  IncompleteRenderError,
  QUALITY_PROFILES,
  type RendererOptions,
  WebGPURenderer,
} from "./index";

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
    GPUBufferUsage: { VERTEX: 1, INDEX: 2, COPY_DST: 4, UNIFORM: 8, STORAGE: 16 },
    GPUTextureUsage: { RENDER_ATTACHMENT: 1, TEXTURE_BINDING: 2 },
  });
  const buffers: { label: string; size: number }[] = [];
  const draws: { count: number; first: number }[] = [];
  const texture = () => ({ createView: () => ({}), destroy: () => {} });
  const pass = () => ({
    setPipeline: () => {},
    setBindGroup: () => {},
    setVertexBuffer: () => {},
    setIndexBuffer: () => {},
    draw: () => {},
    drawIndexed: (count: number, _instances: number, first: number) => {
      draws.push({ count, first });
    },
    end: () => {},
  });
  const device = {
    limits: { maxTextureDimension2D: 4096 },
    queue: { writeBuffer: () => {}, submit: () => {} },
    createTexture: texture,
    createSampler: () => ({}),
    createBindGroup: () => ({}),
    createBuffer: (descriptor: { label: string; size: number }) => {
      buffers.push(descriptor);
      return { destroy: () => {} };
    },
    createCommandEncoder: () => ({ beginRenderPass: pass, finish: () => ({}) }),
  };
  const Constructor = WebGPURenderer as unknown as new (
    canvas: HTMLCanvasElement,
    options: RendererOptions,
  ) => WebGPURenderer;
  const renderer = new Constructor({ clientWidth: 64, clientHeight: 64 } as HTMLCanvasElement, {
    pixelRatio: 1,
    ...options,
  });
  Object.assign(renderer, {
    device,
    context: { getCurrentTexture: texture },
    shadowTexture: texture(),
    skyPipelines: new Map([
      [1, {}],
      [4, {}],
    ]),
    main: new Map([
      ["1:false", {}],
      ["1:true", {}],
      ["4:false", {}],
      ["4:true", {}],
    ]),
    status: "ready",
    bytes: QUALITY_PROFILES[options.quality ?? "balanced"].shadowSize ** 2 * 4 + GLOBAL_FLOATS * 4,
  });
  return { renderer, buffers, draws };
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
  for (let frame = 0; frame < 100 && renderer.needsRender; frame++) renderer.render(scene);
  expect(renderer.completeness.complete).toBe(true);
  expect(renderer.completeness.rendered).toEqual(["required"]);
  expect(renderer.completeness.rejected).toEqual([]);
});
test("material ranges allocate shared vertices once and submit their actual index ranges", () => {
  const { renderer, buffers, draws } = harness();
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
  const { renderer, buffers } = harness();
  const matrix = identityMatrix();
  matrix[12] = 10000;
  renderer.render({ ...scene, surfaces: [{ ...surface, matrix }] });
  expect(renderer.completeness.culled).toEqual(["required"]);
  expect(renderer.completeness.complete).toBe(true);
  expect(buffers).toHaveLength(0);
});
test("out-of-bounds material draw ranges are rejected before buffer allocation", () => {
  const { renderer, buffers } = harness();
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
