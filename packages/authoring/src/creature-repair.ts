import {
  add,
  type CharacterDefinition,
  type CreatureSculpt,
  contentKey,
  creatureSculptSchema,
  evaluateCreatureSculpt,
  idSchema,
  sculptRotate,
  sculptSupport,
  sub,
  type Vec3,
  vec3Schema,
} from "@wrela/model";

import { z } from "zod";
import type { EditBatch } from "./commands";

const point = z.strictObject({
  id: idSchema,
  position: vec3Schema,
  target: vec3Schema,
  tolerance: z.number().finite().min(0.00001).max(1),
});
export const creatureRepairRequestSchema = z.strictObject({
  id: idSchema,
  target: idSchema,
  sourceKey: z.string(),
  expectedRevision: z.number().int().min(0),
  region: idSchema,
  nodeId: idSchema.optional(),
  handles: z.array(point).min(1).max(16),
  protectedPoints: z
    .array(
      z.strictObject({
        id: idSchema,
        position: vec3Schema,
        tolerance: z.number().finite().min(0.00001).max(1),
      }),
    )
    .max(32)
    .default([]),
  support: z.strictObject({
    radii: z.tuple([
      z.number().finite().min(0.005).max(2),
      z.number().finite().min(0.005).max(2),
      z.number().finite().min(0.005).max(2),
    ]),
    rotation: vec3Schema,
  }),
  maxDisplacement: z.number().finite().min(0.001).max(0.5).default(0.15),
  detail: z
    .strictObject({
      maxEdgeLength: z.number().finite().min(0.002).max(0.2),
      passes: z.number().int().min(1).max(6),
    })
    .optional(),
  intent: z.string().min(1).max(1000),
});
export type CreatureRepairRequest = z.input<typeof creatureRepairRequestSchema>;

/** Rest-space fitting, deliberately independent of renderer and compiler. A surface
 * observation provides rest points; captures and posed checks establish visual acceptance. */
export function proposeCreatureRepair(character: CharacterDefinition, input: CreatureRepairRequest) {
  const request = creatureRepairRequestSchema.parse(input),
    creature = character.creature;
  if (!creature || request.target !== character.id || request.sourceKey !== contentKey(character))
    throw Error("Repair source is stale or belongs to another character");
  const region = creature.regions.find((r) => r.id === request.region);
  if (!region) throw Error("Repair region is missing");
  if (new Set(request.handles.map((h) => h.id)).size !== request.handles.length)
    throw Error("Repair handle IDs must be unique");
  const local = (p: Vec3) => sculptRotate(sub(p, region.frame.position), region.frame.rotation, true);
  const rest = (p: Vec3) => add(region.frame.position, sculptRotate(p, region.frame.rotation));
  if (
    request.nodeId &&
    !region.nodeIds.includes(request.nodeId) &&
    !creature.charts.some((c) => c.region === region.id && c.id === request.nodeId)
  )
    throw Error("Repair surface scope is outside its anatomical region");
  if (!request.nodeId && creature.sculpts.some((s) => s.region === region.id && s.nodeIds))
    throw Error("This region has surface-scoped sculpt layers; provide an explicit nodeId for a repair");
  const existing = creature.sculpts.filter(
    (s) => !s.nodeIds || (request.nodeId && s.nodeIds.includes(request.nodeId)),
  );
  const current = (p: Vec3) => evaluateCreatureSculpt(existing, region.id, p);
  const invert = (observed: Vec3) => {
    let p: Vec3 = [...observed];
    for (let i = 0; i < 48; i++) {
      const error = sub(observed, current(p));
      if (Math.hypot(...error) < 1e-7) return p;
      p = add(p, error);
    }
    throw Error(
      "Existing sculpt cannot be inverted reliably at this handle; reduce the edit or use an explicit source point",
    );
  };
  const handles = request.handles.map((h) => ({
    ...h,
    raw: invert(local(h.position)),
    delta: sub(local(h.target), local(h.position)),
  }));
  const protectedPoints = request.protectedPoints.map((h) => ({
    ...h,
    raw: invert(local(h.position)),
    delta: [0, 0, 0] as Vec3,
  }));
  const strokes: CreatureSculpt[] = handles.map((h, i) =>
    creatureSculptSchema.parse({
      id: `${request.id.slice(0, 85)}-${i}`,
      region: region.id,
      nodeIds: request.nodeId ? [request.nodeId] : undefined,
      center: h.raw,
      radius: Math.max(...request.support.radii),
      support: request.support,
      displacement: [0, 0, 0],
      strength: 1,
      falloff: 2,
      mirror: false,
      detail: request.detail,
    }),
  );
  if (strokes.some((s) => creature.sculpts.some((prior) => prior.id === s.id)))
    throw Error("Repair would overwrite an existing sculpt; use a new repair ID");
  const samples = [...handles, ...protectedPoints];
  const basis = samples.map((h) => strokes.map((s) => sculptSupport(s, h.raw).weight));
  // Weighted regularized normal equations. Explicit residual acceptance below is
  // authoritative: a protected sample is never silently traded away by a penalty.
  const n = strokes.length,
    matrix = Array.from({ length: n }, () => Array(n).fill(0) as number[]),
    rhs = Array.from({ length: n }, () => [0, 0, 0] as Vec3);
  for (let row = 0; row < samples.length; row++) {
    const weight = 1 / Math.max(1e-10, samples[row].tolerance ** 2);
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) matrix[i][j] += basis[row][i] * basis[row][j] * weight;
      for (let axis = 0; axis < 3; axis++) rhs[i][axis] += basis[row][i] * samples[row].delta[axis] * weight;
    }
  }
  const regularization = Math.max(1, ...matrix.map((r, i) => r[i])) * 1e-9;
  for (let i = 0; i < n; i++) matrix[i][i] += regularization;
  for (let column = 0; column < n; column++) {
    let pivot = column;
    for (let row = column + 1; row < n; row++)
      if (Math.abs(matrix[row][column]) > Math.abs(matrix[pivot][column])) pivot = row;
    [matrix[column], matrix[pivot]] = [matrix[pivot], matrix[column]];
    [rhs[column], rhs[pivot]] = [rhs[pivot], rhs[column]];
    const d = matrix[column][column];
    if (Math.abs(d) < 1e-14) throw Error("Repair controls are unidentifiable");
    for (let j = column; j < n; j++) matrix[column][j] /= d;
    for (let axis = 0; axis < 3; axis++) rhs[column][axis] /= d;
    for (let row = 0; row < n; row++)
      if (row !== column) {
        const factor = matrix[row][column];
        for (let j = column; j < n; j++) matrix[row][j] -= factor * matrix[column][j];
        for (let axis = 0; axis < 3; axis++) rhs[row][axis] -= factor * rhs[column][axis];
      }
  }
  strokes.forEach((s, i) => {
    s.displacement = rhs[i];
  });
  // Sum of coefficient lengths is a conservative displacement bound everywhere.
  const bound = rhs.reduce((sum, v) => sum + Math.hypot(...v), 0);
  if (bound > request.maxDisplacement)
    throw Error(`Repair exceeds its global displacement bound (${bound.toFixed(4)} m)`);
  const gradientBound = (bound * 1.54) / Math.min(...request.support.radii);
  if (gradientBound >= 0.9)
    throw Error("Repair is too steep for its support; widen the influence or reduce the displacement");
  const residuals = samples.map((h, i) => {
    const after = rest(evaluateCreatureSculpt([...existing, ...strokes], region.id, h.raw));
    const target = i < handles.length ? request.handles[i].target : h.position;
    const error = Math.hypot(...sub(after, target));
    return {
      id: h.id,
      protected: i >= handles.length,
      position: after,
      target,
      error,
      tolerance: h.tolerance,
      passed: error <= h.tolerance,
    };
  });
  if (residuals.some((r) => !r.passed))
    throw Error(
      `Repair constraints conflict: ${residuals
        .filter((r) => !r.passed)
        .map((r) => r.id)
        .join(", ")}`,
    );
  const batch: EditBatch = {
    expectedRevision: request.expectedRevision,
    operations: strokes.map((value) => ({ kind: "creature.sculpt", target: character.id, value })),
    intent: request.intent,
  };
  return {
    id: request.id,
    sourceKey: request.sourceKey,
    batch,
    residuals,
    maxDisplacementBound: bound,
    addedFieldGradientBound: gradientBound,
    sensitivities: samples.map((h, i) => ({
      sample: h.id,
      controls: strokes.map((s, j) => ({ id: s.id, weight: basis[i][j] })),
    })),
    scope:
      "Character-rest displacement field fit. Surface tessellation, appearance displacement, binding and posed quality require matched compiled captures; no automatic visual acceptance.",
  };
}
