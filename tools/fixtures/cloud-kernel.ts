import {
  defaultPhysicalAtmosphere,
  PHYSICAL_AERIAL_SIZE,
  PHYSICAL_ATMOSPHERE_FRAME_FLOATS,
  PHYSICAL_CLOUD_LIGHT_SIZE,
  PHYSICAL_CLOUD_VIEW_SIZE,
  PHYSICAL_DIFFUSE_SIZE,
  PHYSICAL_SKY_SIZE,
  physicalAtmosphereComputeWGSL,
  physicalCloudComputeWGSL,
  physicalCloudLightComputeWGSL,
  physicalMultipleScatteringComputeWGSL,
} from "@wrela/render-webgpu/atmosphere";
import { errorStats, type gpuContext, type Work } from "./cloud-noise-gpu";

type Context = Awaited<ReturnType<typeof gpuContext>>;

/** Production cloud-view kernel, isolated from draw/AA/display and cold LUT construction. */
export async function createCloudKernelComparison(
  context: Context,
  noise: GPUTextureView,
  sampler: GPUSampler,
) {
  const device = context.device;
  const textures: GPUTexture[] = [];
  const texture = (size: readonly number[]) => {
    const value = device.createTexture({
      size: [...size],
      dimension: size.length === 3 ? "3d" : "2d",
      format: "rgba16float",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC,
    });
    textures.push(value);
    return value;
  };
  const clear = texture(PHYSICAL_SKY_SIZE),
    diffuse = texture(PHYSICAL_DIFFUSE_SIZE);
  const output = texture(PHYSICAL_CLOUD_VIEW_SIZE);
  const light = texture(PHYSICAL_CLOUD_LIGHT_SIZE),
    ambient = texture(PHYSICAL_CLOUD_LIGHT_SIZE);
  const airRadiance = texture(PHYSICAL_AERIAL_SIZE),
    airTransmission = texture(PHYSICAL_AERIAL_SIZE);
  const atmosphere = defaultPhysicalAtmosphere();
  const table = device.createBuffer({
    size: atmosphere.data.byteLength,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
  device.queue.writeBuffer(table, 0, atmosphere.data);
  const frame = device.createBuffer({
    size: PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4,
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  const values = new Float32Array(PHYSICAL_ATMOSPHERE_FRAME_FLOATS);
  values.set([0, 1, 0, 0], 60);
  values.set([0, 2, 0, 0], 0);
  values.set([Math.cos(0.58) * Math.sin(-2), Math.sin(0.58), Math.cos(0.58) * Math.cos(-2), 2.7], 4);
  values.set([1, 0.97, 0.92, 0], 8);
  values.set([...atmosphere.planetCenter, 0], 12);
  values.set([1, 0, 0, 0], 16);
  values.set([0, Math.cos(0.3), -Math.sin(0.3), 0], 20);
  values.set([0, Math.sin(0.3), Math.cos(0.3), 0], 24);
  values.set([4 / 3, Math.tan(Math.PI / 6), 0, 0], 28);
  values.set([0.14, 0.17, 0.13, 0], 32);
  values.set([0.58, 0.55, 0.2, 0], 36);
  values.set([0, 1, 0, 0], 40);
  values.set([0.8, 0.1, 0, 0], 48);
  values.set([0, 0, 5000, 0], 52);
  values.set([1, 0, 0, 0], 56);
  device.queue.writeBuffer(frame, 0, values);

  async function kernel(
    code: string,
    entryPoint: string,
    entries: GPUBindGroupEntry[],
    size: readonly number[],
    constants?: Record<string, number>,
  ) {
    const module = device.createShaderModule({ code });
    const errors = (await module.getCompilationInfo()).messages.filter((message) => message.type === "error");
    if (errors.length)
      throw Error(errors.map((message) => `${message.lineNum}: ${message.message}`).join("\n"));
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint, constants },
    });
    const group = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
    const work: Work = (encoder, timestampWrites) => {
      const pass = encoder.beginComputePass({ timestampWrites });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      const groupSize = size.length === 3 ? 4 : 8;
      pass.dispatchWorkgroups(
        Math.ceil(size[0] / groupSize),
        Math.ceil(size[1] / groupSize),
        size[2] ? Math.ceil(size[2] / groupSize) : 1,
      );
      pass.end();
    };
    return work;
  }
  const shared = [
    { binding: 0, resource: { buffer: frame } },
    { binding: 1, resource: { buffer: table } },
  ];
  const buildDiffuse = await kernel(
    physicalMultipleScatteringComputeWGSL,
    "physicalDiffuseBuild",
    [...shared, { binding: 2, resource: diffuse.createView() }],
    PHYSICAL_DIFFUSE_SIZE,
  );
  const buildClear = await kernel(
    physicalAtmosphereComputeWGSL,
    "physicalSkyBuild",
    [
      ...shared,
      { binding: 2, resource: clear.createView() },
      { binding: 5, resource: diffuse.createView() },
      { binding: 6, resource: sampler },
      { binding: 19, resource: light.createView() },
    ],
    PHYSICAL_SKY_SIZE,
  );
  const buildAir = await kernel(
    physicalAtmosphereComputeWGSL,
    "physicalAerialBuild",
    [
      ...shared,
      { binding: 3, resource: airRadiance.createView() },
      { binding: 4, resource: airTransmission.createView() },
      { binding: 5, resource: diffuse.createView() },
      { binding: 6, resource: sampler },
      { binding: 19, resource: light.createView() },
    ],
    PHYSICAL_AERIAL_SIZE,
  );
  const buildLight = await kernel(
    physicalCloudLightComputeWGSL,
    "main",
    [
      ...shared,
      { binding: 5, resource: diffuse.createView() },
      { binding: 6, resource: sampler },
      { binding: 17, resource: noise },
      { binding: 20, resource: light.createView() },
      { binding: 23, resource: ambient.createView() },
    ],
    PHYSICAL_CLOUD_LIGHT_SIZE,
  );
  const works: Record<string, Work> = {};
  for (const compiled of [false, true]) {
    works[compiled ? "compiled" : "analytic"] = await kernel(
      physicalCloudComputeWGSL,
      "physicalCloudViewBuild",
      [
        ...shared,
        { binding: 2, resource: clear.createView() },
        { binding: 4, resource: output.createView() },
        { binding: 5, resource: diffuse.createView() },
        { binding: 6, resource: sampler },
        { binding: 17, resource: noise },
        { binding: 9, resource: airRadiance.createView() },
        { binding: 10, resource: airTransmission.createView() },
        { binding: 19, resource: light.createView() },
        { binding: 21, resource: ambient.createView() },
      ],
      PHYSICAL_CLOUD_VIEW_SIZE,
      { PHYSICAL_CLOUD_COMPILED_NOISE: Number(compiled) },
    );
  }
  const [width, height] = PHYSICAL_CLOUD_VIEW_SIZE;
  const readback = device.createBuffer({
    size: width * height * 8,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  async function image(work: Work) {
    const encoder = device.createCommandEncoder();
    work(encoder);
    encoder.copyTextureToBuffer(
      { texture: output },
      { buffer: readback, bytesPerRow: width * 8 },
      { width, height },
    );
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const half = new Uint16Array(readback.getMappedRange());
    const rgb = new Float32Array(width * height * 3);
    for (let pixel = 0; pixel < width * height; pixel++)
      for (let channel = 0; channel < 3; channel++) {
        const bits = half[pixel * 4 + channel],
          exponent = (bits >> 10) & 31,
          fraction = bits & 1023;
        const value =
          exponent === 0
            ? fraction * 2 ** -24
            : exponent === 31
              ? fraction
                ? NaN
                : Infinity
              : (1 + fraction / 1024) * 2 ** (exponent - 15);
        rgb[pixel * 3 + channel] = bits & 32768 ? -value : value;
      }
    readback.unmap();
    return rgb;
  }
  return {
    async check() {
      const encoder = device.createCommandEncoder();
      buildDiffuse(encoder);
      buildLight(encoder);
      buildClear(encoder);
      buildAir(encoder);
      device.queue.submit([encoder.finish()]);
      const cases = [];
      for (const [name, cover] of [
        ["thin", 0.25],
        ["overcast", 0.92],
      ] as const) {
        values[36] = cover;
        device.queue.writeBuffer(frame, 0, values);
        const lightingEncoder = device.createCommandEncoder();
        buildLight(lightingEncoder);
        device.queue.submit([lightingEncoder.finish()]);
        const analytic = await image(works.analytic),
          compiled = await image(works.compiled);
        const finite = [...analytic, ...compiled].every((value) => Number.isFinite(value) && value >= 0);
        cases.push({
          name,
          cover,
          finite,
          hdrError: errorStats(compiled, analytic),
          kernelMs: await context.benchmark(works, 2),
        });
      }
      return {
        size: [...PHYSICAL_CLOUD_VIEW_SIZE],
        cases,
        scope:
          "Actual production 32-segment camera-cloud kernel over matching clear-air/diffuse products. Timings exclude their construction, image readback, scene rendering, and display; image error is compiled versus analytic noise, not a converged transport reference.",
      };
    },
    dispose() {
      readback.destroy();
      table.destroy();
      frame.destroy();
      for (const value of textures) value.destroy();
    },
  };
}
