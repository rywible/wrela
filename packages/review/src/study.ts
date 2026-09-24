import type { StudyReviewer } from "@wrela/authoring";
import { encodeCapturePixels } from "@wrela/render-webgpu";
import { type ReviewCaptureCache, reviewAuthoringPacket } from "./packet";

/** One browser invocation evaluates a bounded set; the gallery never assigns an art score. */
export const reviewAuthoringStudy = async (
  ...[baseline, candidates, constraints, settings, save, shared]: [
    ...Parameters<StudyReviewer>,
    (name: string, value: Blob | object) => Promise<string>,
    ReviewCaptureCache?,
  ]
) => {
  const blobs = new Map<string, Blob>();
  const cache: ReviewCaptureCache = shared ?? { captures: new Map() };
  const started = performance.now(),
    deadline = started + (settings.budgetMs ?? Infinity);
  const results: Awaited<ReturnType<StudyReviewer>>["results"] = [];
  const sheet = document.createElement("canvas"),
    cellWidth = 480,
    cellHeight = 360,
    labelHeight = 50;
  const columns = Math.min(3, settings.views.length);
  sheet.width = cellWidth * columns;
  sheet.height = (candidates.length + 1) * (cellHeight + labelHeight);
  const ctx = sheet.getContext("2d");
  if (!ctx) throw Error("Study gallery canvas unavailable");
  ctx.fillStyle = "#15201e";
  ctx.fillRect(0, 0, sheet.width, sheet.height);
  try {
    for (const [i, candidate] of candidates.entries()) {
      ctx.fillStyle = "white";
      ctx.font = "15px sans-serif";
      ctx.fillText(
        candidate.label ??
          `${candidate.proposal} · exposure ${candidate.conditions.exposure.toFixed(2)} · moisture ${candidate.conditions.moisture.toFixed(2)}`,
        12,
        (i + 1) * (cellHeight + labelHeight) + 20,
      );
      try {
        if (i > 0 && performance.now() >= deadline)
          throw Error("Search budget reached; retained earlier reviewed alternatives");
        const packet = await reviewAuthoringPacket(
          baseline,
          candidate.project,
          constraints,
          settings,
          async (name, value) => {
            const ref = await save(`${candidate.proposal}-${name}`, value);
            if (value instanceof Blob) blobs.set(ref, value);
            return ref;
          },
          cache,
        );
        results.push({ proposal: candidate.proposal, packet });
        ctx.fillText(
          packet.report.results.every((r) => r.status === "passed")
            ? "Constraints passed · visual acceptance pending"
            : "Constraint failure · ineligible for publication",
          12,
          (i + 1) * (cellHeight + labelHeight) + 40,
        );
        for (let col = 0; col < columns; col++) {
          const capture = packet.captures[col] ?? packet.captures[0],
            blob = blobs.get(capture.candidate);
          if (!blob) throw Error("Study capture missing from retained artifacts");
          if (i === 0) {
            const before = blobs.get(capture.baseline);
            if (!before) throw Error("Missing baseline gallery capture");
            const baselineImage = await createImageBitmap(before);
            ctx.drawImage(baselineImage, col * cellWidth, labelHeight, cellWidth, cellHeight);
            baselineImage.close();
            ctx.fillStyle = "white";
            ctx.fillText(`BASELINE · ${settings.views[col].id}`, col * cellWidth + 12, 30);
          }
          const bitmap = await createImageBitmap(blob);
          ctx.drawImage(
            bitmap,
            col * cellWidth,
            (i + 1) * (cellHeight + labelHeight) + labelHeight,
            cellWidth,
            cellHeight,
          );
          bitmap.close();
        }
        blobs.clear();
      } catch (error) {
        if (results.at(-1)?.proposal === candidate.proposal) results.pop();
        results.push({ proposal: candidate.proposal, error: String(error).slice(0, 1800) });
        ctx.fillText(
          "Review failed; inspect retained failure before retrying",
          12,
          (i + 1) * (cellHeight + labelHeight) + 40,
        );
      }
    }
    const blob = await encodeCapturePixels(
      ctx.getImageData(0, 0, sheet.width, sheet.height).data,
      sheet.width,
      sheet.height,
    );
    return { results, gallery: await save("study-gallery.png", blob) };
  } finally {
    if (!shared) {
      cache.render?.dispose();
      cache.captures.clear();
    }
  }
};
