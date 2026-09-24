import type { AuthoringTrace } from "../authoring-trace";

/** Observable artifacts at each deadline, not inferred artistic scores or model timing. */
export function authoringDeadlines(rows: AuthoringTrace[], startedAt: number) {
  return [15, 30, 60].map((seconds) => {
    const deadline = startedAt + seconds * 1000;
    const complete = rows.filter((r) => r.ok && r.startedAt >= startedAt && r.endedAt <= deadline);
    return {
      seconds,
      candidateImageAvailable: rows.some(
        (r) =>
          (r.imagesAvailableAt ?? r.firstCandidateImageAt) !== undefined &&
          (r.imagesAvailableAt ?? r.firstCandidateImageAt)! >= startedAt &&
          (r.imagesAvailableAt ?? r.firstCandidateImageAt)! <= deadline,
      ),
      reviewedOutputAvailable: complete.some((r) =>
        ["work.job", "work.study", "work.evaluate", "work.hero", "work.iterate"].includes(r.command),
      ),
      deliveryFinished: complete.some((r) => r.command === "work.finish"),
      artisticAcceptance: null,
    };
  });
}
