import { irradianceBasis, irradianceBuildWGSL } from "@wrela/render-webgpu/irradiance";
/** Test the actual convolution shader against a sky with a known cosine integral. */
export async function checkIrradianceGpu() {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw Error("WebGPU unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
  const texture = device.createTexture({
    size: [256, 128],
    format: "rgba16float",
    usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
  });
  const coefficients = device.createBuffer({
    size: 144,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const read = device.createBuffer({ size: 144, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  try {
    const build = await device.createComputePipelineAsync({
      layout: "auto",
      compute: {
        module: device.createShaderModule({
          code: `
@group(0) @binding(0) var sky:texture_storage_2d<rgba16float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
 let uv=(vec2f(id.xy)+0.5)/vec2f(256.0,128.0);let e=2.0*uv.y-1.0;let y=sin(sign(e)*e*e*1.57079632679);
 textureStore(sky,vec2i(id.xy),vec4f(1.0+0.4*y,0.5,0.25-0.1*y,1.0));
}`,
        }),
        entryPoint: "main",
      },
    });
    const integrate = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module: device.createShaderModule({ code: irradianceBuildWGSL }), entryPoint: "main" },
    });
    const encoder = device.createCommandEncoder();
    let pass = encoder.beginComputePass();
    pass.setPipeline(build);
    pass.setBindGroup(
      0,
      device.createBindGroup({
        layout: build.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: texture.createView() }],
      }),
    );
    pass.dispatchWorkgroups(32, 16);
    pass.end();
    pass = encoder.beginComputePass();
    pass.setPipeline(integrate);
    pass.setBindGroup(
      0,
      device.createBindGroup({
        layout: integrate.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: texture.createView() },
          { binding: 1, resource: device.createSampler({ magFilter: "linear", minFilter: "linear" }) },
          { binding: 2, resource: { buffer: coefficients } },
        ],
      }),
    );
    pass.dispatchWorkgroups(9);
    pass.end();
    encoder.copyBufferToBuffer(coefficients, 0, read, 0, 144);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const values = new Float32Array(read.getMappedRange().slice(0));
    read.unmap();
    let maximumError = 0;
    for (let i = 0; i < 128; i++) {
      const y = 1 - (2 * (i + 0.5)) / 128,
        a = i * 2.3999632297,
        r = Math.sqrt(1 - y * y),
        basis = irradianceBasis([r * Math.cos(a), y, r * Math.sin(a)]);
      const actual = [0, 1, 2].map((c) => basis.reduce((sum, b, k) => sum + b * values[k * 4 + c], 0));
      const expected = [1 + (0.4 * y * 2) / 3, 0.5, 0.25 - (0.1 * y * 2) / 3];
      maximumError = Math.max(maximumError, ...actual.map((v, c) => Math.abs(v - expected[c])));
    }
    if (maximumError > 0.002 || errors.length)
      throw Error(`Irradiance convolution failed: ${maximumError}; ${errors.join(";")}`);
    return { directions: 128, maximumError, errors };
  } finally {
    texture.destroy();
    coefficients.destroy();
    read.destroy();
    device.destroy();
  }
}
