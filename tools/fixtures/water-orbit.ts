import { createWaterValley, waterValleyCameras } from "@wrela/examples/water-valley";
import type { Camera } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";

/** Ten seconds of presented camera motion, including a conserved interaction impulse. */
export async function recordWaterOrbit() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing review canvas");
  const project = createWaterValley(),
    host = new BrowserSceneHost(project);
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    antialiasing: "temporal",
    pixelRatio: 1,
  });
  const errors: string[] = [];
  const device = (renderer as unknown as { device: GPUDevice }).device;
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  const stream = canvas.captureStream(30),
    chunks: Blob[] = [];
  const recorder = new MediaRecorder(stream, {
    mimeType: "video/webm;codecs=vp9",
    videoBitsPerSecond: 3_000_000,
  });
  recorder.addEventListener("dataavailable", (event) => chunks.push(event.data));
  try {
    const base = waterValleyCameras.bank;
    host.updateView(base);
    await host.prepare(project.entry, "neutral-stage", "review");
    await host.world?.prepare();
    await host.seek(3);
    for (let i = 0; i < 20; i++) {
      renderer.render(host.extract(base));
      await renderer.flushGpuTimings();
    }
    if (!renderer.completeness.complete) throw Error("Orbit began before residency completed");
    const body = host.runtime?.waterBodies.get("water-study-creek");
    if (!body?.simulation) throw Error("Missing interactive water body");
    let frames = 0,
      interaction = false,
      impulseVolumeError = NaN;
    const captures: { time: number; image: string }[] = [];
    recorder.start();
    const started = performance.now();
    let previous = started,
      captureIndex = 0;
    while (performance.now() - started < 10000) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      const now = performance.now(),
        time = (now - started) / 1000;
      const angle = -0.25 + time * 0.07,
        dx = base.position[0] - base.target[0],
        dz = base.position[2] - base.target[2];
      const camera: Camera = {
        ...base,
        position: [
          base.target[0] + dx * Math.cos(angle) - dz * Math.sin(angle),
          base.position[1],
          base.target[2] + dx * Math.sin(angle) + dz * Math.cos(angle),
        ],
      };
      host.advance(Math.min(0.1, (now - previous) / 1000), camera);
      previous = now;
      if (!interaction && time >= 2) {
        const volume = body.simulation.volume;
        host.runtime!.disturbWater("water-study-creek", 0, 7, 1.4, 2.5);
        impulseVolumeError = Math.abs(body.simulation.volume - volume);
        interaction = true;
      }
      const scene = host.extract(camera);
      scene.grid = false;
      renderer.render(scene);
      frames++;
      // Capture the presented canvas without renderer.capture(), preserving temporal behavior.
      if (time >= captureIndex * 3) {
        captures.push({ time, image: canvas.toDataURL() });
        captureIndex++;
      }
    }
    const stopped = new Promise<void>((resolve) =>
      recorder.addEventListener("stop", () => resolve(), { once: true }),
    );
    recorder.stop();
    await stopped;
    await renderer.flushGpuTimings();
    if (errors.length || impulseVolumeError > 1e-9 || !interaction || frames < 30)
      throw Error(
        `Orbit acceptance failed: ${JSON.stringify({ errors, impulseVolumeError, interaction, frames })}`,
      );
    const data = new Uint8Array(await new Blob(chunks).arrayBuffer());
    let binary = "";
    for (let i = 0; i < data.length; i += 8192) binary += String.fromCharCode(...data.subarray(i, i + 8192));
    return {
      video: btoa(binary),
      captures,
      frames,
      seconds: (performance.now() - started) / 1000,
      impulseVolumeError,
      errors,
      measurements: renderer.measurements,
    };
  } finally {
    if (recorder.state !== "inactive") recorder.stop();
    for (const track of stream.getTracks()) track.stop();
    renderer.dispose();
    host.dispose();
  }
}
