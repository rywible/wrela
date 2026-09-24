import type { Vec3 } from "@wrela/model";

import { rotateVector } from "./animation";
import type { DiagnosticCollider } from "./physics";
import type { RuntimeSession } from "./session";

export type DiagnosticSegment = {
  kind: "rig" | "collider";
  a: Vec3;
  b: Vec3;
  instanceId: string;
  documentId?: string;
  jointId?: string;
  colliderId?: string;
};
export type RuntimeDiagnostics = {
  segments: DiagnosticSegment[];
  tick: number;
  time: number;
  origin: Vec3;
  truncated: boolean;
};
type EvaluatedCharacter = ReturnType<RuntimeSession["evaluatedCharacters"]>[number];
const point = (matrix: ArrayLike<number>, p: Vec3, offset = 0): Vec3 => [
  matrix[offset] * p[0] + matrix[offset + 4] * p[1] + matrix[offset + 8] * p[2] + matrix[offset + 12],
  matrix[offset + 1] * p[0] + matrix[offset + 5] * p[1] + matrix[offset + 9] * p[2] + matrix[offset + 13],
  matrix[offset + 2] * p[0] + matrix[offset + 6] * p[1] + matrix[offset + 10] * p[2] + matrix[offset + 14],
];

/** Character matrices already include any requested render-origin subtraction. */
export function rigDiagnosticSegments(character: EvaluatedCharacter): DiagnosticSegment[] {
  const anchors = new Map(
    character.artifact.joints.map((joint, index) => [
      joint.id,
      point(character.matrix, point(character.skinMatrices, joint.position, index * 16)),
    ]),
  );
  const segments: DiagnosticSegment[] = [];
  for (const joint of character.artifact.joints) {
    const anchor = anchors.get(joint.id);
    if (!anchor) continue;
    const metadata = {
      kind: "rig" as const,
      instanceId: character.id,
      documentId: character.artifact.id,
      jointId: joint.id,
    };
    const parent = joint.parent ? anchors.get(joint.parent) : undefined;
    if (parent) segments.push({ ...metadata, a: parent, b: anchor });
    // A small cross makes root and isolated joints visible as well as bones.
    const radius = Math.max(0.012, Math.min(0.06, joint.radius * 0.08)) * character.scale;
    for (let axis = 0; axis < 3; axis++) {
      const a = [...anchor] as Vec3,
        b = [...anchor] as Vec3;
      a[axis] -= radius;
      b[axis] += radius;
      segments.push({ ...metadata, a, b });
    }
  }
  return segments;
}

/** Wire geometry uses the installed primitive dimensions, including authored scale. */
export function colliderDiagnosticSegments(
  collider: DiagnosticCollider,
  origin: Vec3 = [0, 0, 0],
): DiagnosticSegment[] {
  const lines: [Vec3, Vec3][] = [];
  const line = (a: Vec3, b: Vec3) => lines.push([a, b]);
  const ring = (at: (angle: number) => Vec3, start = 0, end = Math.PI * 2, steps = 24) => {
    for (let index = 0; index < steps; index++)
      line(at(start + ((end - start) * index) / steps), at(start + ((end - start) * (index + 1)) / steps));
  };
  if (collider.shape === "box") {
    const corners = Array.from(
      { length: 8 },
      (_, index) => collider.size.map((value, axis) => value * (index & (1 << axis) ? 1 : -1)) as Vec3,
    );
    for (let index = 0; index < 8; index++)
      for (let axis = 0; axis < 3; axis++)
        if (!(index & (1 << axis))) line(corners[index], corners[index | (1 << axis)]);
  } else if (collider.shape === "sphere") {
    for (let axis = 0; axis < 3; axis++)
      ring((angle) => {
        const p: Vec3 = [0, 0, 0];
        p[(axis + 1) % 3] = collider.radius * Math.cos(angle);
        p[(axis + 2) % 3] = collider.radius * Math.sin(angle);
        return p;
      });
  } else {
    const { radius, halfHeight } = collider;
    for (const sign of [-1, 1])
      ring((angle) => [radius * Math.cos(angle), sign * halfHeight, radius * Math.sin(angle)]);
    for (let meridian = 0; meridian < 4; meridian++) {
      const angle = (meridian * Math.PI) / 2,
        x = Math.cos(angle),
        z = Math.sin(angle);
      line([radius * x, -halfHeight, radius * z], [radius * x, halfHeight, radius * z]);
      for (const sign of [-1, 1])
        ring(
          (theta) => [
            radius * Math.cos(theta) * x,
            sign * (halfHeight + radius * Math.sin(theta)),
            radius * Math.cos(theta) * z,
          ],
          0,
          Math.PI / 2,
          6,
        );
    }
  }
  const transform = (p: Vec3): Vec3 =>
    rotateVector(collider.rotation, p).map(
      (value, axis) => value + collider.position[axis] - origin[axis],
    ) as Vec3;
  return lines.map(([a, b]) => ({
    kind: "collider",
    instanceId: collider.instanceId,
    colliderId: collider.colliderId,
    a: transform(a),
    b: transform(b),
  }));
}

/** Bounded, read-only geometry for capture overlays; origin is explicitly recorded. */
export function runtimeDiagnostics(
  runtime: RuntimeSession,
  options: { time?: number; origin?: Vec3; maxSegments?: number } = {},
): RuntimeDiagnostics {
  const time = options.time ?? runtime.clock.time,
    origin = options.origin ?? [0, 0, 0],
    budget = options.maxSegments ?? 65536;
  if (
    !Number.isFinite(time) ||
    origin.some((value) => !Number.isFinite(value)) ||
    !Number.isInteger(budget) ||
    budget < 1 ||
    budget > 65536
  )
    throw new RangeError("Invalid diagnostic capture limits");
  const segments: DiagnosticSegment[] = [];
  let truncated = false;
  const append = (lines: DiagnosticSegment[]) => {
    const remaining = budget - segments.length;
    if (lines.length > remaining) truncated = true;
    segments.push(...lines.slice(0, remaining));
  };
  const characters = runtime.evaluatedCharacters(time, origin);
  for (const character of characters) append(rigDiagnosticSegments(character));
  const documents = new Map(characters.map((character) => [character.id, character.artifact.id]));
  for (const collider of runtime.physics.diagnosticColliders()) {
    if (segments.length >= budget) {
      truncated = true;
      break;
    }
    append(
      colliderDiagnosticSegments(collider, origin).map((line) => ({
        ...line,
        documentId: documents.get(collider.instanceId),
      })),
    );
  }
  return { segments, tick: runtime.clock.tick, time, origin: [...origin], truncated };
}
