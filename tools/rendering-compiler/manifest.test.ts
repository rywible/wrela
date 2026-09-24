import { expect, test } from "bun:test";
import { assertCompleteIdentities, compareLinear, TRAJECTORY, trajectoryFrame } from "./manifest";
import { observedTimingQuantumMs, summarizeMatchedTrial, trialOrder } from "./timing";

test("acceptance trajectory reproduces authored endpoints and refuses invalid frames", () => {
  expect(trajectoryFrame(0, 91).camera.position).toEqual([...TRAJECTORY[0].position]);
  expect(trajectoryFrame(90, 91).sunDirection).toEqual([...TRAJECTORY[3].sun]);
  expect(trajectoryFrame(45, 91).time).toBe(0.75);
  expect(() => trajectoryFrame(0, 1)).toThrow();
});
test("complete flag cannot hide missing, duplicated, rejected, or extra identities", () => {
  const complete = {
    frame: 1,
    complete: true,
    rendered: ["bunny"],
    culled: ["stone"],
    uploading: [],
    rejected: [],
  };
  assertCompleteIdentities(["bunny", "stone"], complete);
  expect(() => assertCompleteIdentities(["bunny", "stone", "water"], complete)).toThrow();
  expect(() => assertCompleteIdentities(["bunny"], complete)).toThrow();
  expect(() => assertCompleteIdentities(["bunny", "stone"], { ...complete, culled: ["bunny"] })).toThrow();
  expect(() =>
    assertCompleteIdentities(["bunny", "stone"], {
      ...complete,
      rejected: [{ id: "water", reason: "budget" }],
    }),
  ).toThrow();
});
test("radiance metrics preserve HDR differences, denominator floor, and finite-output gate", () => {
  const metric = compareLinear([4, 0, 0, 1], [3, 0, 0, 1]);
  expect(metric.relativeRms).toBeCloseTo(0.25);
  expect(metric.highlightMaximum).toBe(1);
  expect(compareLinear([0, 0, 0, 1], [0, 0, 0, 1]).relativeRms).toBe(0);
  expect(() => compareLinear([NaN, 0, 0, 1], [0, 0, 0, 1])).toThrow();
  expect(() => compareLinear([0, 0, 0, 1], [0, Infinity, 0, 1])).toThrow();
});

test("matched timing rejects missing coverage and excludes unmatched fast frames", () => {
  const sample = (frame: number, gpuMs: number) => ({
    frame,
    trajectoryFrame: frame,
    gpuMs,
    sceneMs: gpuMs,
    shadowMs: 0,
    displayMs: 0,
  });
  const direct = Array.from({ length: 32 }, (_, frame) => sample(frame, frame < 16 ? 0.065536 : 0.65536));
  const candidate = Array.from({ length: 16 }, (_, frame) => sample(frame + 16, 0.32768));
  const trial = summarizeMatchedTrial([
    { variant: "reference", gpu: direct },
    { variant: "candidate", gpu: candidate },
  ]);
  expect(trial.variants[0].passes.gpuMs.p50).toBe(0.65536);
  expect(trial.variants[0].unmatchedSamples).toBe(16);
  expect(observedTimingQuantumMs(direct)).toBe(0.065536);
  expect(() =>
    summarizeMatchedTrial([
      { variant: "reference", gpu: direct },
      { variant: "candidate", gpu: candidate.slice(1) },
    ]),
  ).toThrow();
  expect(trialOrder(["a", "b"], 0)).toEqual(["a", "b"]);
  expect(trialOrder(["a", "b"], 1)).toEqual(["b", "a"]);
});
