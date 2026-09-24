import { contentKey, idSchema, vec3Schema } from "@wrela/model";

import { z } from "zod";
import { sourcePreservationSchema } from "./domain-constraints";

const base = { id: idSchema, target: idSchema };
export const reviewCameraSchema = z
  .strictObject({
    position: vec3Schema,
    target: vec3Schema,
    fov: z.number().finite().min(15).max(100),
  })
  .refine((c) => Math.hypot(...c.position.map((v, i) => v - c.target[i])) > 0.01, "Camera needs a direction");
export const resultConstraintSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("source"), id: idSchema, ...sourcePreservationSchema.shape }),
  z.strictObject({ kind: z.literal("route"), ...base, route: idSchema }),
  z
    .strictObject({
      kind: z.literal("geometry"),
      ...base,
      minTriangles: z.number().int().nonnegative().default(1),
      maxTriangles: z.number().int().positive().max(100_000_000),
      minimumExtent: vec3Schema,
      maximumExtent: vec3Schema,
    })
    .refine(
      (v) =>
        v.minTriangles <= v.maxTriangles &&
        v.minimumExtent.every((n, i) => n >= 0 && n <= v.maximumExtent[i]),
      "Geometry bounds must be ordered and nonnegative",
    ),
  z.strictObject({
    kind: z.literal("dimensions"),
    ...base,
    minimum: vec3Schema,
    maximum: vec3Schema,
    part: idSchema.optional(),
    tolerance: z.number().finite().min(0).max(1).default(0.01),
  }),
  z
    .strictObject({ kind: z.literal("clearance"), ...base, minimum: vec3Schema, maximum: vec3Schema })
    .refine((c) => c.minimum.every((v, i) => v < c.maximum[i]), "Clearance needs positive volume"),
  z.strictObject({ kind: z.literal("sightline"), ...base, from: vec3Schema, to: vec3Schema }),
  z.strictObject({
    kind: z.literal("motion"),
    ...base,
    motion: idSchema,
    maxSlip: z.number().finite().nonnegative().max(10),
    maxResidual: z.number().finite().nonnegative().max(10),
  }),
  z
    .strictObject({
      kind: z.literal("silhouette"),
      ...base,
      camera: reviewCameraSchema,
      stage: idSchema.optional(),
      tick: z.number().int().min(0).max(600).default(0),
      width: z.number().int().min(64).max(1024).default(320),
      height: z.number().int().min(64).max(1024).default(240),
      maxChangedFraction: z.number().min(0).max(1),
      region: z
        .tuple([
          z.number().min(0).max(1),
          z.number().min(0).max(1),
          z.number().min(0).max(1),
          z.number().min(0).max(1),
        ])
        .default([0, 0, 1, 1]),
    })
    .refine((v) => v.region[0] < v.region[2] && v.region[1] < v.region[3], "Review region must have area"),
  z.strictObject({
    kind: z.literal("resources"),
    ...base,
    camera: reviewCameraSchema,
    stage: idSchema.optional(),
    maxGpuBytes: z.number().int().positive(),
    maxCpuMs: z.number().positive(),
    maxGpuMs: z.number().positive(),
    frames: z.number().int().min(30).max(240).default(60),
    width: z.number().int().min(128).max(1920).default(640),
    height: z.number().int().min(128).max(1080).default(480),
  }),
]);
export type ResultConstraint = z.infer<typeof resultConstraintSchema>;
export const constraintResultSchema = z.strictObject({
  id: idSchema,
  status: z.enum(["passed", "failed", "unmeasured"]),
  scope: z.string().min(1).max(2000),
  measured: z.record(z.string(), z.union([z.number().finite(), z.boolean(), z.string(), z.null()])),
  evidence: z.array(z.string().max(1000)).max(32),
  reason: z.string().max(2000).optional(),
});
export type ConstraintResult = z.infer<typeof constraintResultSchema>;
export const authoringReviewSchema = z.strictObject({
  version: z.literal(1),
  baselineKey: z.string().min(1),
  candidateKey: z.string().min(1),
  constraintsKey: z.string().min(1),
  results: z.array(constraintResultSchema).max(64),
  durationMs: z.number().finite().nonnegative(),
  createdAt: z.string().datetime(),
  evaluator: z.string().min(1).max(200),
});
export type AuthoringReview = z.infer<typeof authoringReviewSchema>;
export function reviewVerdict(
  report: AuthoringReview,
  constraints: ResultConstraint[],
  baselineKey: string,
  candidateKey: string,
) {
  const review = authoringReviewSchema.parse(report);
  const expected = constraints.map((c) => c.id);
  if (
    review.baselineKey !== baselineKey ||
    review.candidateKey !== candidateKey ||
    review.constraintsKey !== contentKey(constraints)
  )
    throw Error("Review belongs to different source or constraints; review again");
  if (
    new Set(expected).size !== expected.length ||
    new Set(review.results.map((r) => r.id)).size !== review.results.length ||
    review.results.length !== expected.length ||
    review.results.some((r) => !expected.includes(r.id))
  )
    throw Error("Review must cover every constraint exactly once");
  return {
    passed: review.results.every((r) => r.status === "passed"),
    failed: review.results.filter((r) => r.status === "failed").map((r) => r.id),
    unmeasured: review.results.filter((r) => r.status === "unmeasured").map((r) => r.id),
  };
}

/** Compare masks inside a declared normalized region. Missing/empty silhouettes never pass. */
export function compareSilhouettes(
  a: Uint8Array,
  b: Uint8Array,
  width: number,
  height: number,
  region: [number, number, number, number],
) {
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    a.length !== width * height ||
    b.length !== a.length
  )
    throw Error("Mismatched silhouette dimensions");
  if (
    region.some((v) => !Number.isFinite(v) || v < 0 || v > 1) ||
    region[0] >= region[2] ||
    region[1] >= region[3]
  )
    throw Error("Invalid silhouette region");
  let changed = 0,
    union = 0,
    baseline = 0,
    candidate = 0;
  for (let y = Math.floor(region[1] * height); y < Math.ceil(region[3] * height); y++)
    for (let x = Math.floor(region[0] * width); x < Math.ceil(region[2] * width); x++) {
      const i = y * width + x,
        aa = !!a[i],
        bb = !!b[i];
      baseline += Number(aa);
      candidate += Number(bb);
      union += Number(aa || bb);
      changed += Number(aa !== bb);
    }
  return {
    changedFraction: union ? changed / union : null,
    baselinePixels: baseline,
    candidatePixels: candidate,
  };
}
