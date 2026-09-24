import { type Camera, contentKey, type EvaluatedScene, type Project } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
export async function renderSource(
  project: Project,
  target: string,
  stage: string | undefined,
  camera: Camera,
  width: number,
  height: number,
) {
  if (typeof document === "undefined" || typeof navigator === "undefined" || !navigator.gpu)
    throw Error("Hardware browser review is required");
  if (stage && project.documents.find((d) => d.id === stage)?.kind !== "stage")
    throw Error(`Review stage ${stage} is missing; implicit fallback is forbidden`);
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  canvas.style.position = "fixed";
  canvas.style.left = "-10000px";
  document.body.append(canvas);
  let host = new BrowserSceneHost(project);
  const errors: string[] = [];
  let renderer: WebGPURenderer | undefined;
  try {
    host.updateView(camera);
    await host.prepare(target, stage, "review");
    renderer = await WebGPURenderer.create(canvas, {
      pixelRatio: 1,
      quality: "balanced",
      antialiasing: "spatial",
      onDiagnostic: (d) => {
        if (d.severity === "error") errors.push(d.message);
      },
    });
    if (/swiftshader|software|llvmpipe/i.test(renderer.measurements.adapter))
      throw Error("Software adapters cannot qualify a hardware review");
    const active = renderer;
    return {
      get host() {
        return host;
      },
      renderer: active,
      errors,
      canvas,
      async replaceSource(next: Project, subject: string, nextStage: string | undefined, nextCamera: Camera) {
        if (nextStage && next.documents.find((d) => d.id === nextStage)?.kind !== "stage")
          throw Error(`Review stage ${nextStage} is missing; implicit fallback is forbidden`);
        const candidate = new BrowserSceneHost(next);
        try {
          candidate.updateView(nextCamera);
          await candidate.prepare(subject, nextStage, "review");
        } catch (error) {
          candidate.dispose();
          throw error;
        }
        host.dispose();
        host = candidate;
        project = next;
        target = subject;
        camera = nextCamera;
      },
      setCamera(value: Camera) {
        camera = value;
        host.updateView(value);
      },
      async capture(mode: EvaluatedScene["mode"], tick: number) {
        await host.seek(tick / 60);
        if (host.world) await host.world.prepare();
        const scene = host.extract(camera, mode);
        scene.grid = false;
        if (mode === "silhouette" && project.documents.find((d) => d.id === target)?.kind !== "world") {
          scene.surfaces = scene.surfaces.filter((s) => s.source === target);
          if (!scene.surfaces.length) throw Error("Silhouette subject has no realized surfaces");
        }
        let blob: Blob | undefined;
        for (let i = 0; i < 8; i++) {
          active.render(scene);
          blob = await active.capture();
          if (active.completeness.complete) break;
        }
        if (!blob || !active.completeness.complete || errors.length)
          throw Error(`Incomplete review capture: ${errors.join("; ")}`);
        return blob;
      },
      dispose() {
        active.dispose();
        host.dispose();
        canvas.remove();
      },
    };
  } catch (e) {
    renderer?.dispose();
    host.dispose();
    canvas.remove();
    throw e;
  }
}
export async function silhouetteMask(blob: Blob, width: number, height: number) {
  const image = await createImageBitmap(blob),
    canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw Error("Image comparison canvas unavailable");
  context.drawImage(image, 0, 0);
  image.close();
  const pixels = context.getImageData(0, 0, width, height).data;
  return Uint8Array.from({ length: width * height }, (_, i) => Number(pixels[i * 4] > 127));
}

export async function captureAuthoring(
  project: Project,
  settings: {
    target: string;
    stage?: string;
    camera: Camera;
    width?: number;
    height?: number;
    tick?: number;
    mode?: EvaluatedScene["mode"];
  },
) {
  const render = await renderSource(
    project,
    settings.target,
    settings.stage,
    settings.camera,
    settings.width ?? 640,
    settings.height ?? 480,
  );
  try {
    const blob = await render.capture(settings.mode ?? "beauty", settings.tick ?? 0);
    return {
      blob,
      metadata: {
        sourceKey: contentKey(project),
        settings,
        completeness: render.renderer.completeness,
        measurements: render.renderer.measurements,
      },
    };
  } finally {
    render.dispose();
  }
}
