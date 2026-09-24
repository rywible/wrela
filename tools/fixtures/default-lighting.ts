import { referenceProject } from "@wrela/examples";
import type { EvaluatedScene, GpuFrameTiming, Vec3 } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost, evaluateEnvironment, SkyVisibilityCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectAlpineFixture, indirectBoxFixture } from "./indirect-scenes";

function readLinear(renderer: WebGPURenderer) {
  const source = renderer as unknown as {
    device: GPUDevice;
    sceneColor?: GPUTexture;
    targets?: { sceneColor?: GPUTexture };
  };
  return linearImage({
    device: source.device,
    sceneColor: source.sceneColor ?? source.targets?.sceneColor,
  } as unknown as WebGPURenderer);
}

async function png(renderer: WebGPURenderer) {
  const bytes = new Uint8Array(await (await renderer.capture()).arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(binary);
}
function compare(a: Float32Array, b: Float32Array) {
  let maximum = 0,
    sum = 0,
    before = 0,
    after = 0;
  for (let i = 0; i < a.length; i++)
    if (i % 4 !== 3) {
      if (!Number.isFinite(a[i]) || !Number.isFinite(b[i])) throw Error("Nonfinite lighting");
      maximum = Math.max(maximum, Math.abs(a[i] - b[i]));
      sum += Math.abs(a[i] - b[i]);
      before += a[i];
      after += b[i];
    }
  return {
    maximum,
    mean: sum / (a.length * 0.75),
    beforeMean: before / (a.length * 0.75),
    afterMean: after / (a.length * 0.75),
  };
}
export async function defaultLightingFixture(Previous: typeof WebGPURenderer) {
  const errors: string[] = [],
    renderers: WebGPURenderer[] = [];
  for (const type of [Previous, WebGPURenderer]) {
    const canvas = document.createElement("canvas");
    canvas.style.width = "960px";
    canvas.style.height = "720px";
    document.body.append(canvas);
    renderers.push(
      await type.create(canvas, {
        quality: "balanced",
        pixelRatio: 1,
        antialiasing: "spatial",
        onDiagnostic: (d) => {
          if (d.severity === "error") errors.push(d.message);
        },
      }),
    );
  }
  const render = async (r: WebGPURenderer, scene: EvaluatedScene, count = 5) => {
    for (let i = 0; i < count; i++) {
      r.render(scene);
      await r.flushGpuTimings();
    }
    if (!r.completeness.complete || errors.length)
      throw Error(JSON.stringify({ errors, complete: r.completeness }));
  };
  return {
    async capture(kind: "alpine" | "winter" | "room" | "closed") {
      let scene: EvaluatedScene, host: BrowserSceneHost | undefined;
      const cache = new SkyVisibilityCache();
      if (kind === "winter") {
        const project = referenceProject();
        host = new BrowserSceneHost(project);
        await host.prepare(project.entry, "neutral-stage", "interactive");
        const camera = { position: [8, 4.2, 11] as Vec3, target: [0.8, 1, 0.5] as Vec3, fov: 48 };
        host.updateView(camera);
        await host.world?.prepare();
        scene = host.extract(camera, "beauty");
      } else {
        const f =
          kind === "alpine"
            ? indirectAlpineFixture()
            : indirectBoxFixture({ ceiling: kind === "closed", front: kind === "closed", occluder: false });
        scene = { ...f, environment: evaluateEnvironment(), mode: "beauty", grid: false, time: 0 };
        scene.environment.sunDirection = f.lighting.sunDirection;
        if (kind === "closed") scene.camera = { position: [0, 1, 0.65], target: [0, 0.8, -0.6], fov: 65 };
      }
      scene.grid = false;
      try {
        const original = scene.surfaces;
        if (host) {
          await host.waitForSkyVisibility();
          scene = host.extract(scene.camera, "beauty");
          if (!scene.surfaces.some((s) => s.mesh.skyVisibility) || scene.indirectLighting)
            throw Error("Default host lighting missing");
        } else {
          cache.apply(scene);
          await cache.waitReady();
          cache.apply(scene);
        }
        const lit = scene,
          control = { ...scene, surfaces: original };
        const images: Record<string, string> = {},
          pixels: Record<string, Float32Array> = {},
          timings: Record<string, GpuFrameTiming[]> = { before: [], shadow: [], default: [] };
        const cases = {
          before: { renderer: renderers[0], scene: control },
          shadow: { renderer: renderers[1], scene: control },
          default: { renderer: renderers[1], scene: lit },
        };
        for (const name of ["before", "shadow", "default", "default", "shadow", "before"] as const) {
          const c = cases[name];
          await render(c.renderer, c.scene, 12);
          c.renderer.drainGpuTimings();
          for (let i = 0; i < 8; i++) {
            for (let burst = 0; burst < 4; burst++) c.renderer.render(c.scene);
            await c.renderer.flushGpuTimings();
            timings[name].push(...c.renderer.drainGpuTimings());
          }
          pixels[name] = await readLinear(c.renderer);
          images[name] = await png(c.renderer);
        }
        const stable = host?.skyVisibilityBuilds ?? cache.builds;
        scene.environment.sunIntensity *= 0.6;
        if (host) host.applyIndirectLighting(scene);
        else cache.apply(scene);
        const reuse = (host?.skyVisibilityBuilds ?? cache.builds) === stable;
        const skyReport = host?.skyVisibilityReport ?? cache.report;
        let streaming: unknown;
        if (host) {
          const builds = host.skyVisibilityBuilds;
          const camera = {
            ...scene.camera,
            position: [
              scene.camera.position[0] + 24,
              scene.camera.position[1],
              scene.camera.position[2],
            ] as Vec3,
          };
          host.updateView(camera);
          await host.world?.prepare();
          const moved = host.extract(camera);
          await host.waitForSkyVisibility();
          const settled = host.extract(camera);
          streaming = {
            rebuilt: host.skyVisibilityBuilds !== builds,
            sky: host.skyVisibilityReport,
            ready: !!settled.surfaces.find((s) => s.mesh.skyVisibility),
            indirect: !!moved.indirectLighting,
          };
          await render(renderers[1], settled, 5);
          images.pan = await png(renderers[1]);
        }
        const shadow = compare(pixels.before, pixels.shadow),
          upgrade = compare(pixels.before, pixels.default);
        if (shadow.maximum > 0.002)
          throw Error("Optimization changed lighting beyond the precision tolerance");
        if (kind === "closed" && upgrade.afterMean > upgrade.beforeMean * 0.15)
          throw Error("Enclosed room still receives excessive sky");
        if (!reuse) throw Error("Lighting edit rebuilt geometry visibility");
        return {
          images,
          report: {
            kind,
            adapter: renderers[1].measurements.adapter,
            viewport: [960, 720],
            sky: skyReport,
            streaming,
            receivers: lit.surfaces
              .filter((s) => s.mesh.skyVisibility)
              .map((s) => {
                const a = s.mesh.skyVisibility;
                if (!a) throw Error("Missing receiver visibility");
                let min = 1,
                  max = 0,
                  mean = 0;
                for (let i = 0; i < a.length; i += 4) {
                  const v = Math.hypot(a[i], a[i + 1], a[i + 2]);
                  min = Math.min(min, v);
                  max = Math.max(max, v);
                  mean += v;
                }
                return {
                  id: s.id,
                  vertices: a.length / 4,
                  min,
                  max,
                  mean: mean / (a.length / 4),
                  first: Array.from(a.slice(0, 4)),
                  normal: Array.from(s.mesh.normals.slice(0, 3)),
                  position: Array.from(s.mesh.positions.slice(0, 3)),
                  matrix: Array.from(s.matrix),
                };
              }),
            shadow,
            upgrade,
            reuse,
            timings,
            errors,
          },
        };
      } finally {
        cache.dispose();
        host?.dispose();
      }
    },
    dispose() {
      for (const r of renderers) r.dispose();
    },
  };
}
