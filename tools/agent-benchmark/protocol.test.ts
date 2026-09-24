import { expect, test } from "bun:test";
import { contentKey } from "@wrela/model";

import { type BenchmarkAttempt, type BenchmarkSuite, summarizeBenchmark } from "./protocol";

const suite: BenchmarkSuite = {
  version: 1,
  id: "test",
  seed: 1,
  createdAt: new Date().toISOString(),
  sourceFingerprint: "source",
  repetitions: 1,
  policy: {
    assets: "provided-only",
    track: "content",
    model: "test",
    engineAccess: "best-available-programmatic",
    artReview: "blind-independent",
    delivery: "separate",
  },
  tasks: [
    {
      id: "task",
      category: "revise",
      brief: "Brief",
      acceptance: ["Reviewed"],
      engines: ["wrela", "blender", "unreal"],
      budget: { seconds: 60, tokens: 1000 },
      baseline: "baseline.json",
      workId: "work",
      constraints: [{ id: "shape", kind: "source", target: "gate", path: ["field"] }],
    },
  ],
};
function attempt(): BenchmarkAttempt {
  return {
    version: 1,
    id: "attempt",
    suiteKey: contentKey(suite),
    task: "task",
    engine: "wrela",
    repetition: 0,
    agent: "agent",
    model: "test",
    execution: "agent",
    status: "completed",
    startedAt: suite.createdAt,
    endedAt: suite.createdAt,
    elapsedMs: 50,
    tokens: null,
    humanInterventions: null,
    engineEdits: [],
    constraintsPassed: true,
    artifacts: [],
    stages: [],
  };
}
test("unknown costs and missing art reviews cannot become accepted benchmark wins", () => {
  const a = attempt();
  let report = summarizeBenchmark(suite, [a], [], {});
  expect(report[0].accepted).toBeNull();
  expect(report[0].constraintQualified).toBe(0);
  a.tokens = 100;
  a.humanInterventions = 0;
  report = summarizeBenchmark(suite, [a], [], {});
  expect(report[0].constraintQualified).toBe(1);
  expect(report[0].accepted).toBeNull();
  expect(report[1].attempted).toBe(0);
  expect(() => summarizeBenchmark(suite, [a, a], [], {})).toThrow("Duplicate");
});
test("scripted integration is excluded and engine changes disqualify content-only trials", () => {
  const a = attempt();
  a.tokens = 50;
  a.execution = "scripted-smoke";
  expect(summarizeBenchmark(suite, [a], [], {})[0].attempted).toBe(0);
  a.execution = "agent";
  a.engineEdits = ["renderer.ts"];
  expect(summarizeBenchmark(suite, [a], [], {})[0].constraintQualified).toBe(0);
});
