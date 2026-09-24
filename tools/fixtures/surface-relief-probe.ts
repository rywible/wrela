import { surfaceReliefBands, surfaceReliefDepth, surfaceReliefSlopeVariance } from "@wrela/compiler";
import {
  identityMatrix,
  normalize,
  type RenderSurface,
  type SurfaceReliefAppearance,
  surfaceReliefSchema,
  type Vec3,
} from "@wrela/model";

import { packSurface, RELIEF_APPEARANCE_OFFSET } from "@wrela/render-webgpu/packing";
import reliefWGSL from "@wrela/render-webgpu/surface-relief.wgsl" with { type: "text" };

export type ReliefProbeCase = {
  kind: string;
  seed: number;
  scale: number;
  allocation: string;
  position: Vec3;
  normal: Vec3;
  footprint: number;
  appearance: SurfaceReliefAppearance;
  packed: Float32Array;
};
export function surfaceReliefProbeCases(): ReliefProbeCase[] {
  const cases: ReliefProbeCase[] = [];
  const sites: { position: Vec3; normal: Vec3 }[] = [
    { position: [-0.137, 0.089, -0.222], normal: normalize([1, 0.2, -0.3]) },
    { position: [0.071, -0.193, 0.117], normal: normalize([-0.2, 0.1, 1]) },
    { position: [-0.03, -0.09, -0.21], normal: [0, 1, 0] },
    { position: [0.219, 0.133, -0.071], normal: normalize([0.4, -0.6, 0.5]) },
  ];
  for (const kind of ["bark", "stone"] as const)
    for (const seed of [-2147483648, -173, -1, 0, 37, 2147483647])
      for (const size of [0.015, 0.07, 0.25]) {
        const recipe = surfaceReliefSchema.parse({
          kind,
          amplitude: kind === "bark" ? 0.022 : 0.014,
          scale: size,
          seed,
          targetEdgeLength: 0.022,
          direction: normalize([0.12, 1, 0.07]),
        });
        const slopeVariance = surfaceReliefSlopeVariance(recipe);
        for (const [allocation, geometryWeights] of [
          ["near", [1, 0.5, 0]],
          ["coarse", [0, 0, 0]],
          ["complete", [1, 1, 1]],
        ] as [string, Vec3][]) {
          const appearance: SurfaceReliefAppearance = {
            recipe,
            geometryWeights,
            residualWeights: geometryWeights.map((value) => 1 - value) as Vec3,
            slopeVariance,
          };
          const surface: RenderSurface = {
            id: "probe",
            source: "probe",
            mesh: {
              positions: new Float32Array(),
              normals: new Float32Array(),
              indices: new Uint32Array(),
              bounds: { min: [0, 0, 0], max: [0, 0, 0] },
            },
            matrix: identityMatrix(),
            material: {
              color: [0.2, 0.2, 0.2],
              secondary: [0.2, 0.2, 0.2],
              roughness: 0.5,
              metallic: 0,
              pattern: 0,
              scale: 1,
              normalStrength: 0,
            },
            reliefAppearance: appearance,
          };
          const packed = packSurface(surface).slice(RELIEF_APPEARANCE_OFFSET, RELIEF_APPEARANCE_OFFSET + 20);
          for (const site of sites)
            for (const footprint of [0, size * 0.03, size * 0.3, size * 2])
              cases.push({ kind, seed, scale: size, allocation, ...site, footprint, appearance, packed });
        }
      }
  return cases;
}

export function surfaceReliefProbeReference(sample: ReliefProbeCase): {
  bands: number[];
  resolved: number[];
} {
  const { recipe, geometryWeights, residualWeights, slopeVariance } = sample.appearance;
  const { bands, mean } = surfaceReliefBands(sample.position, sample.normal, recipe);
  const frequencies = recipe.kind === "stone" ? [1, 2.7, 7.1] : [1, 3, 8];
  const visibility = frequencies.map((frequency) => {
    const t = Math.max(0, Math.min(1, ((frequency * sample.footprint) / recipe.scale - 0.2) / 0.45));
    return 1 - t * t * (3 - 2 * t);
  });
  const geometry = surfaceReliefDepth(sample.position, sample.normal, recipe, geometryWeights);
  const resolved = surfaceReliefDepth(
    sample.position,
    sample.normal,
    recipe,
    geometryWeights.map((value, index) => value + residualWeights[index] * visibility[index]) as Vec3,
  );
  const variance = slopeVariance.reduce(
    (sum, value, index) => sum + value * residualWeights[index] ** 2 * (1 - visibility[index] ** 2),
    0,
  );
  return {
    bands: [...bands, mean],
    resolved: [
      -recipe.amplitude * (resolved - geometry),
      variance,
      geometry,
      surfaceReliefDepth(sample.position, sample.normal, recipe),
    ],
  };
}

/** Executes the production WGSL and production uniform packing without sky, tone mapping, or screenshots. */
export async function probeSurfaceRelief(device: GPUDevice) {
  const samples = surfaceReliefProbeCases(),
    values = new Float32Array(samples.length * 32);
  for (const [index, sample] of samples.entries()) {
    const offset = index * 32;
    values.set(sample.packed, offset);
    values.set(sample.position, offset + 20);
    values.set(sample.normal, offset + 24);
    values[offset + 28] = sample.footprint;
  }
  const outputBytes = samples.length * 8 * 4;
  const input = device.createBuffer({
    size: values.byteLength,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
  const output = device.createBuffer({
    size: outputBytes,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const readback = device.createBuffer({
    size: outputBytes,
    usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
  });
  try {
    device.queue.writeBuffer(input, 0, values);
    const module = device.createShaderModule({
      code: `
struct ReliefObject { relief:vec4f, reliefDirection:vec4f, reliefGeometry:vec4f, reliefResidual:vec4f, reliefSlopeVariance:vec4f };
struct Sample { object:ReliefObject, position:vec4f, normal:vec4f, footprint:vec4f };
struct Result { bands:vec4f, resolved:vec4f };
var<private> obj:ReliefObject;
@group(0) @binding(0) var<storage,read> samples:array<Sample>;
@group(0) @binding(1) var<storage,read_write> results:array<Result>;
${reliefWGSL}
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {
  if(id.x>=arrayLength(&samples)){return;}
  let sample=samples[id.x];obj=sample.object;
  let bands=reliefBands(sample.position.xyz,normalize(sample.normal.xyz));
  let residual=authoredReliefResidual(sample.position.xyz,sample.normal.xyz,sample.footprint.x);
  let geometry=clamp(bands.w+dot(bands.xyz,obj.reliefGeometry.xyz),0.0,1.0);
  let complete=clamp(bands.w+dot(bands.xyz,vec3f(1.0)),0.0,1.0);
  results[id.x]=Result(bands,vec4f(residual,geometry,complete));
}`,
    });
    const messages = await module.getCompilationInfo();
    const errors = messages.messages
      .filter((message) => message.type === "error")
      .map((message) => message.message);
    if (errors.length) throw new Error(errors.join("\n"));
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "main" },
    });
    const bind = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: input } },
        { binding: 1, resource: { buffer: output } },
      ],
    });
    const encoder = device.createCommandEncoder(),
      pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bind);
    pass.dispatchWorkgroups(Math.ceil(samples.length / 64));
    pass.end();
    encoder.copyBufferToBuffer(output, 0, readback, 0, outputBytes);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const actual = new Float32Array(readback.getMappedRange().slice(0));
    readback.unmap();
    let maximumBandAbsoluteError = 0,
      maximumResidualMetresError = 0,
      maximumVarianceAbsoluteError = 0,
      maximumInvisibleResidual = 0,
      failures = 0;
    const examples: unknown[] = [];
    for (const [index, sample] of samples.entries()) {
      const expected = surfaceReliefProbeReference(sample),
        reference = [...expected.bands, ...expected.resolved];
      for (let field = 0; field < 8; field++) {
        const value = actual[index * 8 + field],
          error = Math.abs(value - reference[field]);
        const tolerance =
          field === 4 ? 0.00003 : field === 5 ? 0.0001 + Math.abs(reference[field]) * 0.00002 : 0.001;
        if (!Number.isFinite(value) || error > tolerance) {
          failures++;
          if (examples.length < 12)
            examples.push({
              index,
              kind: sample.kind,
              seed: sample.seed,
              scale: sample.scale,
              allocation: sample.allocation,
              position: sample.position,
              normal: sample.normal,
              footprint: sample.footprint,
              field,
              actual: value,
              expected: reference[field],
              error,
              tolerance,
            });
        }
        if (field < 4) maximumBandAbsoluteError = Math.max(maximumBandAbsoluteError, error);
        if (field === 4) maximumResidualMetresError = Math.max(maximumResidualMetresError, error);
        if (field === 5) maximumVarianceAbsoluteError = Math.max(maximumVarianceAbsoluteError, error);
      }
      if (sample.footprint >= sample.scale)
        maximumInvisibleResidual = Math.max(maximumInvisibleResidual, Math.abs(actual[index * 8 + 4]));
    }
    return {
      samples: samples.length,
      failures,
      examples,
      maximumBandAbsoluteError,
      maximumResidualMetresError,
      maximumVarianceAbsoluteError,
      maximumInvisibleResidual,
      scope:
        "Production packing + WGSL bands, signed residual height, pixel filtering and nominal slope variance. Does not measure geometric/radiance error or certify BRDF energy conservation.",
    };
  } finally {
    input.destroy();
    output.destroy();
    readback.destroy();
  }
}

export async function surfaceReliefProbeFixture() {
  const adapter = await navigator.gpu?.requestAdapter();
  if (!adapter) throw new Error("WebGPU adapter unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (event) =>
    errors.push((event as GPUUncapturedErrorEvent).error.message),
  );
  return {
    async check() {
      return { ...(await probeSurfaceRelief(device)), adapter: adapter.info, errors };
    },
    dispose() {
      device.destroy();
    },
  };
}
