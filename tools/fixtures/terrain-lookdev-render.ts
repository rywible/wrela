import type { Camera, EvaluatedScene, Project } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";

export async function createTerrainLookdevRenderer(project: Project, worldId: string) {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw new Error("Capture canvas missing");
  const host = new BrowserSceneHost(project);
  host.setViewportHeight(canvas.clientHeight || canvas.height);
  host.updateView({ position: [14, 7, -24], target: [-6, 2, 4], fov: 48 });
  await host.prepare(worldId, undefined, "review");
  const renderer = await WebGPURenderer.create(canvas, { quality: "balanced", pixelRatio: 1 });
  return {
    async prepare(nextWorldId: string) {
      await host.prepare(nextWorldId, undefined, "review");
    },
    async capture(camera: Camera, mode: EvaluatedScene["mode"] = "beauty") {
      host.updateView(camera);
      await host.teleport(camera.position);
      const scene = host.extract(camera, mode);
      for (let frame = 0; frame < 32; frame++) {
        renderer.render(scene);
        if (renderer.completeness.complete) break;
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      }
      const blob = await renderer.capture();
      const image = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
      return {
        image,
        camera,
        completeness: renderer.completeness,
        measurements: renderer.measurements,
        diagnostics: [...host.diagnostics, ...renderer.diagnostics],
        surfaces: scene.surfaces.length,
      };
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
