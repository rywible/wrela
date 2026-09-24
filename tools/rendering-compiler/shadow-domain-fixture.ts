import sceneShader from "@wrela/render-webgpu/scene.wgsl" with { type: "text" };
/** Exercise the production footprint shadow certificate, including a one-texel blocker. */
export async function checkShadowDomainGpu() {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw Error("WebGPU unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
  const source = sceneShader.slice(
    sceneShader.indexOf("fn waterGlintVisibility("),
    sceneShader.indexOf("fn shade("),
  );
  const texture = device.createTexture({
    size: [32, 32],
    format: "depth32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  const uniform = device.createBuffer({ size: 96, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const output = device.createBuffer({ size: 16, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
  const read = device.createBuffer({ size: 16, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  const frame = new Float32Array(24);
  frame[0] = frame[5] = frame[10] = frame[15] = frame[23] = 1;
  device.queue.writeBuffer(uniform, 0, frame);
  try {
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: {
        module: device.createShaderModule({
          code: `
struct Frame {lightVP:mat4x4f,ground:vec4f,sunlight:vec4f};
@group(0) @binding(0) var<uniform> g:Frame;
@group(0) @binding(1) var shadowMap:texture_depth_2d;
@group(0) @binding(2) var<storage,read_write> results:array<f32>;
${source}
@compute @workgroup_size(1) fn main(){
 results[0]=waterGlintVisibility(vec3f(0.0,0.0,0.5),vec3f(0.001,0.0,0.0),vec3f(0.0,0.001,0.0));
 results[1]=waterGlintVisibility(vec3f(2.0,0.0,0.5),vec3f(0.001,0.0,0.0),vec3f(0.0,0.001,0.0));
 results[2]=waterGlintVisibility(vec3f(0.999,0.0,0.5),vec3f(0.001,0.0,0.0),vec3f(0.0,0.001,0.0));
 results[3]=waterGlintVisibility(vec3f(0.0,0.0,0.5),vec3f(1.0,0.0,0.0),vec3f(0.0,1.0,0.0));
}`,
        }),
        entryPoint: "main",
      },
    });
    const blocker = device.createShaderModule({
      code: `
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {let p=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(p*2.0-1.0,0.2,1.0);}
@fragment fn fs()->@builtin(frag_depth) f32 {return 0.2;}`,
    });
    const raster = await device.createRenderPipelineAsync({
      layout: "auto",
      vertex: { module: blocker, entryPoint: "vs" },
      fragment: { module: blocker, entryPoint: "fs", targets: [] },
      depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "always" },
    });
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: uniform } },
        { binding: 1, resource: texture.createView() },
        { binding: 2, resource: { buffer: output } },
      ],
    });
    const cases = [];
    for (const name of ["lit", "dark", "one-texel-blocker"]) {
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [],
        depthStencilAttachment: {
          view: texture.createView(),
          depthClearValue: name === "dark" ? 0.2 : 0.8,
          depthLoadOp: "clear",
          depthStoreOp: "store",
        },
      });
      if (name === "one-texel-blocker") {
        pass.setPipeline(raster);
        pass.setScissorRect(16, 16, 1, 1);
        pass.draw(3);
      }
      pass.end();
      const compute = encoder.beginComputePass();
      compute.setPipeline(pipeline);
      compute.setBindGroup(0, group);
      compute.dispatchWorkgroups(1);
      compute.end();
      encoder.copyBufferToBuffer(output, 0, read, 0, 16);
      device.queue.submit([encoder.finish()]);
      await read.mapAsync(GPUMapMode.READ);
      const actual = Array.from(new Float32Array(read.getMappedRange().slice(0)));
      read.unmap();
      const expected = [name === "lit" ? 1 : name === "dark" ? 0 : -1, 1, -1, -1];
      if (actual.some((v, i) => v !== expected[i]))
        throw Error(`Shadow domain ${name}: ${actual}, expected ${expected}`);
      cases.push({ name, actual, expected });
    }
    if (errors.length) throw Error(errors.join("\n"));
    return { cases, errors };
  } finally {
    texture.destroy();
    uniform.destroy();
    output.destroy();
    read.destroy();
    device.destroy();
  }
}
