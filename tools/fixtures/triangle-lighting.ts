import { referenceProject } from "@wrela/examples";
import type { EvaluatedScene, GpuFrameTiming, Vec3 } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost, evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectAlpineFixture, indirectBoxFixture } from "./indirect-scenes";

function difference(a: Float32Array, b: Float32Array) {
  let maximum = 0,
    mean = 0;
  for (let i = 0; i < a.length; i++)
    if (i % 4 !== 3) {
      const d = Math.abs(a[i] - b[i]);
      if (!Number.isFinite(d)) throw Error("Nonfinite triangle lighting");
      maximum = Math.max(maximum, d);
      mean += d;
    }
  return { maximum, mean: mean / ((a.length / 4) * 3) };
}
async function image(blob: Blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let s = "";
  for (let i = 0; i < bytes.length; i += 8192) s += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(s);
}
export async function createTriangleLightingFixture(width = 640, height = 480) {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing canvas");
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  const errors: string[] = [],
    cache = new IndirectLightingCache();
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    resolutionScale: 1,
    antialiasing: "spatial",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const render = async (scene: EvaluatedScene, count = 3) => {
    for (let i = 0; i < count; i++) {
      renderer.render(scene);
      await renderer.flushGpuTimings();
    }
    if (errors.length || !renderer.completeness.complete)
      throw Error(JSON.stringify({ errors, completeness: renderer.completeness }));
  };
  return {
    async capture(kind: "alpine" | "material" | "sealed" | "winter") {
      let scene: EvaluatedScene, host: BrowserSceneHost | undefined;
      if (kind === "winter") {
        const project = referenceProject();
        host = new BrowserSceneHost(project);
        await host.prepare(project.entry, "neutral-stage", "interactive");
        const camera = { position: [8, 4.2, 11] as Vec3, target: [0.8, 1, 0.5] as Vec3, fov: 48 };
        host.updateView(camera);
        await host.world?.prepare();
        scene = host.extract(camera);
      } else {
        const fixture =
          kind === "sealed"
            ? indirectBoxFixture({ ceiling: true, front: true, occluder: false })
            : indirectAlpineFixture();
        scene = { ...fixture, environment: evaluateEnvironment(), time: 0, mode: "beauty", grid: false };
        scene.environment.sunDirection = fixture.lighting.sunDirection;
        if (kind === "material")
          for (const s of scene.surfaces) {
            s.material.normalStrength = 0.5;
            s.material.pattern = 3;
            s.material.scale = 2;
            s.material.metallic = 0.3;
            s.material.roughness = 0.4;
          }
        if (kind === "sealed") scene.camera = { position: [0, 1, 0.65], target: [0, 0.8, -0.6], fov: 65 };
      }
      scene.grid = false;
      scene.time = 0;
      scene.mode = "beauty";
      try {
        cache.update(scene, {
          dimensions: kind === "winter" ? [12, 6, 12] : [8, 6, 8],
          samples: 64,
          skySamples: 4,
          maxTriangles: 180000,
          ...(kind === "winter" ? { cameraVolume: { radius: 24, halfHeight: 6, snap: 8 } } : {}),
        });
        const field = await cache.waitReady();
        const control = {
          ...field,
          key: `${field.key}/reference`,
          triangleCache: undefined,
          visibility: field.visibility
            ? {
                ...field.visibility,
                cells: field.visibility.cells?.slice(
                  0,
                  (field.visibility.cells?.length ?? 0) - (field.triangleCache?.report.regionBytes ?? 0) / 4,
                ),
              }
            : undefined,
        };
        const memory = { cpuFieldBytes: cache.byteLength, controlGpuBytes: 0, compiledGpuBytes: 0 };
        const timings: { control: GpuFrameTiming[]; compiled: GpuFrameTiming[] } = {
          control: [],
          compiled: [],
        };
        const images: Record<string, string> = {},
          comparisons: Record<string, ReturnType<typeof difference>> = {};
        const pixels: Record<string, Float32Array> = {};
        await render(scene);
        images.environment = await image(await renderer.capture());
        for (const condition of ["control", "compiled", "compiled", "control"] as const) {
          scene.indirectLighting = condition === "control" ? control : field;
          await render(scene, 5);
          renderer.drainGpuTimings();
          for (let i = 0; i < 12; i++) {
            await render(scene, 1);
            timings[condition].push(...renderer.drainGpuTimings());
          }
          pixels[condition] = await linearImage(renderer);
          images[condition] = await image(await renderer.capture());
          const memoryKey = condition === "control" ? "controlGpuBytes" : "compiledGpuBytes";
          if (!memory[memoryKey]) memory[memoryKey] = renderer.measurements.gpuBytes;
        }
        comparisons.beauty = difference(pixels.control, pixels.compiled);
        if (kind === "winter" && field.triangleCache) {
          scene.indirectLighting = {
            ...field,
            key: `${field.key}/geometry-only`,
            triangleCache: {
              ...field.triangleCache,
              sources: field.triangleCache.sources.map((s) => ({
                ...s,
                mesh: {
                  ...s.mesh,
                  positions: s.mesh.positions.slice(),
                  indirectProofs: new Float32Array(s.mesh.indirectProofs?.length ?? 0),
                },
              })),
            },
          };
          await render(scene);
          const geometryOnly = await linearImage(renderer);
          comparisons.geometryOnly = difference(pixels.control, geometryOnly);
          comparisons.proofOnly = difference(geometryOnly, pixels.compiled);
        }
        scene.indirectLighting = field;
        await render(scene);
        const admittedSurfaces =
          renderer.realizedScene?.surfaces.filter((s) => s.mesh.indirectProofs).length ?? 0;
        scene.mode = "indirect-cache";
        await render(scene);
        images.coverage = await image(await renderer.capture());
        const coverage = await linearImage(renderer);
        let green = 0,
          magenta = 0;
        for (let i = 0; i < coverage.length; i += 4) {
          if (coverage[i + 1] > 0.8 && coverage[i] < 0.2) green++;
          if (coverage[i] > 0.8 && coverage[i + 2] > 0.8 && coverage[i + 1] < 0.2) magenta++;
        }
        scene.mode = "beauty";
        let fullReference:
          | {
              changedChannels: number;
              controlError: number;
              compiledError: number;
              controlMaximum: number;
              compiledMaximum: number;
            }
          | undefined;
        if (kind === "winter") {
          const referenceCanvas = document.createElement("canvas");
          referenceCanvas.style.width = `${width}px`;
          referenceCanvas.style.height = `${height}px`;
          document.body.append(referenceCanvas);
          const reference = await WebGPURenderer.create(referenceCanvas, {
            quality: "balanced",
            pixelRatio: 1,
            resolutionScale: 1,
            antialiasing: "spatial",
            indirectVisibilityReference: true,
            onDiagnostic: (d) => {
              if (d.severity === "error") errors.push(d.message);
            },
          });
          try {
            const referenceScene = {
              ...scene,
              indirectLighting: {
                ...control,
                surfaceCache: undefined,
                visibility: control.visibility ? { ...control.visibility, cells: undefined } : undefined,
              },
            };
            for (let i = 0; i < 5; i++) {
              reference.render(referenceScene);
              await reference.flushGpuTimings();
            }
            if (!reference.completeness.complete) throw Error("Incomplete geometry reference");
            const truth = await linearImage(reference);
            images.reference = await image(await reference.capture());
            fullReference = {
              changedChannels: 0,
              controlError: 0,
              compiledError: 0,
              controlMaximum: 0,
              compiledMaximum: 0,
            };
            for (let i = 0; i < truth.length; i++)
              if (i % 4 !== 3 && Math.abs(pixels.control[i] - pixels.compiled[i]) > 0.00001) {
                fullReference.changedChannels++;
                const a = Math.abs(truth[i] - pixels.control[i]),
                  b = Math.abs(truth[i] - pixels.compiled[i]);
                fullReference.controlError += a;
                fullReference.compiledError += b;
                fullReference.controlMaximum = Math.max(fullReference.controlMaximum, a);
                fullReference.compiledMaximum = Math.max(fullReference.compiledMaximum, b);
              }
          } finally {
            reference.dispose();
            referenceCanvas.remove();
          }
        }
        for (const variant of ["moved", "relit", "rebased"]) {
          if (variant === "moved")
            scene.camera = {
              ...scene.camera,
              position: scene.camera.position.map((v, a) => v + (a === 0 ? 0.17 : 0)) as Vec3,
            };
          if (variant === "relit") scene.environment.sunIntensity *= 0.6;
          if (variant === "rebased") {
            const shift: Vec3 = [16, 0, -32];
            scene = {
              ...scene,
              origin: (scene.origin ?? [0, 0, 0]).map((v, a) => v + shift[a]) as Vec3,
              camera: {
                ...scene.camera,
                position: scene.camera.position.map((v, a) => v - shift[a]) as Vec3,
                target: scene.camera.target.map((v, a) => v - shift[a]) as Vec3,
              },
              surfaces: scene.surfaces.map((s) => {
                const matrix = s.matrix.slice();
                for (let a = 0; a < 3; a++) matrix[12 + a] -= shift[a];
                return { ...s, matrix };
              }),
            };
          }
          scene.indirectLighting = control;
          await render(scene);
          const a = await linearImage(renderer);
          scene.indirectLighting = field;
          await render(scene);
          comparisons[variant] = difference(a, await linearImage(renderer));
        }
        const failures = Object.entries(comparisons)
          .filter(([, d]) => d.maximum > 0.002 || d.mean > 0.00001)
          .map(([name]) => `${kind}/${name} changed lighting`);
        return {
          images,
          report: {
            kind,
            adapter: renderer.measurements.adapter,
            viewport: [width, height],
            field: field.report,
            memory,
            triangles: field.triangleCache?.report,
            admittedSurfaces,
            coverage: { green, magenta, fraction: green / Math.max(1, green + magenta) },
            comparisons,
            fullReference,
            timings,
            errors: [...errors, ...failures],
          },
        };
      } finally {
        host?.dispose();
      }
    },
    dispose() {
      cache.dispose();
      renderer.dispose();
    },
  };
}
