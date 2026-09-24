import type { PacketSettings, ResultConstraint, ReviewPacket } from "@wrela/authoring";
import { contentKey, type Project } from "@wrela/model";
import { encodeCapturePixels } from "@wrela/render-webgpu";
import { renderSource } from "./browser";
import { type ReviewOptions, reviewAuthoring } from "./index";
import { reviewStage } from "./review-stage";
export type ReviewCaptureCache = {
  captures: Map<
    string,
    {
      blob: Blob;
      completeness: unknown;
      measurements: unknown;
      timing?: { prepareMs: number; captureMs: number };
    }
  >;
  render?: Awaited<ReturnType<typeof renderSource>>;
  stageKey?: string;
  maxEntries?: number;
};

/** One browser, two reusable scene hosts, matched cameras, and one retained contact sheet. */
export async function reviewAuthoringPacket(
  baseline: Project,
  candidate: Project,
  constraints: ResultConstraint[],
  settings: PacketSettings,
  save: NonNullable<ReviewOptions["saveArtifact"]>,
  cache?: ReviewCaptureCache,
): Promise<ReviewPacket> {
  const started = performance.now(),
    report = await reviewAuthoring(baseline, candidate, constraints, { saveArtifact: save });
  const reviewed = performance.now();
  const captures: ReviewPacket["captures"] = [];
  const evidence: string[] = [];
  const sheet = document.createElement("canvas");
  sheet.width = settings.width * 2;
  sheet.height = (settings.height + 30) * settings.views.length;
  const ctx = sheet.getContext("2d");
  if (!ctx) throw Error("Review contact sheet canvas unavailable");
  ctx.fillStyle = "#15201e";
  ctx.fillRect(0, 0, sheet.width, sheet.height);
  ctx.font = "16px sans-serif";
  ctx.fillStyle = "#ffffff";
  let firstCandidateImageMs = 0,
    firstCandidateImageAt: number | undefined,
    cacheHits = 0,
    renderedImages = 0;
  // Reuse a host across cameras when the review stage is unchanged.
  for (const [column, project] of [baseline, candidate].entries()) {
    let render = cache?.render,
      stage: string | undefined = cache?.stageKey;
    try {
      for (const [row, view] of settings.views.entries()) {
        const fixture = reviewStage(project, settings.target, view);
        if (!project.documents.some((d) => d.id === settings.target)) {
          if (column !== 0) throw Error("Capture target is missing from candidate");
          const empty = document.createElement("canvas");
          empty.width = settings.width;
          empty.height = settings.height;
          const ec = empty.getContext("2d");
          if (!ec) throw Error("Baseline placeholder canvas unavailable");
          ec.fillStyle = "#15201e";
          ec.fillRect(0, 0, empty.width, empty.height);
          ec.fillStyle = "white";
          ec.font = "18px sans-serif";
          ec.fillText("Not present in baseline", 24, 40);
          const blob = await encodeCapturePixels(
            ec.getImageData(0, 0, empty.width, empty.height).data,
            empty.width,
            empty.height,
          );
          const image = await save(`${view.id}-baseline.png`, blob);
          captures[row] = { view: view.id, baseline: image, candidate: "" };
          ctx.drawImage(empty, 0, row * (settings.height + 30) + 30);
          ctx.fillStyle = "white";
          ctx.fillText(`${view.id} · baseline absent`, 12, row * (settings.height + 30) + 21);
          continue;
        }
        const captureKey = contentKey([
          fixture.project,
          settings.target,
          view,
          settings.width,
          settings.height,
        ]);
        let captured = cache?.captures.get(captureKey);
        if (!captured) {
          const prepareStarted = performance.now();
          const stageKey = contentKey([
            fixture.project,
            settings.target,
            fixture.stage,
            settings.width,
            settings.height,
          ]);
          if (
            render &&
            (render.canvas.width !== settings.width || render.canvas.height !== settings.height)
          ) {
            render.dispose();
            render = undefined;
            if (cache) cache.render = undefined;
          }
          if (!render || stage !== stageKey) {
            if (render)
              await render.replaceSource(fixture.project, settings.target, fixture.stage, view.camera);
            else
              render = await renderSource(
                fixture.project,
                settings.target,
                fixture.stage,
                view.camera,
                settings.width,
                settings.height,
              );
            if (cache) cache.render = render;
            stage = stageKey;
            if (cache) cache.stageKey = stageKey;
          }
          render.setCamera(view.camera);
          const captureStarted = performance.now();
          const blob = await render.capture(view.mode ?? "beauty", view.tick);
          const captureEnded = performance.now();
          if (view.rig && (!view.mode || view.mode === "beauty")) await verifyReviewImage(blob);
          captured = {
            blob,
            completeness: structuredClone(render.renderer.completeness),
            measurements: structuredClone(render.renderer.measurements),
            timing: { prepareMs: captureStarted - prepareStarted, captureMs: captureEnded - captureStarted },
          };
          renderedImages++;
          cache?.captures.set(captureKey, captured);
          if (cache)
            while (cache.captures.size > (cache.maxEntries ?? 32))
              cache.captures.delete(cache.captures.keys().next().value!);
        } else cacheHits++;
        const { blob } = captured;
        if (column === 1 && !firstCandidateImageMs) {
          firstCandidateImageMs = performance.now() - started;
          firstCandidateImageAt = Date.now();
        }
        const label = column === 0 ? "baseline" : "candidate",
          name = `${view.id}-${label}.png`;
        const image = await save(name, blob);
        evidence.push(
          await save(`${view.id}-${label}.json`, {
            sourceKey: column === 0 ? report.baselineKey : report.candidateKey,
            lightingKey: fixture.lightingKey,
            reviewRig: view.rig ?? null,
            settings: { ...settings, views: undefined, view },
            completeness: captured.completeness,
            measurements: captured.measurements,
            timing: captured.timing,
            image,
          }),
        );
        captures[row] ??= { view: view.id, baseline: "", candidate: "" };
        captures[row][label] = image;
        const bitmap = await createImageBitmap(blob);
        ctx.drawImage(bitmap, column * settings.width, row * (settings.height + 30) + 30);
        bitmap.close();
        ctx.fillText(
          `${view.id} · ${label}`,
          column * settings.width + 12,
          row * (settings.height + 30) + 21,
        );
      }
    } finally {
      if (!cache) render?.dispose();
    }
  }
  const blob = await encodeCapturePixels(
    ctx.getImageData(0, 0, sheet.width, sheet.height).data,
    sheet.width,
    sheet.height,
  );
  const contactSheet = await save("contact-sheet.png", blob);
  return {
    version: 1,
    report,
    contactSheet,
    evidence,
    captures,
    timings: {
      reviewMs: reviewed - started,
      captureMs: performance.now() - reviewed,
      totalMs: performance.now() - started,
      firstCandidateImageMs,
      firstCandidateImageAt,
      cacheHits,
      renderedImages,
    },
  };
}

/** Fixed daylight review images must contain visible variation; this is capture validity, not an art score. */
export async function verifyReviewImage(blob: Blob) {
  const bitmap = await createImageBitmap(blob),
    canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 48;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    bitmap.close();
    throw Error("Review image inspection unavailable");
  }
  ctx.drawImage(bitmap, 0, 0, 64, 48);
  bitmap.close();
  const pixels = ctx.getImageData(0, 0, 64, 48).data;
  let min = 255,
    max = 0,
    visible = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    const light = (pixels[i] + pixels[i + 1] + pixels[i + 2]) / 3;
    min = Math.min(min, light);
    max = Math.max(max, light);
    if (light > 4) visible++;
  }
  if (max - min < 8 || visible < 64 * 48 * 0.01)
    throw Error(
      "Fixed review image is blank, dark or constant; source constraints alone cannot qualify a visual review",
    );
}
