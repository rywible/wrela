import { resultConstraintSchema } from "@wrela/authoring";
import { contentKey, idSchema } from "@wrela/model";

import { z } from "zod";

export const benchmarkTaskSchema = z.strictObject({
  id: idSchema,
  category: z.enum(["create", "revise", "repair", "compose", "handoff"]),
  brief: z.string().min(1),
  acceptance: z.array(z.string()).min(1),
  engines: z.array(z.enum(["wrela", "blender", "unreal"])).min(1),
  budget: z.strictObject({ seconds: z.number().int().positive(), tokens: z.number().int().positive() }),
  baseline: z.string(),
  workId: idSchema,
  subject: idSchema.optional(),
  prerequisite: idSchema.optional(),
  constraints: z.array(resultConstraintSchema).min(1).max(64),
});
export type BenchmarkTask = z.infer<typeof benchmarkTaskSchema>;
export const benchmarkSuiteSchema = z.strictObject({
  version: z.literal(1),
  id: idSchema,
  seed: z.number().int(),
  createdAt: z.string().datetime(),
  sourceFingerprint: z.string(),
  repetitions: z.number().int().min(1).max(20),
  policy: z.strictObject({
    assets: z.enum(["provided-only", "open-library"]),
    track: z.enum(["content", "tool-development"]),
    model: z.string().min(1),
    engineAccess: z.literal("best-available-programmatic"),
    artReview: z.literal("blind-independent"),
    delivery: z.literal("separate"),
  }),
  tasks: z.array(benchmarkTaskSchema).min(1).max(30),
});
export type BenchmarkSuite = z.infer<typeof benchmarkSuiteSchema>;
export const benchmarkAttemptSchema = z.strictObject({
  version: z.literal(1),
  id: idSchema,
  suiteKey: z.string(),
  task: idSchema,
  engine: z.enum(["wrela", "blender", "unreal"]),
  repetition: z.number().int().nonnegative(),
  agent: z.string(),
  model: z.string(),
  execution: z.enum(["agent", "scripted-smoke"]),
  dispatch: z.enum(["agent-started", "engine-started"]).optional(),
  status: z.enum(["running", "completed", "failed", "timeout", "unavailable"]),
  reason: z.string().optional(),
  startedAt: z.string().datetime(),
  endedAt: z.string().datetime(),
  elapsedMs: z.number().finite().nonnegative(),
  tokens: z.number().int().nonnegative().nullable(),
  humanInterventions: z.number().int().nonnegative().nullable(),
  engineEdits: z.array(z.string()),
  constraintsPassed: z.boolean().nullable(),
  deadlines: z
    .array(
      z.strictObject({
        seconds: z.number().positive(),
        candidateImageAvailable: z.boolean(),
        reviewedOutputAvailable: z.boolean(),
        deliveryFinished: z.boolean(),
        artisticAcceptance: z.null(),
      }),
    )
    .optional(),
  artifacts: z.array(
    z.strictObject({
      path: z.string(),
      sha256: z.string().regex(/^[a-f0-9]{64}$/),
      kind: z.enum(["source", "image", "review", "transcript", "handoff"]),
    }),
  ),
  telemetry: z
    .strictObject({
      toolCalls: z.number().int().nonnegative(),
      failedCalls: z.number().int().nonnegative(),
      discoveryCalls: z.number().int().nonnegative(),
      discoveryMs: z.number().nonnegative().optional(),
      firstCandidateImageMs: z.number().nonnegative().nullable().optional(),
      firstReviewDeliveredMs: z.number().nonnegative().optional(),
      toolMs: z.number().nonnegative(),
      observedToolWallMs: z.number().nonnegative(),
      unattributedMs: z.number().nonnegative(),
      modelMs: z.null(),
      scope: z.string(),
    })
    .optional(),
  stages: z.array(
    z.strictObject({
      name: z.string(),
      durationMs: z.number().nonnegative(),
      origin: z.enum(["runner", "agent-reported"]),
    }),
  ),
});
export type BenchmarkAttempt = z.infer<typeof benchmarkAttemptSchema>;
export const benchmarkJudgementSchema = z.strictObject({
  version: z.literal(1),
  blindId: idSchema,
  reviewer: z.string().min(1),
  verdict: z.enum(["accepted", "rejected"]),
  reason: z.string().min(1),
  scores: z.strictObject({
    brief: z.number().int().min(1).max(5),
    quality: z.number().int().min(1).max(5),
    editability: z.number().int().min(1).max(5),
  }),
});
export type BenchmarkJudgement = z.infer<typeof benchmarkJudgementSchema>;
export function summarizeBenchmark(
  suite: BenchmarkSuite,
  attempts: BenchmarkAttempt[],
  judgements: BenchmarkJudgement[],
  mapping: Record<string, string>,
) {
  const key = contentKey(suite);
  const seen = new Set<string>();
  for (const a of attempts) {
    benchmarkAttemptSchema.parse(a);
    if (
      a.suiteKey !== key ||
      !suite.tasks.some((t) => t.id === a.task && t.engines.includes(a.engine)) ||
      a.repetition >= suite.repetitions
    )
      throw Error("Attempt does not belong to the declared suite");
    const cell = `${a.task}/${a.engine}/${a.repetition}`;
    if (seen.has(cell)) throw Error("Duplicate attempt cell would bias the comparison");
    seen.add(cell);
  }
  for (const j of judgements) {
    benchmarkJudgementSchema.parse(j);
    if (!mapping[j.blindId]) throw Error("Unknown blind review identity");
  }
  if (
    new Set(Object.values(mapping)).size !== Object.keys(mapping).length ||
    Object.values(mapping).some((id) => !attempts.some((a) => a.id === id))
  )
    throw Error("Invalid blind review mapping");
  if (new Set(judgements.map((j) => `${j.blindId}/${j.reviewer}`)).size !== judgements.length)
    throw Error("A reviewer cannot vote twice on the same sample");
  return ["wrela", "blender", "unreal"].map((engine) => {
    const rows = attempts.filter((a) => a.engine === engine && a.execution === "agent");
    const qualified = rows.filter((a) => {
      const task = suite.tasks.find((t) => t.id === a.task);
      if (!task) return false;
      return (
        a.status === "completed" &&
        a.constraintsPassed === true &&
        a.model === suite.policy.model &&
        !(suite.policy.track === "content" && a.engineEdits.length) &&
        a.tokens !== null &&
        a.humanInterventions !== null &&
        a.tokens <= task.budget.tokens &&
        a.elapsedMs <= task.budget.seconds * 1000
      );
    });
    const reviewed = qualified.flatMap((a) => {
      const blind = Object.entries(mapping).find(([, id]) => id === a.id)?.[0];
      const reviews = judgements.filter((j) => j.blindId === blind);
      return reviews.length ? [{ attempt: a, accepted: reviews.every((j) => j.verdict === "accepted") }] : [];
    });
    const accepted = reviewed.filter((r) => r.accepted);
    return {
      engine,
      expected:
        suite.tasks.filter((t) => t.engines.includes(engine as BenchmarkAttempt["engine"])).length *
        suite.repetitions,
      attempted: rows.length,
      pending: rows.filter((a) => a.status === "running").length,
      unavailable: rows.filter((a) => a.status === "unavailable").length,
      failed: rows.filter((a) => a.status === "failed" || a.status === "timeout").length,
      constraintQualified: qualified.length,
      reviewed: reviewed.length,
      accepted: reviewed.length ? accepted.length : null,
      acceptedElapsedMs: accepted.map((r) => r.attempt.elapsedMs),
      totalAttemptElapsedMs: rows.reduce((sum, r) => sum + r.elapsedMs, 0),
      unknownTokenRuns: rows.filter((a) => a.tokens === null).length,
      unknownInterventionRuns: rows.filter((a) => a.humanInterventions === null).length,
      conclusion:
        "No ranking until matched task/model/asset/budget cells have complete measurements and independent artistic review. Missing engines are not losses.",
    };
  });
}
