import { createWaterLookdev, waterStudyCameras } from "@wrela/examples/water-lookdev";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";

/** Read presented frames without capture(), which deliberately disables temporal rendering. */
export async function checkWaterMotion() {
  const canvas = document.querySelector("canvas")!;
  const cases = [];
  const images: Record<string, string> = {};
  for (const subject of ["ocean", "creek"] as const) {
    for (const antialiasing of [undefined, "temporal", "spatial"] as const) {
      const project = createWaterLookdev(subject);
      const host = new BrowserSceneHost(project);
      await host.prepare(project.entry, "neutral-stage");
      await host.seek(5);
      const camera = waterStudyCameras[project.entry];
      const renderer = await WebGPURenderer.create(canvas, {
        antialiasing,
        quality: "balanced",
        pixelRatio: 1,
      });
      const errors: string[] = [];
      const internals = renderer as unknown as {
        device: GPUDevice;
        context: GPUCanvasContext;
        format: string;
      };
      const device = internals.device;
      device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
      let previous: Uint8Array | undefined;
      const changes = [];
      let readback: GPUBuffer | undefined;
      try {
        for (let frame = 0; frame < 48; frame++) {
          if (frame >= 32) host.advance(1 / 60, camera);
          const scene = host.extract(camera);
          scene.grid = false;
          renderer.render(scene);
          const width = canvas.width,
            height = canvas.height,
            rowBytes = Math.ceil((width * 4) / 256) * 256;
          readback ??= device.createBuffer({
            size: rowBytes * height,
            usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
          });
          const encoder = device.createCommandEncoder();
          encoder.copyTextureToBuffer(
            { texture: internals.context.getCurrentTexture() },
            { buffer: readback, bytesPerRow: rowBytes },
            [width, height],
          );
          device.queue.submit([encoder.finish()]);
          await readback.mapAsync(GPUMapMode.READ);
          const raw = new Uint8Array(readback.getMappedRange());
          const pixels = new Uint8Array(width * height * 4);
          for (let y = 0; y < height; y++)
            pixels.set(raw.subarray(y * rowBytes, y * rowBytes + width * 4), y * width * 4);
          readback.unmap();
          if (previous && frame >= 16) {
            let sum = 0,
              high = 0,
              top = 0,
              bottom = 0,
              count = 0;
            for (let y = 0; y < height; y++)
              for (let x = 0; x < width; x++) {
                let delta = 0;
                const at = (y * width + x) * 4;
                for (let c = 0; c < 3; c++) delta += Math.abs(pixels[at + c] - previous[at + c]) / 3;
                sum += delta;
                if (delta > 20) high++;
                if (y < height / 3) top += delta;
                if (y > (height * 2) / 3) {
                  bottom += delta;
                  count++;
                }
              }
            changes.push({
              frame,
              phase: frame < 32 ? "frozen" : "moving",
              mean: sum / (width * height),
              changedFraction: high / (width * height),
              sky: top / (width * Math.ceil(height / 3)),
              water: bottom / count,
            });
          }
          if (frame === 16 || frame === 17 || frame === 18) {
            const output = document.createElement("canvas");
            output.width = width;
            output.height = height;
            const ctx = output.getContext("2d")!;
            const image = ctx.createImageData(width, height);
            const bgra = internals.format.startsWith("bgra");
            for (let i = 0; i < pixels.length; i += 4) {
              image.data[i] = pixels[i + (bgra ? 2 : 0)];
              image.data[i + 1] = pixels[i + 1];
              image.data[i + 2] = pixels[i + (bgra ? 0 : 2)];
              image.data[i + 3] = 255;
            }
            ctx.putImageData(image, 0, 0);
            images[`${subject}-${antialiasing ?? "default"}-${frame}`] = output.toDataURL();
          }
          previous = pixels;
        }
        cases.push({
          subject,
          antialiasing: antialiasing ?? "default",
          changes,
          errors,
          measurements: renderer.measurements,
        });
      } finally {
        readback?.destroy();
        renderer.dispose();
        host.dispose();
      }
    }
  }
  return { cases, images };
}
