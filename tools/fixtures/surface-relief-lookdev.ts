import {
  createSurfaceReliefLookdevProject,
  SURFACE_RELIEF_CAMERAS,
} from "@wrela/examples/surface-relief-lookdev";
import type { EvaluatedScene, MeshData } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { projectedGeometryError } from "@wrela/render-webgpu/realization";
import { BrowserSceneHost } from "@wrela/runtime";

export const SURFACE_RELIEF_SHOTS = [
  "near-clay",
  "bark-clay",
  "bark-silhouette",
  "stone-clay",
  "stone-silhouette",
  "bark-grazing",
  "stone-grazing",
  "far-clay",
  "far-silhouette",
] as const;
export type SurfaceReliefShot = (typeof SURFACE_RELIEF_SHOTS)[number];
const percentile = (values: number[], fraction: number): number | null =>
  values.length
    ? [...values].sort((a, b) => a - b)[Math.min(values.length - 1, Math.floor(values.length * fraction))]
    : null;
const meshSummary = (mesh: MeshData) => ({
  vertices: mesh.positions.length / 3,
  triangles: mesh.indices.length / 3,
  bounds: mesh.bounds,
  bytes: mesh.positions.byteLength + mesh.normals.byteLength + mesh.indices.byteLength,
  fidelity: mesh.fidelity,
});

/** All geometry/material realization comes from authored documents through the production scene host. */
export async function createSurfaceReliefLookdevFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw new Error("Surface-relief canvas missing");
  const host = new BrowserSceneHost(createSurfaceReliefLookdevProject(false));
  host.setViewportHeight(canvas.height);
  const failures: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") failures.push(diagnostic.message);
    },
  });
  let prepared = "";
  const silhouetteMasks = new Map<string, Uint8Array>();
  return {
    async capture(displaced: boolean, shot: SurfaceReliefShot, frames = 12, forceNear = false) {
      if (!SURFACE_RELIEF_SHOTS.includes(shot) || !Number.isInteger(frames) || frames < 4 || frames > 60)
        throw new Error("Invalid relief capture settings");
      const view = shot.startsWith("bark")
        ? "bark"
        : shot.startsWith("stone")
          ? "stone"
          : shot.startsWith("far")
            ? "far"
            : "near";
      const camera = structuredClone(SURFACE_RELIEF_CAMERAS[view]);
      const grazing = shot.endsWith("grazing");
      const mode: EvaluatedScene["mode"] = shot.endsWith("silhouette")
        ? "silhouette"
        : grazing
          ? "beauty"
          : "clay";
      const subject =
        view === "bark" || view === "stone" ? `surface-relief-${view}-specimen` : "surface-relief-specimens";
      const next = `${displaced}:${grazing}:${subject}`;
      let preparationMs = 0;
      if (prepared !== next) {
        const begin = performance.now();
        await host.setProject(createSurfaceReliefLookdevProject(displaced, grazing));
        await host.prepare(subject, "surface-relief-stage", "review");
        preparationMs = performance.now() - begin;
        prepared = next;
      }
      host.updateView(camera);
      const extractionStart = performance.now();
      const scene = host.extract(camera, mode);
      if (forceNear)
        scene.surfaces = scene.surfaces.map((surface) => ({
          ...surface,
          renderProducts: surface.renderProducts?.filter((product) => product.kind === "direct-mesh"),
        }));
      const extractionMs = performance.now() - extractionStart;
      for (let warmup = 0; warmup < 24; warmup++) {
        renderer.render(scene);
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        if (warmup >= 3 && renderer.completeness.complete) break;
      }
      if (!renderer.completeness.complete) throw new Error("Relief specimen did not become fully resident");
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      const firstFrame = renderer.measurements.frame + 1;
      const cpu: number[] = [];
      const gpuFrames: ReturnType<WebGPURenderer["drainGpuTimings"]> = [];
      for (let index = 0; index < frames; index++) {
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        renderer.render(scene);
        cpu.push(renderer.measurements.cpuMs);
        gpuFrames.push(...renderer.drainGpuTimings());
      }
      await renderer.flushGpuTimings();
      gpuFrames.push(...renderer.drainGpuTimings());
      const lastFrame = renderer.measurements.frame;
      const measured = gpuFrames.filter((frame) => frame.frame >= firstFrame && frame.frame <= lastFrame);
      const gpu = measured.map((frame) => frame.gpuMs);
      const blob = await renderer.capture();
      const image = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
      let silhouette:
        | { occupiedPixels: number; changedPixels: number | null; relativeChangedArea: number | null }
        | undefined;
      if (mode === "silhouette") {
        const bitmap = await createImageBitmap(blob);
        const pixels = document.createElement("canvas");
        pixels.width = bitmap.width;
        pixels.height = bitmap.height;
        const context = pixels.getContext("2d");
        if (!context) throw new Error("Silhouette measurement unavailable");
        context.drawImage(bitmap, 0, 0);
        bitmap.close();
        const rgba = context.getImageData(0, 0, pixels.width, pixels.height).data;
        const mask = new Uint8Array(pixels.width * pixels.height);
        let occupiedPixels = 0;
        for (let index = 0; index < mask.length; index++) {
          mask[index] = rgba[index * 4] > 127 ? 1 : 0;
          occupiedPixels += mask[index];
        }
        const smooth = silhouetteMasks.get(shot);
        let changedPixels: number | null = null;
        if (!displaced) silhouetteMasks.set(shot, mask);
        else if (smooth) {
          changedPixels = 0;
          for (let index = 0; index < mask.length; index++)
            changedPixels += Number(mask[index] !== smooth[index]);
        }
        silhouette = {
          occupiedPixels,
          changedPixels,
          relativeChangedArea: changedPixels === null ? null : changedPixels / Math.max(1, occupiedPixels),
        };
      }
      const geometry = scene.surfaces.map((surface) => {
        const selection = renderer.completeness.realizations?.find((entry) => entry.id === surface.id);
        const selected = surface.renderProducts?.find((product) => product.key === selection?.key);
        return {
          id: surface.id,
          source: surface.source,
          mesh: meshSummary(surface.mesh),
          selected: selection,
          selectedMesh: meshSummary(selected?.kind === "parametric-mesh" ? selected.mesh : surface.mesh),
          candidates: surface.renderProducts?.map((product) => ({
            ...product,
            projectedGeometryErrors: product.errors.flatMap((error) =>
              (error.kind === "numeric-bound" || error.kind === "real-bound") &&
              (error.metric === "depth" || error.metric === "silhouette")
                ? [
                    {
                      metric: error.metric,
                      maximumLocalMetres: error.maximum,
                      maximumPixels: projectedGeometryError(scene, surface, error.maximum, canvas.height),
                    },
                  ]
                : [],
            ),
            ...(product.kind === "parametric-mesh" ? { mesh: meshSummary(product.mesh) } : {}),
          })),
        };
      });
      return {
        image,
        choice: forceNear ? "forced-near" : "automatic",
        selectedGeometryBytes: geometry.reduce((sum, surface) => sum + surface.selectedMesh.bytes, 0),
        variant: displaced ? "physical-relief" : "smooth",
        shot,
        camera,
        mode,
        geometry,
        reliefReviews: host.surfaceReliefReviews,
        resourceUsage: host.resourceUsage,
        timing: {
          scope: "Short settled-frame observations, not an isolated throughput benchmark",
          preparationMs,
          extractionMs,
          cpuP50Ms: percentile(cpu, 0.5),
          cpuP95Ms: percentile(cpu, 0.95),
          gpuP50Ms: percentile(gpu, 0.5),
          gpuP95Ms: percentile(gpu, 0.95),
          requestedFrames: frames,
          gpuSamples: measured.length,
          firstFrame,
          lastFrame,
          gpuFrames: measured,
        },
        silhouette,
        measurements: renderer.measurements,
        completeness: renderer.completeness,
        diagnostics: [...host.diagnostics, ...renderer.diagnostics],
        failures,
      };
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
