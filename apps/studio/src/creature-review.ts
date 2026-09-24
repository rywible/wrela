import { canonical } from "@wrela/model";

/** Capture identity excludes source revision: source is the variable being compared. */
export type CreatureCaptureMetadata = {
  revision: number;
  key: string;
  subject: string;
  stage: string;
  camera: unknown;
  tick: number;
  quality: string;
  channels: readonly string[];
  overlays: readonly string[];
  viewOverrides: unknown;
  rendering: unknown;
};

export function compareCreatureCaptures(
  baseline: CreatureCaptureMetadata,
  candidate: CreatureCaptureMetadata,
) {
  const settings = [
    "subject",
    "stage",
    "camera",
    "tick",
    "quality",
    "channels",
    "overlays",
    "viewOverrides",
    "rendering",
  ] as const;
  const mismatches = settings.filter((key) => canonical(baseline[key]) !== canonical(candidate[key]));
  return {
    matched: mismatches.length === 0,
    mismatches,
    sourceChanged: baseline.key !== candidate.key,
    baselineRevision: baseline.revision,
    candidateRevision: candidate.revision,
    assessment: "Visual judgment required; matched captures do not establish artistic quality.",
  };
}
