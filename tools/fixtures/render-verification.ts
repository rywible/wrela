import { compileDocument, queryWater } from "@wrela/compiler";
import {
  type EvaluatedScene,
  identityMatrix,
  type RenderMaterial,
  type RenderSurface,
  referenceProject,
  transformMatrix,
  type Vec3,
  type WaterDefinition,
  waterGeometryAttenuation,
  waterGeometryErrorBound,
} from "@wrela/model";
import {
  GLOBAL_FLOATS,
  IncompleteRenderError,
  OBJECT_FLOATS,
  packSurface,
  WebGPURenderer,
} from "@wrela/render-webgpu";
import { verifyProjectStorage } from "../../packages/authoring/src/storage.browser";
import { shader } from "../../packages/render-webgpu/src/shader";

const target = window as unknown as {
  ready?: boolean;
  failure?: string;
  fixture?: Awaited<ReturnType<typeof setup>>;
};
const errors: string[] = [];
window.addEventListener("error", (event) => errors.push(event.message));
window.addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));

/** Verification-only access: exercise real WGSL on the renderer's actual device. */
function deviceOf(renderer: WebGPURenderer) {
  return (renderer as unknown as { device: GPUDevice }).device;
}
async function digest(renderer: WebGPURenderer) {
  return Array.from(
    new Uint8Array(await crypto.subtle.digest("SHA-256", await (await renderer.capture()).arrayBuffer())),
  ).join(",");
}
async function pixels(renderer: WebGPURenderer) {
  const bitmap = await createImageBitmap(await renderer.capture());
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) throw Error("Pixel comparison context unavailable");
    context.drawImage(bitmap, 0, 0);
    return context.getImageData(0, 0, canvas.width, canvas.height).data;
  } finally {
    bitmap.close();
  }
}
function difference(a: Uint8ClampedArray, b: Uint8ClampedArray) {
  if (a.length !== b.length) throw Error("Capture dimensions changed during comparison");
  let total = 0,
    max = 0,
    changedChannels = 0,
    minimum = 255,
    maximum = 0;
  for (let index = 0; index < a.length; index++) {
    if (index % 4 === 3) continue;
    const delta = Math.abs(a[index] - b[index]);
    total += delta;
    max = Math.max(max, delta);
    if (delta > 2) changedChannels++;
    minimum = Math.min(minimum, a[index]);
    maximum = Math.max(maximum, a[index]);
  }
  return { mean: total / (a.length * 0.75), max, changedChannels, sourceRange: maximum - minimum };
}
function isolatedCanvas(size: number) {
  const canvas = document.createElement("canvas");
  canvas.style.cssText = `position:fixed;left:-10000px;top:0;width:${size}px;height:${size}px`;
  document.body.append(canvas);
  return canvas;
}
async function waterParity(
  renderer: WebGPURenderer,
  surface: RenderSurface,
  water: WaterDefinition,
  spacing = 0,
) {
  const device = deviceOf(renderer),
    count = 64,
    points = new Float32Array(count * 4);
  for (let index = 0; index < count; index++)
    points.set([(index % 8) * 2.7 - 9, 0, Math.floor(index / 8) * 1.9 - 6, 1], index * 4);
  const code = `${shader}
@group(2) @binding(0) var<storage,read> points:array<vec4f>;
@group(2) @binding(1) var<storage,read_write> answers:array<vec4f>;
@compute @workgroup_size(64) fn verifyWater(@builtin(global_invocation_id) id:vec3u) {
  let i=id.x; var v:Vertex; v.position=points[i].xyz; v.normal=vec3f(0,1,0);
  v.color=vec3f(1); v.joints=vec4f(0); v.weights=vec4f(1,0,0,0);
  let r=deform(v,obj.model); answers[i*2]=vec4f(r.world,1); answers[i*2+1]=vec4f(r.normal,0);
}`;
  const pipeline = await device.createComputePipelineAsync({
    layout: "auto",
    compute: { module: device.createShaderModule({ code }), entryPoint: "verifyWater" },
  });
  const make = (size: number, usage: GPUBufferUsageFlags) => device.createBuffer({ size, usage });
  const global = make(GLOBAL_FLOATS * 4, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST),
    uniform = make(OBJECT_FLOATS * 4, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST),
    skin = make(4096, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST),
    input = make(points.byteLength, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST),
    result = make(count * 32, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC),
    read = make(count * 32, GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST);
  try {
    const globals = new Float32Array(GLOBAL_FLOATS);
    globals[60] = 12.4;
    device.queue.writeBuffer(global, 0, globals);
    device.queue.writeBuffer(
      uniform,
      0,
      packSurface({
        ...surface,
        water,
        waterApproximation: { spacing, maxHeightError: waterGeometryErrorBound(water.waves, spacing) },
      }) as Float32Array<ArrayBuffer>,
    );
    device.queue.writeBuffer(skin, 0, identityMatrix() as Float32Array<ArrayBuffer>);
    device.queue.writeBuffer(input, 0, points);
    const encoder = device.createCommandEncoder(),
      pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    for (const [index, resources] of [[global], [uniform, skin], [input, result]].entries())
      pass.setBindGroup(
        index,
        device.createBindGroup({
          layout: pipeline.getBindGroupLayout(index),
          entries: resources.map((buffer, binding) => ({ binding, resource: { buffer } })),
        }),
      );
    pass.dispatchWorkgroups(1);
    pass.end();
    encoder.copyBufferToBuffer(result, 0, read, 0, count * 32);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const values = new Float32Array(read.getMappedRange());
    let maxError = 0;
    for (let index = 0; index < count; index++) {
      const sample = queryWater(
        {
          ...water,
          waves: water.waves.map((wave) => ({
            ...wave,
            amplitude: wave.amplitude * waterGeometryAttenuation(wave.wavelength, spacing),
          })),
        },
        points[index * 4],
        points[index * 4 + 2],
        12.4,
      );
      maxError = Math.max(
        maxError,
        Math.abs(sample.height - values[index * 8 + 1]),
        ...sample.normal.map((normal, axis) => Math.abs(normal - values[index * 8 + 4 + axis])),
      );
    }
    read.unmap();
    if (maxError > 0.0001) throw Error(`CPU/GPU water disagreement ${maxError}`);
    return {
      samples: count,
      maxError,
      ...(spacing ? { spacing, maxHeightError: waterGeometryErrorBound(water.waves, spacing) } : {}),
    };
  } finally {
    for (const buffer of [global, uniform, skin, input, result, read]) buffer.destroy();
  }
}
async function setup() {
  const project = referenceProject(),
    bunny = project.documents.find((document) => document.kind === "character"),
    water = project.documents.find((document) => document.kind === "water");
  if (!bunny || !water) throw Error("Reference character or water is missing");
  const artifact = compileDocument(bunny, "interactive");
  if (!artifact || artifact.kind !== "character")
    throw Error("Reference character compiled to wrong product");
  const material: RenderMaterial = {
    color: [0.88, 0.92, 0.92],
    secondary: [0.65, 0.74, 0.78],
    roughness: 0.83,
    metallic: 0,
    pattern: 1,
    scale: 18,
    normalStrength: 0.12,
  };
  const surface: RenderSurface = {
    id: "bunny",
    source: bunny.id,
    mesh: artifact.mesh,
    matrix: identityMatrix(),
    material,
  };
  const ground: RenderSurface = {
    id: "ground",
    source: "ground",
    mesh: {
      positions: new Float32Array([-12, 0, -12, 12, 0, -12, 12, 0, 12, -12, 0, 12]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0]),
      indices: new Uint32Array([0, 2, 1, 0, 3, 2]),
      bounds: { min: [-12, 0, -12], max: [12, 0, 12] },
    },
    matrix: identityMatrix(),
    material: { ...material, color: [0.16, 0.22, 0.2], secondary: [0.1, 0.14, 0.12], scale: 0.8 },
  };
  const scene: EvaluatedScene = {
    surfaces: [ground, surface],
    camera: { position: [5, 3.2, 7], target: [0, 1.2, 0], fov: 42 },
    environment: {
      sunDirection: [0.4, 0.8, 0.3],
      sunColor: [1, 0.88, 0.68],
      sunIntensity: 3,
      ambient: 0.55,
      skyColor: [0.2, 0.4, 0.63],
      horizonColor: [0.65, 0.75, 0.78],
      groundColor: [0.18, 0.23, 0.21],
      fogDensity: 0.002,
      wind: [0.2, 0, 0.1],
      exposure: 1,
    },
    time: 0,
    mode: "beauty",
    grid: false,
  };
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Verification canvas is missing");
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") console.error(diagnostic.message);
    },
  });
  const fixture = {
    renderer,
    scene,
    errors,
    storage: verifyProjectStorage,
    async shaderDiagnostics() {
      const device = deviceOf(renderer);
      device.pushErrorScope("validation");
      const module = device.createShaderModule({
        code: "@vertex fn invalid() -> @builtin(position) vec4f {\nreturn missing_symbol;\n}",
      });
      const info = await module.getCompilationInfo();
      await device.popErrorScope();
      return info.messages
        .filter((message) => message.type === "error")
        .map((message) => ({
          line: message.lineNum,
          column: message.linePos,
          message: message.message,
        }));
    },
    waterParity: () => waterParity(renderer, surface, water),
    waterApproximation: () => waterParity(renderer, surface, water, 2.5),
    async instancing() {
      const originals = scene.surfaces,
        mode = scene.mode;
      try {
        const instances = [-2, 0, 2].map((x, index) => ({
          ...surface,
          id: `bunny-${index}`,
          instanceId: `actor-${index}`,
          matrix: transformMatrix([x, 0, 0]),
        }));
        scene.mode = "identity";
        scene.surfaces = [ground, ...instances];
        renderer.render(scene);
        const batched = await digest(renderer),
          draws = renderer.measurements.drawCalls;
        scene.surfaces = [
          ground,
          ...instances.map((instance) => ({ ...instance, mesh: { ...instance.mesh } })),
        ];
        renderer.render(scene);
        const separate = await digest(renderer),
          separateDraws = renderer.measurements.drawCalls;
        if (batched !== separate || draws >= separateDraws)
          throw Error("Instancing changed per-instance pixels or failed to reduce draws");
        return { batchedDraws: draws, separateDraws, pixelIdentical: true };
      } finally {
        scene.surfaces = originals;
        scene.mode = mode;
        renderer.render(scene);
      }
    },
    async pointLighting() {
      const previous = scene.environment.pointLights;
      try {
        scene.mode = "beauty";
        renderer.render(scene);
        const before = await digest(renderer);
        scene.environment.pointLights = [{ position: [0, 2, 3], color: [1, 0.05, 0.02], intensity: 15 }];
        renderer.render(scene);
        return before !== (await digest(renderer));
      } finally {
        scene.environment.pointLights = previous;
        renderer.render(scene);
      }
    },
    async budgetRejection() {
      const tinyCanvas = isolatedCanvas(64);
      const constrained = await WebGPURenderer.create(tinyCanvas, {
        pixelRatio: 1,
        quality: "low",
        maxGpuBytes: 1,
      });
      try {
        constrained.render({ ...scene, mode: "identity", surfaces: [surface] });
        let rejection: IncompleteRenderError | undefined;
        try {
          await constrained.capture();
        } catch (error) {
          if (error instanceof IncompleteRenderError) rejection = error;
          else throw error;
        }
        if (
          !rejection ||
          !rejection.completeness.rejected.some((entry) => entry.id === surface.id) ||
          rejection.completeness.complete ||
          constrained.needsRender
        )
          throw Error("Budget-refused geometry was not reported as an explicit incomplete capture");
        const diagnosticErrors = constrained.diagnostics.filter(
          (diagnostic) => diagnostic.severity === "error",
        );
        if (diagnosticErrors.length)
          throw Error(diagnosticErrors.map((diagnostic) => diagnostic.message).join("\n"));
        return {
          captureRejected: true,
          maxGpuBytes: 1,
          completeness: structuredClone(rejection.completeness),
        };
      } finally {
        constrained.dispose();
        tinyCanvas.remove();
      }
    },
    async materialRebase() {
      const rebaseCanvas = isolatedCanvas(256);
      const isolated = await WebGPURenderer.create(rebaseCanvas, { pixelRatio: 1, quality: "low" });
      try {
        const plane: RenderSurface = {
          ...ground,
          id: "world-domain-plane",
          matrix: transformMatrix([0, 0, 0], 8),
          material: {
            ...material,
            domain: "world",
            scale: 0.125,
            normalStrength: 0,
            color: [0.05, 0.1, 0.2],
            secondary: [0.85, 0.65, 0.3],
          },
        };
        const before: EvaluatedScene = {
          ...scene,
          surfaces: [plane],
          mode: "albedo",
          origin: [0, 0, 0],
          camera: { position: [0, 12, 12], target: [0, 0, 0], fov: 42 },
        };
        isolated.render(before);
        const initial = await pixels(isolated);
        const offset: Vec3 = [256, 0, 256];
        const shift = (position: Vec3): Vec3 => position.map((value, axis) => value - offset[axis]) as Vec3;
        const after: EvaluatedScene = {
          ...before,
          origin: offset,
          surfaces: [{ ...plane, matrix: transformMatrix(shift([0, 0, 0]), 8) }],
          camera: {
            ...before.camera,
            position: shift(before.camera.position),
            target: shift(before.camera.target),
          },
        };
        isolated.render(after);
        const rebased = difference(initial, await pixels(isolated));
        isolated.render({ ...after, origin: [0, 0, 0] });
        const uncompensated = difference(initial, await pixels(isolated));
        // Diagnostics bypass antialiasing/display transfer. Allow sub-byte rounding
        // at backend-dependent interpolated positions; the negative control must change visibly.
        if (
          rebased.sourceRange < 10 ||
          rebased.mean > 0.05 ||
          rebased.max > 8 ||
          rebased.changedChannels > initial.length * 0.005 ||
          uncompensated.mean < 1
        )
          throw Error(
            `World material changed across equivalent origin rebase: ${JSON.stringify({ rebased, uncompensated })}`,
          );
        return { originDelta: offset, channel: "albedo", rebased, uncompensated, stable: true };
      } finally {
        isolated.dispose();
        rebaseCanvas.remove();
      }
    },
    capture: () => renderer.capture(),
    lose: () => deviceOf(renderer).destroy(),
  };
  renderer.render(scene);
  return fixture;
}
export type RenderVerificationFixture = Awaited<ReturnType<typeof setup>>;
void setup()
  .then((fixture) => {
    target.fixture = fixture;
    target.ready = true;
  })
  .catch((error) => {
    target.failure = String(error);
    errors.push(String(error));
  });
