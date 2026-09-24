import { type CreatureFixtureId, createCreatureFixture } from "@wrela/examples";
import { type Camera, contentKey, type EvaluatedScene, type Project, VIEW_MODES } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { applyCreatureInspection, BrowserSceneHost, runtimeDiagnostics } from "@wrela/runtime";
import { overlayCapture } from "../../apps/studio/src/capture";

/** Explicit, finite render fixture: no animation loop, no world streaming, no water. */
export async function createCreatureRenderFixture(id: CreatureFixtureId = "ash-warden", project?: Project) {
  const fixture = createCreatureFixture(id);
  if (project) fixture.project = structuredClone(project);
  const host = new BrowserSceneHost(fixture.project);
  const canvas = document.querySelector("canvas");
  if (!canvas) throw new Error("Creature capture canvas is missing");
  host.setViewportHeight(canvas.clientHeight || canvas.height);
  let renderer: WebGPURenderer | undefined;
  try {
    await host.prepare(fixture.characterId, fixture.stageId, "review");
    renderer = await WebGPURenderer.create(canvas, {
      quality: "low",
      pixelRatio: 1,
      maxGpuBytes: 64 * 1024 * 1024,
    });
  } catch (error) {
    renderer?.dispose();
    host.dispose();
    throw error;
  }
  const active = renderer;
  let disposed = false;
  return {
    async capture(
      motion = "idle",
      time = 0,
      view = "three-quarter",
      mode: EvaluatedScene["mode"] = "beauty",
      hideGroom = false,
      skeleton = false,
      options?: {
        camera?: Camera;
        light?: "neutral" | "raking" | "backlight";
        hideAttachments?: boolean;
        hideGarments?: boolean;
      },
    ) {
      if (disposed) throw new Error("Creature capture is disposed");
      if (!Number.isFinite(time) || time < 0 || time > 10)
        throw new Error("Capture time exceeds ten-second smoke budget");
      if (!VIEW_MODES.includes(mode)) throw new Error("Unknown capture mode");
      const source = fixture.project.documents.find((d) => d.id === fixture.characterId);
      if (source?.kind !== "character") throw new Error("Missing creature source");
      if (!source.motions.some((m) => m.id === motion)) throw new Error("Unknown creature motion");
      const review =
        source.creature?.reviewScenarios.find((s) => s.motion === motion) ??
        source.creature?.reviewScenarios[0];
      const camera = review?.cameras.find((c) => c.id === view) ?? review?.cameras[0];
      const studyCamera = fixture.reviewScenarios.find((scenario) => scenario.id === view)?.camera;
      const selected: Camera =
        options?.camera ??
        studyCamera ??
        (camera
          ? { position: camera.position, target: camera.target, fov: 42 }
          : { position: [5, 3, 6], target: [0, 1.2, 0], fov: 42 });
      await host.resetRuntime();
      host.runtime?.playMotion(fixture.characterId, motion, 0);
      host.advance(1 / 60, selected); // Apply queued clip selection at its zero-time pose.
      for (let tick = 0; tick < Math.ceil(time * 60); tick++) host.advance(1 / 60, selected);
      const hiddenSourceIds = [
        ...(options?.hideAttachments ? (source.creature?.attachments.flatMap((a) => a.nodeIds) ?? []) : []),
        ...(options?.hideGarments ? (source.creature?.cloth.map((cloth) => cloth.chart) ?? []) : []),
      ];
      const scene = applyCreatureInspection(host.extract(selected, mode), {
        hideGroom,
        hideSourceIds: hiddenSourceIds,
      });
      if (options?.light && options.light !== "neutral") {
        scene.environment = {
          ...scene.environment,
          sunDirection: options.light === "raking" ? [0.9, 0.3, 0.15] : [-0.4, 0.6, -1],
          pointLights: [],
        };
      }
      if (scene.surfaces.some((s) => s.water))
        throw new Error("Water is forbidden in the bounded creature smoke");
      active.render(scene);
      const raw = await active.capture();
      const rig =
        skeleton && host.runtime
          ? runtimeDiagnostics(host.runtime, { time: host.runtime.clock.time })
          : undefined;
      const blob = rig ? await overlayCapture(raw, rig.segments, scene.camera, ["rig"]) : raw;
      const data = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
      return {
        id,
        motion,
        time,
        mode,
        hideGroom,
        hiddenSourceIds,
        skeleton,
        camera: selected,
        light: options?.light ?? "neutral",
        tick: host.runtime?.clock.tick ?? 0,
        motionTick: Math.ceil(time * 60),
        runtimeTime: host.runtime?.clock.time,
        source: contentKey(fixture.project),
        image: data,
        measurements: active.measurements,
        completeness: active.completeness,
        resources: host.resourceUsage,
        diagnostics: [...host.diagnostics, ...active.diagnostics],
        scope: "one bounded creature view; not AAA approval or performance certification",
      };
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      active.dispose();
      host.dispose();
    },
  };
}
