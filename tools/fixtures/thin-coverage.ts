import { needleCoverageTile, needleCoverageTriangles } from "@wrela/compiler/thin-coverage";
import { createThinCoverageGpu } from "@wrela/render-webgpu/thin-coverage";
import coverageWGSL from "@wrela/render-webgpu/thin-coverage.wgsl" with { type: "text" };
import { probeCoarseThinCoverage } from "./thin-coverage-coarse";

const SIZE = 128,
  REFERENCE_SCALE = 16;
const source = { seed: 73, count: 48, width: 0.0032, length: 0.085, shootLength: 0.4, spread: 0.08925 };
export type CoverageState = "still" | "wind" | "shadow";

/** Isolates production coverage filtering from lighting/tone mapping. The
 * reference rasterizes the exact semantic needles at 16x in each dimension. */
export async function createThinCoverageFixture() {
  const adapter = await navigator.gpu?.requestAdapter({ powerPreference: "high-performance" });
  if (!adapter) throw Error("No WebGPU adapter");
  const device = await adapter.requestDevice(),
    errors: string[] = [];
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  const tile = needleCoverageTile(source);
  const texture = createThinCoverageGpu(device, { ...tile, uv: new Float32Array(0) });
  const empty = device.createBindGroupLayout({ entries: [] });
  const layout = device.createBindGroupLayout({
    entries: [
      {
        binding: 3,
        visibility: GPUShaderStage.FRAGMENT,
        texture: { sampleType: "float", viewDimension: "2d-array" },
      },
      { binding: 4, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
    ],
  });
  const group = device.createBindGroup({
    layout,
    entries: [
      { binding: 3, resource: texture.view },
      {
        binding: 4,
        resource: device.createSampler({
          minFilter: "linear",
          magFilter: "linear",
          mipmapFilter: "linear",
          maxAnisotropy: 4,
        }),
      },
    ],
  });
  const module = device.createShaderModule({
    code: `${coverageWGSL}
struct V { @builtin(position) position:vec4f,@location(0) uv:vec2f,@location(1) @interpolate(flat) layer:f32 };
@vertex fn vertex(@location(0) p:vec2f,@location(1) uv:vec2f,@location(2) layer:f32)->V {return V(vec4f(p,0.5,1.0),uv,layer);}
@fragment fn filtered(v:V)->@location(0) vec4f {let reject=thinCoverageReject(vec3f(v.uv,0.0),v.position.xy);if(reject){discard;}return vec4f(1.0);}
@fragment fn msaa(v:V)->@location(0) vec4f {return vec4f(vec3f(1.0),thinCoverageAlpha(vec3f(v.uv,0.0)));}
struct Masked { @location(0) color:vec4f,@builtin(sample_mask) mask:u32 };
@fragment fn independent(v:V)->Masked {return Masked(vec4f(1.0),thinCoverageSampleMask(vec3f(v.uv,0.0),vec3f(v.uv*vec2f(0.1785,0.4),v.layer*0.037),4u));}
@fragment fn reference()->@location(0) vec4f {return vec4f(1.0);}`,
  });
  const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [empty, layout] });
  const pipelines = await Promise.all(
    ["filtered", "reference", "msaa", "independent"].map((entryPoint) =>
      device.createRenderPipelineAsync({
        layout: pipelineLayout,
        vertex: {
          module,
          entryPoint: "vertex",
          buffers: [
            {
              arrayStride: 20,
              attributes: [
                { shaderLocation: 0, offset: 0, format: "float32x2" },
                { shaderLocation: 1, offset: 8, format: "float32x2" },
                { shaderLocation: 2, offset: 16, format: "float32" },
              ],
            },
          ],
        },
        fragment: { module, entryPoint, targets: [{ format: "rgba8unorm" }] },
        primitive: { topology: "triangle-list" },
        multisample: {
          count: entryPoint === "msaa" || entryPoint === "independent" ? 4 : 1,
          alphaToCoverageEnabled: entryPoint === "msaa",
        },
      }),
    ),
  );
  async function raster(vertices: Float32Array, mode: "filtered" | "reference" | "msaa" | "independent") {
    const reference = mode === "reference";
    const size = SIZE * (reference ? REFERENCE_SCALE : 1),
      bytesPerRow = size * 4;
    const output = device.createTexture({
      size: [size, size],
      format: "rgba8unorm",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    const multisample =
      mode === "msaa" || mode === "independent"
        ? device.createTexture({
            size: [size, size],
            sampleCount: 4,
            format: "rgba8unorm",
            usage: GPUTextureUsage.RENDER_ATTACHMENT,
          })
        : undefined;
    const readback = device.createBuffer({
      size: bytesPerRow * size,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const buffer = device.createBuffer({
      size: vertices.byteLength,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    try {
      device.queue.writeBuffer(buffer, 0, vertices as Float32Array<ArrayBuffer>);
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: (multisample ?? output).createView(),
            ...(multisample ? { resolveTarget: output.createView() } : {}),
            loadOp: "clear",
            storeOp: "store",
            clearValue: [0, 0, 0, 1],
          },
        ],
      });
      pass.setPipeline(pipelines[mode === "independent" ? 3 : mode === "msaa" ? 2 : reference ? 1 : 0]);
      pass.setBindGroup(1, group);
      pass.setVertexBuffer(0, buffer);
      pass.draw(vertices.length / 5);
      pass.end();
      encoder.copyTextureToBuffer({ texture: output }, { buffer: readback, bytesPerRow }, [size, size]);
      device.queue.submit([encoder.finish()]);
      await readback.mapAsync(GPUMapMode.READ);
      const bytes = new Uint8Array(readback.getMappedRange()),
        result = new Float32Array(SIZE * SIZE);
      const scale = reference ? REFERENCE_SCALE : 1;
      for (let y = 0; y < size; y++)
        for (let x = 0; x < size; x++)
          result[Math.floor(y / scale) * SIZE + Math.floor(x / scale)] +=
            bytes[(y * size + x) * 4] / 255 / (scale * scale);
      readback.unmap();
      return result;
    } finally {
      buffer.destroy();
      readback.destroy();
      output.destroy();
      multisample?.destroy();
    }
  }
  function vertices(widthPixels: number, state: CoverageState, reference: boolean, layers: number) {
    const pixelScale = widthPixels / source.width;
    const width = source.spread * 2 * pixelScale,
      height = source.shootLength * pixelScale;
    const columns = Math.max(1, Math.ceil(SIZE / width)),
      rows = Math.max(1, Math.ceil(SIZE / height));
    const triangles = reference
      ? needleCoverageTriangles(source)
      : [
          [
            [0, 0],
            [1, 0],
            [0, 1],
          ],
          [
            [1, 0],
            [1, 1],
            [0, 1],
          ],
        ];
    const values: number[] = [];
    for (let layer = 0; layer < layers; layer++)
      for (let row = 0; row < rows; row++)
        for (let column = 0; column < columns; column++) {
          const dx = (column + 0.5) * width - SIZE * 0.5,
            dy = (row + 0.5) * height - SIZE * 0.5;
          const shift = state === "wind" ? 0.73 * Math.sin(row * 1.3 + column * 0.8) : 0;
          for (const triangle of triangles)
            for (const [u, v] of triangle) {
              let x = dx + (u - 0.5) * width + shift + layer * 0.37,
                y = dy + (v - 0.5) * height + layer * 0.61;
              // A different projected footprint exercises the same mask in light space.
              if (state === "shadow") x = x * 0.45 + y * 0.18;
              values.push((x / SIZE) * 2, (-y / SIZE) * 2, u, v, layer);
            }
        }
    return new Float32Array(values);
  }
  return {
    coarse() {
      return probeCoarseThinCoverage(device);
    },
    async capture(widthPixels: number, state: CoverageState = "still", layers = 1) {
      const reference = await raster(vertices(widthPixels, state, true, layers), "reference");
      const proxy = vertices(widthPixels, state, false, layers);
      const filtered = await raster(proxy, "filtered"),
        msaa = await raster(proxy, "msaa"),
        independent = await raster(proxy, "independent");
      let referenceCoverage = 0,
        filteredCoverage = 0,
        error = 0,
        msaaCoverage = 0,
        msaaError = 0,
        independentCoverage = 0,
        independentError = 0;
      const image = new ImageData(SIZE * 5, SIZE);
      for (let i = 0; i < reference.length; i++) {
        referenceCoverage += reference[i];
        filteredCoverage += filtered[i];
        error += Math.abs(reference[i] - filtered[i]);
        msaaCoverage += msaa[i];
        msaaError += Math.abs(reference[i] - msaa[i]);
        independentCoverage += independent[i];
        independentError += Math.abs(reference[i] - independent[i]);
        const x = i % SIZE,
          y = Math.floor(i / SIZE);
        for (let column = 0; column < 5; column++) {
          const value =
            column === 0
              ? reference[i]
              : column === 1
                ? filtered[i]
                : column === 2
                  ? msaa[i]
                  : column === 3
                    ? independent[i]
                    : Math.abs(reference[i] - independent[i]);
          const offset = (y * SIZE * 5 + column * SIZE + x) * 4;
          image.data.set([value * 255, value * 255, value * 255, 255], offset);
        }
      }
      referenceCoverage /= reference.length;
      filteredCoverage /= filtered.length;
      error /= reference.length;
      msaaCoverage /= reference.length;
      msaaError /= reference.length;
      independentCoverage /= reference.length;
      independentError /= reference.length;
      const canvas = document.createElement("canvas");
      canvas.width = SIZE * 5;
      canvas.height = SIZE;
      const context = canvas.getContext("2d");
      if (!context) throw Error("Missing coverage comparison canvas");
      context.putImageData(image, 0, 0);
      if (errors.length) throw Error(errors.join("\n"));
      return {
        image: canvas.toDataURL(),
        widthPixels,
        state,
        layers,
        referenceCoverage,
        filteredCoverage,
        coverageBias: filteredCoverage - referenceCoverage,
        meanAbsolutePixelError: error,
        msaaCoverage,
        msaaCoverageBias: msaaCoverage - referenceCoverage,
        msaaMeanAbsolutePixelError: msaaError,
        independentCoverage,
        independentCoverageBias: independentCoverage - referenceCoverage,
        independentMeanAbsolutePixelError: independentError,
        referenceScale: REFERENCE_SCALE,
        adapter: adapter.info.description || adapter.info.device || adapter.info.vendor,
        scope:
          "Production coverage shader;16x supersampled semantic triangles. Columns: reference, hashed single sample, hardware4x alpha-to-coverage, independent4x sample mask, absolute independent error. Layers are shifted overlapping sprays. Wind is subpixel source translation; shadow is oblique projection. No lighting or temporal accumulation.",
      };
    },
    dispose() {
      texture.texture.destroy();
      device.destroy();
    },
  };
}
