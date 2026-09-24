import { expect, test } from "bun:test";
import { validateNativeEvaluation } from "./native-evaluation";
import type { BenchmarkTask } from "./protocol";

test("native comparison requires exact source hashes and complete independent measurements", () => {
  const task: BenchmarkTask = {
    id: "revise",
    category: "revise",
    brief: "Widen",
    acceptance: ["Clear doorway"],
    engines: ["blender"],
    budget: { seconds: 60, tokens: 1000 },
    baseline: "scene.json",
    workId: "revise",
    constraints: [
      { id: "width", kind: "clearance", target: "gate", minimum: [-1, 0, -1], maximum: [1, 2, 1] },
    ],
  };
  const sources = [{ path: "final.blend", sha256: "a".repeat(64) }];
  const report = {
    version: 1,
    suiteKey: "suite",
    task: "revise",
    evaluator: "native-test",
    sources,
    checks: [
      {
        id: "width",
        status: "passed",
        scope: "Native evaluated geometry",
        measurements: { width: 2 },
        evidence: ["measurement.json"],
      },
    ],
  };
  expect(validateNativeEvaluation(report, "suite", task, sources).passed).toBe(true);
  expect(() => validateNativeEvaluation({ ...report, checks: [] }, "suite", task, sources)).toThrow(
    "every registered",
  );
  expect(() =>
    validateNativeEvaluation(report, "suite", task, [{ ...sources[0], sha256: "b".repeat(64) }]),
  ).toThrow("source bytes");
  expect(
    validateNativeEvaluation(
      { ...report, checks: [{ ...report.checks[0], status: "unmeasured" }] },
      "suite",
      task,
      sources,
    ).passed,
  ).toBe(false);
});
