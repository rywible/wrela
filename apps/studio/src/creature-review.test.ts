import { describe, expect, test } from "bun:test";
import { type CreatureCaptureMetadata, compareCreatureCaptures } from "./creature-review";

const capture: CreatureCaptureMetadata = {
  revision: 3,
  key: "before",
  subject: "guardian",
  stage: "neutral-stage",
  camera: { position: [0, 2, 5], target: [0, 1, 0], fov: 48 },
  tick: 0,
  quality: "interactive",
  channels: ["beauty"],
  overlays: [],
  viewOverrides: { exposure: 1, wind: 0, grid: false },
  rendering: { outputResolution: [320, 240], renderResolution: [320, 240] },
};

describe("creature evidence comparison", () => {
  test("allows source changes while holding review conditions fixed", () => {
    const result = compareCreatureCaptures(capture, { ...capture, revision: 5, key: "after" });
    expect(result.matched).toBe(true);
    expect(result.sourceChanged).toBe(true);
    expect(result.baselineRevision).toBe(3);
    expect(result.candidateRevision).toBe(5);
  });
  test("rejects mismatched view, timing, subject and rendering conditions", () => {
    const result = compareCreatureCaptures(capture, {
      ...capture,
      subject: "other",
      camera: { position: [2, 2, 5] },
      tick: 60,
      rendering: { outputResolution: [640, 480], renderResolution: [640, 480] },
    });
    expect(result.matched).toBe(false);
    expect(result.mismatches).toEqual(["subject", "camera", "tick", "rendering"]);
  });
});
