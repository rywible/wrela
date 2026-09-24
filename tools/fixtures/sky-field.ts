import {
  defaultPhysicalAtmosphere,
  PHYSICAL_AERIAL_SIZE,
  PHYSICAL_ATMOSPHERE_FRAME_FLOATS,
  PHYSICAL_CLOUD_LIGHT_SIZE,
  PHYSICAL_CLOUD_NOISE_SIZE,
  PHYSICAL_CLOUD_VIEW_SIZE,
  PHYSICAL_DIFFUSE_SIZE,
  PHYSICAL_SKY_SIZE,
  physicalAtmosphereComputeWGSL,
  physicalCloudComputeWGSL,
  physicalCloudLightComputeWGSL,
  physicalCloudNoiseComputeWGSL,
  physicalCloudTemporalComputeWGSL,
  physicalMultipleScatteringComputeWGSL,
} from "@wrela/render-webgpu/atmosphere";
import cloudTransportWGSL from "@wrela/render-webgpu/atmosphere-cloud-transport.wgsl" with { type: "text" };
import { packCloudFormations } from "@wrela/render-webgpu/cloud-formations";
import { canReuseCloudHistory, cloudHistoryMaxAge } from "@wrela/render-webgpu/cloud-history";
import { errorStats, gpuContext, type Work } from "./cloud-noise-gpu";

export async function createSkyFieldFixture() {
  const context = await gpuContext();
  const { device } = context;
  const textures: GPUTexture[] = [];
  const buffers: GPUBuffer[] = [];
  function texture(size: readonly number[], format: GPUTextureFormat = "rgba16float") {
    const t = device.createTexture({
      size: [...size],
      dimension: size.length === 3 ? "3d" : "2d",
      format,
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC,
    });
    textures.push(t);
    return t;
  }
  function buffer(size: number, usage: GPUBufferUsageFlags) {
    const b = device.createBuffer({ size, usage });
    buffers.push(b);
    return b;
  }
  const clear = texture(PHYSICAL_SKY_SIZE),
    diffuse = texture(PHYSICAL_DIFFUSE_SIZE),
    noise = texture(PHYSICAL_CLOUD_NOISE_SIZE, "rgba8unorm"),
    ambient = texture(PHYSICAL_CLOUD_LIGHT_SIZE),
    light = texture(PHYSICAL_CLOUD_LIGHT_SIZE),
    output = texture(PHYSICAL_CLOUD_VIEW_SIZE);
  const airRadiance = texture(PHYSICAL_AERIAL_SIZE),
    airTransmission = texture(PHYSICAL_AERIAL_SIZE);
  const atmosphere = defaultPhysicalAtmosphere();
  const table = buffer(atmosphere.data.byteLength, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST);
  device.queue.writeBuffer(table, 0, atmosphere.data);
  const frame = buffer(
    PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4,
    GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  );
  const sampler = device.createSampler({ minFilter: "linear", magFilter: "linear", addressModeU: "repeat" });
  const values = new Float32Array(PHYSICAL_ATMOSPHERE_FRAME_FLOATS);
  values.set(packCloudFormations(undefined), 60);
  values.set([0, 2, 0, 0]);
  values.set([-0.6, 0.55, -0.58, 2.7], 4);
  values.set([1, 0.97, 0.92, 0], 8);
  values.set([...atmosphere.planetCenter, 0], 12);
  values.set([1, 0, 0, 0], 16);
  values.set([0, Math.cos(0.5), -Math.sin(0.5), 0], 20);
  values.set([0, Math.sin(0.5), Math.cos(0.5), 0], 24);
  values.set([4 / 3, Math.tan(Math.PI / 6), 0, 0], 28);
  values.set([0.14, 0.17, 0.13, 0], 32);
  values.set([0.58, 0.55, 0.2, 0], 36);
  values.set([0.6, -0.55, 0.58, 0], 40);
  values.set([0.8, 0.1, 0, 0], 48);
  values.set([0, 0, 5000, 0], 52);
  values.set([1, 0, 0, 0], 56);
  const entry = (binding: number, resource: GPUBindingResource): GPUBindGroupEntry => ({ binding, resource });
  const shared = [entry(0, { buffer: frame }), entry(1, { buffer: table })];
  const cache = [entry(21, ambient.createView())];
  async function kernel(
    code: string,
    entryPoint: string,
    entries: GPUBindGroupEntry[],
    size: readonly number[],
    constants?: Record<string, number>,
    groupSize = 8,
  ): Promise<Work & { pipeline: GPUComputePipeline; group: GPUBindGroup }> {
    const module = device.createShaderModule({ code });
    const errors = (await module.getCompilationInfo()).messages.filter((m) => m.type === "error");
    if (errors.length) throw Error(errors.map((m) => `${m.lineNum}: ${m.message}`).join("\n"));
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint, constants },
    });
    const group = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
    const work: Work = (encoder, timestampWrites) => {
      const pass = encoder.beginComputePass({ timestampWrites });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      pass.dispatchWorkgroups(
        Math.ceil(size[0] / groupSize),
        Math.ceil(size[1] / groupSize),
        size[2] ? Math.ceil(size[2] / groupSize) : 1,
      );
      pass.end();
    };
    return Object.assign(work, { pipeline, group });
  }
  const buildNoise = await kernel(
    physicalCloudNoiseComputeWGSL,
    "main",
    [entry(0, noise.createView())],
    PHYSICAL_CLOUD_NOISE_SIZE,
    undefined,
    4,
  );
  const buildDiffuse = await kernel(
    physicalMultipleScatteringComputeWGSL,
    "physicalDiffuseBuild",
    [...shared, entry(2, diffuse.createView())],
    PHYSICAL_DIFFUSE_SIZE,
  );
  const buildClear = await kernel(
    physicalAtmosphereComputeWGSL,
    "physicalSkyBuild",
    [
      ...shared,
      entry(2, clear.createView()),
      entry(5, diffuse.createView()),
      entry(6, sampler),
      entry(19, light.createView()),
    ],
    PHYSICAL_SKY_SIZE,
  );
  const buildAir = await kernel(
    physicalAtmosphereComputeWGSL,
    "physicalAerialBuild",
    [
      ...shared,
      entry(3, airRadiance.createView()),
      entry(4, airTransmission.createView()),
      entry(5, diffuse.createView()),
      entry(6, sampler),
      entry(19, light.createView()),
    ],
    PHYSICAL_AERIAL_SIZE,
    undefined,
    4,
  );
  const buildLight = await kernel(
    physicalCloudLightComputeWGSL,
    "main",
    [
      ...shared,
      entry(5, diffuse.createView()),
      entry(6, sampler),
      entry(17, noise.createView()),
      entry(20, light.createView()),
      entry(23, ambient.createView()),
    ],
    PHYSICAL_CLOUD_LIGHT_SIZE,
    undefined,
    4,
  );
  const works: Record<string, Work> = {};
  for (const [name, steps, cached] of [
    ["scrambled32", 32, 0],
    ["procedural64", 64, 0],
    ["cached64", 64, 1],
    ["adaptive256", 256, 1],
    ["adaptive384", 384, 1],
    ["adaptive512", 512, 1],
    ["adaptive192", 192, 1],
    ["adaptive224", 224, 1],
    ["reference1024", 1024, 0],
    ["reference2048", 2048, 0],
    ["reference4096", 4096, 0],
  ] as const) {
    works[name] = await kernel(
      physicalCloudComputeWGSL,
      "physicalCloudViewBuild",
      [
        ...shared,
        entry(2, clear.createView()),
        entry(4, output.createView()),
        entry(5, diffuse.createView()),
        entry(6, sampler),
        entry(9, airRadiance.createView()),
        entry(10, airTransmission.createView()),
        entry(17, noise.createView()),
        entry(19, light.createView()),
        ...cache,
      ],
      PHYSICAL_CLOUD_VIEW_SIZE,
      {
        PHYSICAL_CLOUD_VIEW_STEPS: steps,
        PHYSICAL_CLOUD_FORMATION_STEPS: name === "adaptive384" ? 512 : steps,
        PHYSICAL_CLOUD_PHASE_SPREAD: name === "adaptive384" ? 0.6 : 0,
        PHYSICAL_CLOUD_CACHED_LIGHTING: cached,
        PHYSICAL_CLOUD_CACHED_AIR: cached,
        PHYSICAL_CLOUD_JITTER: Number(name === "scrambled32"),
        PHYSICAL_CLOUD_ADAPTIVE: Number(name.startsWith("adaptive")),
        PHYSICAL_CLOUD_TARGET_STEP:
          name === "adaptive192" ? 40 : name === "adaptive256" || name === "adaptive384" ? 15 : 35,
      },
    );
  }
  const [width, height] = PHYSICAL_CLOUD_VIEW_SIZE;
  const readback = buffer(width * height * 8, GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ);
  async function image(work: Work, source = output) {
    const encoder = device.createCommandEncoder();
    work(encoder);
    encoder.copyTextureToBuffer({ texture: source }, { buffer: readback, bytesPerRow: width * 8 }, [
      width,
      height,
    ]);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const raw = new Uint16Array(readback.getMappedRange());
    const data = new Float32Array(width * height * 4);
    for (let i = 0; i < raw.length; i++) {
      const bits = raw[i],
        e = (bits >> 10) & 31,
        f = bits & 1023;
      data[i] =
        (bits & 32768 ? -1 : 1) *
        (e === 0 ? f * 2 ** -24 : e === 31 ? (f ? NaN : Infinity) : (1 + f / 1024) * 2 ** (e - 15));
    }
    readback.unmap();
    return data;
  }
  function display(data: Float32Array) {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw Error("Missing canvas");
    const out = ctx.createImageData(width, height);
    for (let i = 0; i < data.length; i++) {
      const x = data[i] * 3;
      out.data[i] =
        i % 4 === 3
          ? 255
          : Math.round(
              255 *
                Math.min(1, Math.max(0, (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14))) **
                  (1 / 2.2),
            );
    }
    ctx.putImageData(out, 0, 0);
    return canvas.toDataURL();
  }
  return {
    async check(selected?: string[]) {
      device.queue.writeBuffer(frame, 0, values);
      let encoder = device.createCommandEncoder();
      buildNoise(encoder);
      buildDiffuse(encoder);
      device.queue.submit([encoder.finish()]);
      const cases = [];
      const images: Record<string, string> = {};
      for (const [name, cover, elevation, development, storm] of [
        ["broken", 0.48, 0.6, 0.8, 0.1],
        ["overcast", 0.92, 0.5, 0.62, 0.7],
        ["sunset", 0.48, 0.04, 0.8, 0.1],
        ["zenith", 0.48, 0.6, 0.8, 0.1],
        ["night", 0.48, -0.6, 0.8, 0.1],
        ["translated-camera", 0.48, 0.6, 0.8, 0.1],
        ["formations", 0.48, 0.6, 0.8, 0.1],
        ["formations-sunset", 0.48, 0.04, 0.8, 0.1],
        ["formations-bank", 0.48, 0.6, 0.8, 0.1],
        ["formations-wisp", 0.48, 0.6, 0.8, 0.1],
        ["layers-only", 0, 0.6, 0.8, 0.1],
        ["layers-horizon", 0, 0.04, 0.8, 0.1],
        ["layers-night", 0, -0.6, 0.8, 0.1],
        ["clear", 0, 0.6, 0.8, 0.1],
      ] as const) {
        if (selected && !selected.includes(name)) continue;
        values.set(
          packCloudFormations({
            development,
            storminess: storm,
            highCloudCover: name.startsWith("layers") ? 0.4 : 0,
            midCloudCover: name.startsWith("layers") ? 0.4 : 0,
            background: name.startsWith("formations") ? 0.35 : 1,
            formations: name.startsWith("formations")
              ? [
                  {
                    id: "reference-formation",
                    kind: name === "formations-bank" ? "bank" : name === "formations-wisp" ? "wisp" : "tower",
                    center: [0, 6000],
                    base: name === "formations-wisp" ? 2700 : 1200,
                    size:
                      name === "formations-bank"
                        ? [4200, 1800, 1900]
                        : name === "formations-wisp"
                          ? [3000, 900, 1000]
                          : [2200, 4000, 1800],
                    yaw: 0.3,
                    density: 1,
                    erosion: name === "formations-wisp" ? 0.8 : 0.35,
                    seed: 31,
                  },
                ]
              : undefined,
          }),
          60,
        );
        values[0] = name === "translated-camera" ? 15500 : 0;
        values[43] = name === "night" || name === "layers-night" ? 0.08 : 0;
        values[44] = name === "night" || name === "layers-night" ? 1 : 0;
        values.set(
          [0.8 * Math.cos(elevation), -Math.sin(elevation), 0.6 * Math.cos(elevation), values[43]],
          40,
        );
        values[36] = cover;
        values[50] = name.startsWith("layers") ? 0.4 : 0;
        values[48] = development;
        values[49] = storm;
        values.set([-0.8 * Math.cos(elevation), Math.sin(elevation), -0.6 * Math.cos(elevation), 2.7], 4);
        const pitch = name === "zenith" ? 1.4 : name === "layers-horizon" ? 0.06 : 0.5;
        values.set([0, Math.cos(pitch), -Math.sin(pitch), 0], 20);
        values.set([0, Math.sin(pitch), Math.cos(pitch), 0], 24);
        device.queue.writeBuffer(frame, 0, values);
        encoder = device.createCommandEncoder();
        buildLight(encoder);
        buildClear(encoder);
        buildAir(encoder);
        device.queue.submit([encoder.finish()]);
        const reference = await image(works.reference1024);
        const denser = await image(works.reference2048);
        const finest = await image(works.reference4096);
        const samples: Record<string, Float32Array> = {};
        for (const key of [
          "scrambled32",
          "procedural64",
          "cached64",
          "adaptive256",
          "adaptive384",
          "adaptive512",
          "adaptive192",
          "adaptive224",
        ]) {
          samples[key] = await image(works[key]);
          images[`${name}-${key}`] = display(samples[key]);
        }
        images[`${name}-reference1024`] = display(reference);
        const errors = Object.fromEntries(
          Object.entries(samples).map(([key, data]) => [key, errorStats(data, reference)]),
        );
        const finite = [reference, denser, ...Object.values(samples)].every((a) =>
          a.every((v) => Number.isFinite(v) && v >= 0),
        );
        cases.push({
          name,
          finite,
          errors,
          referenceConvergence: errorStats(reference, denser),
          finestConvergence: errorStats(denser, finest),
          cacheOnlyError: errorStats(samples.cached64, samples.procedural64),
          timing: await context.benchmark(
            {
              scrambled32: works.scrambled32,
              procedural64: works.procedural64,
              cached64: works.cached64,
              adaptive256: works.adaptive256,
              adaptive384: works.adaptive384,
              adaptive192: works.adaptive192,
              adaptive224: works.adaptive224,
            },
            2,
          ),
        });
      }
      // Time an occupied overcast field, not the last (clear) early-out case.
      values[36] = 0.92;
      values[48] = 0.62;
      values[49] = 0.7;
      device.queue.writeBuffer(frame, 0, values);
      return {
        scope:
          "Production camera cloud kernel. Scrambled32 is a control with the UPDATED density and aerial model, not the old renderer. 1024/2048 sample comparisons test view quadrature only; shared lighting is an approximation. Timings exclude scene, display, readback, and lookup builds.",
        adapter: context.adapter,
        size: [width, height],
        additionalBytes: PHYSICAL_CLOUD_LIGHT_SIZE.reduce((a, b) => a * b, 8),
        buildMs: await context.benchmark({ lighting: buildLight }, 2),
        cases,
        images,
        errors: context.errors,
      };
    },
    async temporalCheck(selected?: string[]) {
      // Audit emitted WGSL against numerical integration, including both clamp
      // crossings, reversed gradients, saturated columns, and near-zero slopes.
      const integralBuffer = buffer(129 * 4, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
      const pairs = Array.from({ length: 129 }, (_, i) =>
        i === 128 ? [0.42, 0.420001] : [Math.sin(i * 1.73) * 5, Math.cos(i * 2.17) * 5],
      );
      const literals = pairs.map(([a, b]) => `vec2f(${a},${b})`).join(",");
      const integralWork = await kernel(
        `${cloudTransportWGSL}
        @group(0) @binding(0) var<storage,read_write> result:array<f32>;
        @compute @workgroup_size(8,8) fn integral(@builtin(global_invocation_id) id:vec3u) {
          if(id.y>0u||id.x>=129u) {return;}
          let pairs=array<vec2f,129>(${literals});
          result[id.x]=compiledCloudDensityIntegral(pairs[id.x].x,pairs[id.x].y);
        }`,
        "integral",
        [entry(0, { buffer: integralBuffer })],
        [129, 1],
      );
      const integralValues = await context.values(integralWork, integralBuffer, 129);
      const integralError = Math.max(
        ...pairs.map(([a, b], i) => {
          let sum = 0;
          for (let k = 0; k < 32768; k++) sum += Math.max(0, Math.min(1, a + ((b - a) * (k + 0.5)) / 32768));
          return Math.abs(integralValues[i] - sum / 32768);
        }),
      );
      if (!Number.isFinite(integralError) || integralError > 0.000002)
        throw Error("Compiled cloud integral mismatch");
      const tailBuffer = buffer(65 * 4, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
      const tailWork = await kernel(
        `${physicalCloudComputeWGSL}
        @group(0) @binding(28) var<storage,read_write> tailResult:array<f32>;
        @compute @workgroup_size(8,8) fn tailAudit(@builtin(global_invocation_id) id:vec3u) {
          if(id.y>0u||id.x>=65u) {return;}
          tailResult[id.x]=physicalCloudScatteringTail(f32(id.x)*4.0);
        }`,
        "tailAudit",
        [entry(28, { buffer: tailBuffer })],
        [65, 1],
      );
      const tailValues = await context.values(tailWork, tailBuffer, 65);
      const scatteringTailError = Math.max(
        ...tailValues.map((v, i) => {
          let expected = 0;
          for (let order = 4; order <= 7; order++) expected += 0.7 ** order * Math.exp(-i * 4 * 0.5 ** order);
          if (!Number.isFinite(v) || v < 0 || (i > 0 && v > tailValues[i - 1]))
            throw Error("Invalid scattering tail");
          return Math.abs(v - expected);
        }),
      );
      if (scatteringTailError > 0.000001 || Math.abs((1.533 + tailValues[0]) * 0.7159626 - 1.533) > 0.000001)
        throw Error("Cloud scattering tail exceeded energy/reference gate");
      const history = [texture(PHYSICAL_CLOUD_VIEW_SIZE), texture(PHYSICAL_CLOUD_VIEW_SIZE)];
      const moments = [
        texture(PHYSICAL_CLOUD_VIEW_SIZE, "rgba16float"),
        texture(PHYSICAL_CLOUD_VIEW_SIZE, "rgba16float"),
      ];
      const queue = buffer(
        512 + width * Math.ceil(height / 16) * 16 * 4,
        GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC,
      );
      const previous = buffer(
        (PHYSICAL_ATMOSPHERE_FRAME_FLOATS + 4) * 4,
        GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
      );
      const previousData = new Float32Array(PHYSICAL_ATMOSPHERE_FRAME_FLOATS + 4);
      const schedule = new Uint32Array(previousData.buffer, PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4, 4);
      const updates: Work[] = [];
      for (let index = 0; index < 2; index++) {
        const classify = await kernel(
          physicalCloudTemporalComputeWGSL,
          "physicalCloudTemporalBuild",
          [
            entry(0, { buffer: frame }),
            entry(4, history[index].createView()),
            entry(6, sampler),
            entry(22, history[1 - index].createView()),
            entry(24, moments[1 - index].createView()),
            entry(25, moments[index].createView()),
            entry(26, { buffer: previous }),
            entry(27, { buffer: queue }),
          ],
          PHYSICAL_CLOUD_VIEW_SIZE,
        );
        const trace = await kernel(
          physicalCloudTemporalComputeWGSL,
          "physicalCloudTraceQueued",
          [
            ...shared,
            entry(2, clear.createView()),
            entry(4, history[index].createView()),
            entry(5, diffuse.createView()),
            entry(6, sampler),
            entry(9, airRadiance.createView()),
            entry(10, airTransmission.createView()),
            entry(17, noise.createView()),
            entry(19, light.createView()),
            ...cache,
            entry(25, moments[index].createView()),
            entry(27, { buffer: queue }),
          ],
          [width * 16, Math.ceil(height / 16)],
          {
            PHYSICAL_CLOUD_VIEW_STEPS: 384,
            PHYSICAL_CLOUD_FORMATION_STEPS: 512,
            PHYSICAL_CLOUD_PHASE_SPREAD: 0.6,
            PHYSICAL_CLOUD_TARGET_STEP: 15,
          },
          64,
        );
        updates.push((encoder, timestamps) => {
          encoder.clearBuffer(queue, 0, 512);
          const pass = encoder.beginComputePass({ timestampWrites: timestamps });
          pass.setPipeline(classify.pipeline);
          pass.setBindGroup(0, classify.group);
          pass.dispatchWorkgroups(Math.ceil(width / 8), Math.ceil(height / 8));
          pass.setPipeline(trace.pipeline);
          pass.setBindGroup(0, trace.group);
          pass.dispatchWorkgroups(Math.ceil((width * 16) / 64), Math.ceil(height / 16));
          pass.end();
        });
      }
      values[36] = 0.48;
      values[50] = 0.15;
      values[48] = 0.8;
      values[49] = 0.1;
      device.queue.writeBuffer(frame, 0, values);
      let encoder = device.createCommandEncoder();
      buildNoise(encoder);
      buildDiffuse(encoder);
      device.queue.submit([encoder.finish()]);
      const cases = [];
      const images: Record<string, string> = {};
      let index = 0;
      for (const scenario of [
        "still",
        "walking",
        "turning",
        "wind",
        "weather",
        "lighting",
        "sunset",
        "cirrus",
        "clear-transition",
        "source-switch",
        "cut",
        "rebase",
        "formations",
        "formation-edit",
        "formation-weather",
        "formation-rebase",
        "growth",
        "middle-wind",
        "layers-only",
        "layers-rebase",
      ] as const) {
        if (selected && !selected.includes(scenario)) continue;
        const steps = [];
        for (let step = 0; step < 12; step++) {
          previousData.set(values);
          values.set(
            packCloudFormations({
              development: 0.8,
              storminess: 0.1,
              highCloudCover: 0.15,
              midCloudCover: ["middle-wind", "layers-only", "layers-rebase"].includes(scenario) ? 0.45 : 0,
              background: scenario === "formation-weather" ? 0.4 + step * 0.002 : 1,
              formations:
                scenario.startsWith("formation") || scenario === "growth"
                  ? [
                      {
                        id: "motion-tower",
                        maturity: scenario === "growth" ? step / 12 : 0.45,
                        shear: scenario === "growth" ? step / 12 : 0,
                        kind: "tower",
                        center: [scenario === "formation-edit" ? step * 80 : 0, 6000],
                        base: 1200,
                        size: [2200, 4000, 1800],
                        yaw: 0.3,
                        density: 1,
                        erosion: 0.35,
                        seed: 31,
                      },
                    ]
                  : undefined,
            }),
            60,
          );
          const pitch = 0.5;
          const yaw = scenario === "turning" ? step * 0.02 : 0;
          const x = scenario === "walking" ? step * 2 : scenario === "cut" && step >= 6 ? 4000 : 0;
          values[0] = x;
          values[39] =
            scenario === "wind" || scenario === "cirrus" || scenario === "middle-wind" ? step * 20 : 0;
          values[50] = scenario === "cirrus" ? 0.8 : 0.15;
          values[7] = scenario === "lighting" ? 2.7 + step * 0.008 : 2.7;
          const solarElevation = 0.12 - step * 0.008;
          values.set(
            scenario === "sunset"
              ? [-0.8 * Math.cos(solarElevation), Math.sin(solarElevation), -0.6 * Math.cos(solarElevation)]
              : [-0.6, 0.55, -0.58],
            4,
          );
          values[44] = scenario === "source-switch" && step >= 6 ? 0.505 : 0;
          values[43] = scenario === "source-switch" && step >= 6 ? 0.006 : 0;
          values[36] =
            scenario === "weather"
              ? 0.48 + step * 0.002
              : scenario === "clear-transition" && step >= 6
                ? 0
                : scenario === "layers-only" || scenario === "layers-rebase"
                  ? 0
                  : 0.48;
          values.set([Math.cos(yaw), 0, -Math.sin(yaw), 0], 16);
          values.set(
            [-Math.sin(yaw) * Math.sin(pitch), Math.cos(pitch), -Math.cos(yaw) * Math.sin(pitch), 0],
            20,
          );
          values.set(
            [Math.sin(yaw) * Math.cos(pitch), Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch), 0],
            24,
          );
          values[12] =
            (scenario === "rebase" || scenario === "formation-rebase" || scenario === "layers-rebase") &&
            step >= 6
              ? -8192
              : 0;
          if (values[12] !== 0) values[0] -= 8192;
          schedule.set([
            step % 8,
            Number(
              step > 0 &&
                canReuseCloudHistory(previousData.subarray(0, PHYSICAL_ATMOSPHERE_FRAME_FLOATS), values),
            ),
            cloudHistoryMaxAge(previousData.subarray(0, PHYSICAL_ATMOSPHERE_FRAME_FLOATS), values),
            0,
          ]);
          device.queue.writeBuffer(frame, 0, values);
          device.queue.writeBuffer(previous, 0, previousData);
          encoder = device.createCommandEncoder();
          buildLight(encoder);
          buildClear(encoder);
          buildAir(encoder);
          device.queue.submit([encoder.finish()]);
          const rendered = await image(updates[index], history[index]);
          const reference = await image(works.adaptive384);
          const count = await context.values(() => {}, queue, 128);
          const fresh = new Uint32Array(count.buffer).reduce((a, b) => a + b, 0);
          steps.push({
            step,
            error: errorStats(rendered, reference),
            freshFraction: fresh / (width * height),
          });
          if (step === 10) {
            cases.push({
              name: scenario,
              steps,
              timing: await context.benchmark({ full: works.adaptive384, temporal: updates[index] }, 2),
            });
            images[`${scenario}-temporal`] = display(rendered);
            images[`${scenario}-reference`] = display(reference);
          }
          index = 1 - index;
        }
      }
      return {
        adapter: context.adapter,
        size: [width, height],
        integralError,
        scatteringTailError,
        cases,
        images,
        errors: context.errors,
      };
    },
    dispose() {
      for (const t of textures) t.destroy();
      for (const b of buffers) b.destroy();
      context.dispose();
    },
  };
}
