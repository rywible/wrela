import type { EvaluatedScene } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import {
  ATMOSPHERE_CASES,
  atmosphereCamera,
  atmosphereSun,
  summarizeAtmosphere,
  validateAtmosphereSamples,
} from "./atmosphere-cases";

function half(bits: number): number {
  const sign = bits & 0x8000 ? -1 : 1;
  const exponent = (bits >> 10) & 31;
  const mantissa = bits & 1023;
  return (
    sign *
    (exponent === 31
      ? mantissa
        ? NaN
        : Infinity
      : exponent === 0
        ? mantissa * 2 ** -24
        : (1 + mantissa / 1024) * 2 ** (exponent - 15))
  );
}
/** Copy immediately after render, before an await can expire the swapchain image. */
async function readFrame(renderer: WebGPURenderer) {
  const { device, sceneColor, context } = renderer as unknown as {
    device: GPUDevice;
    sceneColor: GPUTexture;
    context: GPUCanvasContext;
  };
  const swapchain = context.getCurrentTexture();
  const width = sceneColor.width,
    height = sceneColor.height;
  if (width !== 320 || height !== 180 || swapchain.width !== width || swapchain.height !== height)
    throw Error("Atmosphere smoke must remain exactly 320×180");
  const hdrRow = Math.ceil((width * 8) / 256) * 256,
    displayRow = Math.ceil((width * 4) / 256) * 256;
  const hdr = device.createBuffer({
    size: hdrRow * height,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  const display = device.createBuffer({
    size: displayRow * height,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  try {
    const encoder = device.createCommandEncoder();
    encoder.copyTextureToBuffer({ texture: sceneColor }, { buffer: hdr, bytesPerRow: hdrRow }, [
      width,
      height,
    ]);
    encoder.copyTextureToBuffer({ texture: swapchain }, { buffer: display, bytesPerRow: displayRow }, [
      width,
      height,
    ]);
    device.queue.submit([encoder.finish()]);
    await Promise.all([hdr.mapAsync(GPUMapMode.READ), display.mapAsync(GPUMapMode.READ)]);
    const source = new Uint16Array(hdr.getMappedRange()),
      shown = new Uint8Array(display.getMappedRange());
    const pixels = new Float32Array(width * height * 4),
      displayPixels = new Uint8Array(pixels.length);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width * 4; x++) pixels[y * width * 4 + x] = half(source[(y * hdrRow) / 2 + x]);
      displayPixels.set(shown.subarray(y * displayRow, y * displayRow + width * 4), y * width * 4);
    }
    hdr.unmap();
    display.unmap();
    return { pixels, displayPixels };
  } finally {
    hdr.destroy();
    display.destroy();
  }
}
export async function verifyAtmosphereEdges() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing acceptance canvas");
  canvas.style.width = "320px";
  canvas.style.height = "180px";
  const errors: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    // Exposure invariance must compare the same rays, without per-frame TAA jitter.
    antialiasing: "spatial",
    pixelRatio: 1,
    onDiagnostic: (diagnostic) => errors.push(`${diagnostic.code}: ${diagnostic.message}`),
  });
  const images = new Map<string, Float32Array>();
  const samples = [];
  try {
    if (/swiftshader|llvmpipe|software/i.test(renderer.measurements.adapter))
      throw Error("Hardware adapter required");
    for (const item of ATMOSPHERE_CASES) {
      const scene: EvaluatedScene = {
        surfaces: [],
        camera: atmosphereCamera(item.height),
        time: 0,
        mode: "beauty",
        grid: false,
        environment: {
          sunDirection: atmosphereSun(item.sunElevation),
          sunColor: [1, 1, 1],
          sunIntensity: 1,
          ambient: 1,
          skyColor: [0, 0, 0],
          horizonColor: [0, 0, 0],
          groundColor: [0.3, 0.3, 0.3],
          fogDensity: 0,
          wind: [0, 0, 0],
          exposure: item.exposure,
        },
      };
      renderer.render(scene);
      const image = await readFrame(renderer);
      if (!renderer.completeness.complete) throw Error(`${item.id}: incomplete atmosphere frame`);
      samples.push(summarizeAtmosphere(item.id, image.pixels, image.displayPixels));
      images.set(item.id, image.pixels);
    }
    const low = images.get("exposure-low"),
      high = images.get("exposure-high");
    if (!low || !high) throw Error("Missing exposure images");
    let error = 0,
      energy = 0;
    for (let i = 0; i < low.length; i++)
      if (i % 4 !== 3) {
        error += (low[i] - high[i]) ** 2;
        energy += low[i] ** 2;
      }
    const exposureRelativeRms = Math.sqrt(error / Math.max(energy, 1e-20));
    const checks = validateAtmosphereSamples(samples, exposureRelativeRms);
    if (errors.length) throw Error(errors.join("\n"));
    return {
      adapter: renderer.measurements.adapter,
      width: 320,
      height: 180,
      frames: ATMOSPHERE_CASES.length,
      samples,
      exposureRelativeRms,
      checks,
      errors,
      scope:
        "Behavioral edge acceptance of unchanged production sky/render/display; not radiometric reference accuracy.",
    };
  } finally {
    renderer.dispose();
  }
}
