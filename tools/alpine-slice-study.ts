import { AuthoringSession } from "@wrela/authoring";
import { ALPINE_LOOKDEV_CAMERAS, createAlpineLookdevProject } from "@wrela/examples";
import { createAlpineCompositionEdits } from "@wrela/examples/alpine-composition";
import type { Camera, FrameMeasurements, GpuFrameTiming } from "@wrela/model";

export const ALPINE_PROFILES = {
  portable: {
    output: [1280, 720] as [number, number],
    quality: "low" as const,
    gpuP95Ms: 26,
    cpuP95Ms: 5,
    gpuBytes: 128 * 1024 ** 2,
    installedBytes: 192 * 1024 ** 2,
  },
  balanced: {
    output: [1920, 1080] as [number, number],
    quality: "balanced" as const,
    gpuP95Ms: 12,
    cpuP95Ms: 4,
    gpuBytes: 256 * 1024 ** 2,
    installedBytes: 192 * 1024 ** 2,
  },
};
export type AlpineProfile = keyof typeof ALPINE_PROFILES;
export type AlpineComposition = "primary" | "river-bend";

/** A held-out composition is an ordinary undoable source transaction, never a custom renderer. */
export function createAlpineSliceStudy(composition: AlpineComposition, profile: AlpineProfile) {
  const authoring = new AuthoringSession(createAlpineLookdevProject());
  const operations =
    composition === "river-bend" ? createAlpineCompositionEdits(authoring.getSnapshot().project) : [];
  if (operations.length)
    authoring.apply({
      expectedRevision: 0,
      actor: "alpine-review",
      intent: "Recompose the same construction recipes around a wayside shelter",
      label: "River bend composition",
      operations,
    });
  const project = authoring.getSnapshot().project;
  const world = project.documents.find((document) => document.id === project.entry);
  if (world?.kind !== "world" || !world.composition) throw Error("Missing alpine world composition");
  const stage = project.documents.find(
    (document) => document.kind === "stage" && document.subjects.includes(world.id),
  );
  const cameras: { id: string; camera: Camera }[] =
    composition === "primary"
      ? ALPINE_LOOKDEV_CAMERAS
      : [
          { id: "approach", camera: { position: [-3.7, 2.2, -13], target: [-4.5, 1.8, 8], fov: 53 } },
          {
            id: "shelter",
            camera: stage?.kind === "stage" ? stage.camera : ALPINE_LOOKDEV_CAMERAS[0].camera,
          },
          { id: "river-overlook", camera: { position: [12, 7, -16], target: [-8, 2, 11], fov: 48 } },
        ];
  return {
    project,
    subject: project.entry,
    composition,
    profile,
    contract: ALPINE_PROFILES[profile],
    operations,
    cameras,
  };
}
export type AlpineSliceStudy = ReturnType<typeof createAlpineSliceStudy>;
export function percentile(values: number[], fraction: number): number | null {
  if (!values.length) return null;
  if (
    fraction <= 0 ||
    fraction > 1 ||
    !Number.isFinite(fraction) ||
    values.some((value) => !Number.isFinite(value))
  )
    throw new Error("Percentiles require finite values and a fraction in (0, 1]");
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)];
}
/** Renderer measurements cache the last asynchronous result; only a tagged timing belongs to a frame. */
export function matchAlpineGpuTimings(
  measurements: readonly FrameMeasurements[],
  timings: readonly GpuFrameTiming[],
): FrameMeasurements[] {
  const byFrame = new Map<number, number>();
  for (const timing of timings) {
    if (byFrame.has(timing.frame)) throw new Error(`Duplicate GPU timing for frame ${timing.frame}`);
    byFrame.set(timing.frame, timing.gpuMs);
  }
  return measurements.map((frame) => ({ ...frame, gpuMs: byFrame.get(frame.frame) ?? null }));
}
/** GPU timestamps measure pass cost; RAF/capture/readback time is not mislabeled frame rate. */
export function assessAlpineBudget(
  profile: AlpineProfile,
  measurements: FrameMeasurements[],
  installedBytes: number,
) {
  const target = ALPINE_PROFILES[profile];
  const gpu = measurements.flatMap((frame) =>
    frame.gpuMs !== null && Number.isFinite(frame.gpuMs) && frame.gpuMs > 0 ? [frame.gpuMs] : [],
  );
  const cpu = measurements.flatMap((frame) =>
    Number.isFinite(frame.cpuMs) && frame.cpuMs >= 0 ? [frame.cpuMs] : [],
  );
  const gpuP95 = percentile(gpu, 0.95),
    cpuP95 = percentile(cpu, 0.95);
  const uniqueFrames = new Set(measurements.map((frame) => frame.frame)).size === measurements.length;
  const validMeasurements =
    uniqueFrames &&
    Number.isFinite(installedBytes) &&
    installedBytes >= 0 &&
    measurements.every(
      (frame) =>
        Number.isInteger(frame.frame) &&
        frame.frame >= 0 &&
        Number.isFinite(frame.cpuMs) &&
        frame.cpuMs >= 0 &&
        Number.isFinite(frame.gpuBytes) &&
        frame.gpuBytes >= 0 &&
        (frame.gpuMs === null || (Number.isFinite(frame.gpuMs) && frame.gpuMs > 0)) &&
        frame.renderResolution?.length === 2 &&
        frame.renderResolution.every((n) => Number.isInteger(n) && n > 0),
    );
  const outputMatches =
    measurements.length >= 30 &&
    measurements.every(
      (frame) =>
        frame.outputResolution?.length === 2 &&
        frame.outputResolution.every((n, i) => n === target.output[i]),
    );
  const maximumBytes = measurements.length ? Math.max(...measurements.map((frame) => frame.gpuBytes)) : null;
  const failures: string[] = [];
  if (!validMeasurements)
    failures.push("Invalid or duplicate frame, timing, resolution, or resource measurements");
  if (!outputMatches) failures.push(`Require at least 30 frames at ${target.output.join("×")} output`);
  if (gpu.length < 30) failures.push(`Only ${gpu.length}/30 required frame-tagged GPU samples`);
  if (gpuP95 === null) failures.push("GPU p95 is unavailable");
  else if (gpuP95 > target.gpuP95Ms)
    failures.push(`GPU p95 ${gpuP95.toFixed(3)} ms exceeds ${target.gpuP95Ms} ms`);
  if (cpuP95 === null) failures.push("CPU p95 is unavailable");
  else if (cpuP95 > target.cpuP95Ms)
    failures.push(`CPU p95 ${cpuP95.toFixed(3)} ms exceeds ${target.cpuP95Ms} ms`);
  if (maximumBytes === null) failures.push("GPU allocation measurement is unavailable");
  else if (maximumBytes > target.gpuBytes)
    failures.push(`GPU payload ${maximumBytes} bytes exceeds ${target.gpuBytes} bytes`);
  if (installedBytes > target.installedBytes)
    failures.push(`Installed payload ${installedBytes} bytes exceeds ${target.installedBytes} bytes`);
  return {
    target,
    frames: measurements.length,
    gpuSamples: gpu.length,
    uniqueFrames,
    validMeasurements,
    outputMatches,
    gpuP95,
    cpuP95,
    maximumBytes,
    installedBytes,
    actualRenderResolutions: [
      ...new Set(measurements.map((frame) => frame.renderResolution?.join("×") ?? "unknown")),
    ],
    failures,
    met: failures.length === 0,
    scope:
      "Static-view extraction plus render submission CPU, frame-tagged GPU timestamps, and engine-counted resource payloads on the measured adapter. Simulation, process heap and other hardware are unmeasured. Internal resolution is disclosed; output size does not establish native-resolution fidelity. Artistic acceptance remains unverified.",
  };
}

/** Capture completion is separate: retain every camera's evidence even when its budget fails. */
export function summarizeAlpineBudgets(
  frames: readonly { id: string; budget: ReturnType<typeof assessAlpineBudget> }[],
  expectedFrames: number,
) {
  const budgetFailures = frames.flatMap((frame) =>
    frame.budget.failures.map((reason) => `${frame.id}: ${reason}`),
  );
  if (frames.length !== expectedFrames)
    budgetFailures.unshift(`Measured ${frames.length}/${expectedFrames} required camera budgets`);
  return { budgetMet: budgetFailures.length === 0, budgetFailures };
}
