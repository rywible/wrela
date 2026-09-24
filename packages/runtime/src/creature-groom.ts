import type { CompiledCharacter, CompiledGroomGuide, Vec3 } from "@wrela/model";

import { rotateVector } from "./animation";
import type { CreatureRuntimeContext } from "./creature-runtime";
import { projectCreatureCollision, rotationBetween } from "./creature-runtime";

const add = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v + b[i]) as Vec3;
const sub = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v - b[i]) as Vec3;
const mul = (a: Vec3, s: number): Vec3 => a.map((v) => v * s) as Vec3;
const distance = (a: Vec3, b: Vec3) => Math.hypot(...sub(a, b));
const unit = (v: Vec3, fallback: Vec3): Vec3 =>
  Math.hypot(...v) > 1e-8 ? mul(v, 1 / Math.hypot(...v)) : fallback;

/** A bounded set of physical guides drives every visible tuft, independently of
 * display LOD. Followers share a layer, retain their own roots and scale/rotate
 * the driver's rest-space displacement into their authored growth direction. */
type GroomBinding = {
  selected: number[];
  masters: Uint32Array;
  vertices: number[][];
  parameters: Float32Array;
  bodyCount: number;
};
const bindings = new WeakMap<CompiledCharacter, GroomBinding>();
function bind(artifact: CompiledCharacter): GroomBinding | undefined {
  const cached = bindings.get(artifact);
  if (cached) return cached;
  const groom = artifact.creatureGroom,
    detail =
      groom?.details.find((value) => value.label === artifact.creatureGroomDetail) ?? groom?.details[0],
    bodyCount = artifact.creatureBodyVertexCount;
  if (!groom?.guides.length || !detail || bodyCount === undefined) return;
  const vertices: number[][] = groom.guides.map(() => []);
  for (let i = 0; i < detail.vertexGuideIndices.length; i++)
    vertices[detail.vertexGuideIndices[i]].push(bodyCount + i);
  const groups = new Map<string, number[]>();
  for (let i = 0; i < groom.guides.length; i++) {
    if (!vertices[i].length) continue;
    const layer = groom.guides[i].root.layer,
      group = groups.get(layer) ?? [];
    group.push(i);
    groups.set(layer, group);
  }
  const total = [...groups.values()].reduce((sum, group) => sum + group.length, 0),
    budget = Math.min(128, total);
  const selected: number[] = [];
  const masters = new Uint32Array(groom.guides.length);
  let remaining = budget;
  const entries = [...groups.values()];
  for (let g = 0; g < entries.length; g++) {
    const group = entries[g];
    const count = Math.max(
      1,
      Math.min(
        group.length,
        remaining - (entries.length - g - 1),
        Math.ceil((budget * group.length) / total),
      ),
    );
    const chosen = Array.from({ length: count }, (_, i) => group[Math.floor((i * group.length) / count)]);
    remaining -= chosen.length;
    selected.push(...chosen);
    for (const index of group) {
      let nearest = chosen[0],
        residual = Infinity;
      for (const candidate of chosen) {
        const d = distance(groom.guides[index].points[0], groom.guides[candidate].points[0]);
        if (d < residual) {
          residual = d;
          nearest = candidate;
        }
      }
      masters[index] = nearest;
    }
  }
  const parameters = new Float32Array(detail.vertexGuideIndices.length);
  for (let i = 0; i < detail.vertexGuideIndices.length; i++) {
    const guide = groom.guides[detail.vertexGuideIndices[i]],
      offset = (bodyCount + i) * 3;
    const p: Vec3 = [
      artifact.mesh.positions[offset],
      artifact.mesh.positions[offset + 1],
      artifact.mesh.positions[offset + 2],
    ];
    let residual = Infinity,
      parameter = 0;
    for (let segment = 0; segment < guide.points.length - 1; segment++) {
      const a = guide.points[segment],
        delta = sub(guide.points[segment + 1], a),
        length2 = delta.reduce((sum, value) => sum + value * value, 0);
      const t = Math.max(
        0,
        Math.min(
          1,
          length2 > 1e-12
            ? sub(p, a).reduce((sum, value, axis) => sum + value * delta[axis], 0) / length2
            : 0,
        ),
      );
      const d = distance(p, add(a, mul(delta, t)));
      if (d < residual) {
        residual = d;
        parameter = (segment + t) / (guide.points.length - 1);
      }
    }
    parameters[i] = parameter;
  }
  const result = { selected, masters, vertices, parameters, bodyCount };
  bindings.set(artifact, result);
  return result;
}

export type CreatureGroomState = {
  revision: number;
  guides: { index: number; positions: Vec3[]; previous: Vec3[] }[];
};
export const createCreatureGroomState = (): CreatureGroomState => ({ revision: 0, guides: [] });

function skinMatrix(artifact: CompiledCharacter, matrices: Float32Array, vertex: number): Float64Array {
  const result = new Float64Array(16);
  for (let influence = 0; influence < 4; influence++) {
    const index = vertex * 4 + influence,
      joint = artifact.jointIndices[index] * 16,
      weight = artifact.weights[index];
    for (let c = 0; c < 16; c++) result[c] += matrices[joint + c] * weight;
  }
  return result;
}
function inverseVector(matrix: Float64Array, v: Vec3): Vec3 {
  const a = matrix[0],
    b = matrix[4],
    c = matrix[8],
    d = matrix[1],
    e = matrix[5],
    f = matrix[9],
    g = matrix[2],
    h = matrix[6],
    i = matrix[10];
  const determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
  // Collapsed LBS frames have no stable inverse: suppress guide offsets there.
  if (Math.abs(determinant) < 1e-8) return [0, 0, 0];
  return [
    ((e * i - f * h) * v[0] + (c * h - b * i) * v[1] + (b * f - c * e) * v[2]) / determinant,
    ((f * g - d * i) * v[0] + (a * i - c * g) * v[1] + (c * d - a * f) * v[2]) / determinant,
    ((d * h - e * g) * v[0] + (b * g - a * h) * v[1] + (a * e - b * d) * v[2]) / determinant,
  ];
}
function posedPoints(
  guide: CompiledGroomGuide,
  matrix: Float64Array,
  context: CreatureRuntimeContext,
): Vec3[] {
  const [qx, qy, qz, qw] = context.rotation;
  const m00 = 1 - 2 * (qy * qy + qz * qz),
    m01 = 2 * (qx * qy - qz * qw),
    m02 = 2 * (qx * qz + qy * qw);
  const m10 = 2 * (qx * qy + qz * qw),
    m11 = 1 - 2 * (qx * qx + qz * qz),
    m12 = 2 * (qy * qz - qx * qw);
  const m20 = 2 * (qx * qz - qy * qw),
    m21 = 2 * (qy * qz + qx * qw),
    m22 = 1 - 2 * (qx * qx + qy * qy);
  return guide.points.map((p) => {
    const x = (matrix[0] * p[0] + matrix[4] * p[1] + matrix[8] * p[2] + matrix[12]) * context.scale;
    const y = (matrix[1] * p[0] + matrix[5] * p[1] + matrix[9] * p[2] + matrix[13]) * context.scale;
    const z = (matrix[2] * p[0] + matrix[6] * p[1] + matrix[10] * p[2] + matrix[14]) * context.scale;
    return [
      context.position[0] + m00 * x + m01 * y + m02 * z,
      context.position[1] + m10 * x + m11 * y + m12 * z,
      context.position[2] + m20 * x + m21 * y + m22 * z,
    ];
  });
}

export function stepCreatureGroom(
  artifact: CompiledCharacter,
  matrices: Float32Array,
  state: CreatureGroomState,
  context: CreatureRuntimeContext,
  dt: number,
) {
  if (!Number.isFinite(dt) || dt <= 0 || dt > 0.1) throw new RangeError("Invalid groom step");
  const binding = bind(artifact),
    groom = artifact.creatureGroom;
  if (!binding || !groom) return;
  const existing = new Map(state.guides.map((guide) => [guide.index, guide]));
  state.guides = binding.selected.map((index) => {
    const guide = groom.guides[index],
      matrix = skinMatrix(artifact, matrices, binding.vertices[index][0]);
    const rest = posedPoints(guide, matrix, context),
      previous = existing.get(index);
    const positions = previous?.positions ?? rest.map((p) => [...p] as Vec3);
    const history = previous?.previous ?? rest.map((p) => [...p] as Vec3);
    const attenuation = Math.exp(-guide.damping * dt),
      spring = 1 - 1 / (1 + guide.stiffness * dt * dt);
    for (let i = 0; i < positions.length; i++) {
      const position = positions[i],
        before = history[i];
      for (let axis = 0; axis < 3; axis++) {
        const value = position[axis];
        const predicted =
          value + (previous ? (value - before[axis]) * attenuation : 0) - (axis === 1 ? 9.81 * dt * dt : 0);
        before[axis] = value;
        position[axis] = i === 0 ? rest[0][axis] : predicted + (rest[i][axis] - predicted) * spring;
      }
    }
    const lengths = rest.slice(1).map((p, i) => distance(p, rest[i]));
    const directions = rest.slice(1).map((p, i) => unit(sub(p, rest[i]), [0, 1, 0]));
    const radius = guide.width * context.scale * 0.5;
    for (let pass = 0; pass < 4; pass++) {
      let collided = false;
      for (let axis = 0; axis < 3; axis++) positions[0][axis] = rest[0][axis];
      for (let i = 1; i < positions.length; i++) {
        const previousPoint = positions[i - 1],
          current = positions[i];
        const dx = current[0] - previousPoint[0],
          dy = current[1] - previousPoint[1],
          dz = current[2] - previousPoint[2];
        const squared = dx * dx + dy * dy + dz * dz;
        if (squared > 1e-16) {
          const scale = lengths[i - 1] / Math.sqrt(squared);
          current[0] = previousPoint[0] + dx * scale;
          current[1] = previousPoint[1] + dy * scale;
          current[2] = previousPoint[2] + dz * scale;
        } else positions[i] = add(previousPoint, mul(directions[i - 1], lengths[i - 1]));
        const ground = context.ground?.(positions[i][0], positions[i][2]);
        if (ground && positions[i][1] < ground.height + radius) {
          positions[i][1] = ground.height + radius;
          collided = true;
        }
        const projected = projectCreatureCollision(positions[i], radius, context);
        collided ||= projected !== positions[i];
        positions[i] = projected;
      }
      // Forward length projection is already exact in one pass. Extra passes
      // exist only to resolve constraints disturbed by collision projections.
      if (!collided) break;
    }
    return { index, positions, previous: history };
  });
  state.revision++;
}

/** Dense rest-space offsets for the immutable hero mesh; all tufts retain their
 * root and respond to a bounded driver set. Consumers recompute changed normals. */
export function creatureGroomOffsets(
  artifact: CompiledCharacter,
  matrices: Float32Array,
  state: CreatureGroomState,
  context: CreatureRuntimeContext,
  renderArtifact: CompiledCharacter = artifact,
): Float32Array | undefined {
  const driverBinding = bind(artifact),
    binding = bind(renderArtifact),
    groom = artifact.creatureGroom;
  if (!driverBinding || !binding || !groom || !state.guides.length) return;
  const inverseBody = [
    -context.rotation[0],
    -context.rotation[1],
    -context.rotation[2],
    context.rotation[3],
  ] as [number, number, number, number];
  const masterOffsets = new Map<number, Vec3[]>();
  for (const simulation of state.guides) {
    const guide = groom.guides[simulation.index],
      matrix = skinMatrix(artifact, matrices, driverBinding.vertices[simulation.index][0]);
    const rest = posedPoints(guide, matrix, context);
    masterOffsets.set(
      simulation.index,
      simulation.positions.map((p, i) =>
        i === 0
          ? [0, 0, 0]
          : inverseVector(matrix, mul(rotateVector(inverseBody, sub(p, rest[i])), 1 / context.scale)),
      ),
    );
  }
  const result = new Float32Array(renderArtifact.mesh.positions.length);
  for (let index = 0; index < groom.guides.length; index++) {
    const master = driverBinding.masters[index],
      offsets = masterOffsets.get(master);
    if (!offsets) continue;
    const source = groom.guides[master],
      guide = groom.guides[index];
    const sourceDirection = sub(source.points[source.points.length - 1], source.points[0]),
      targetDirection = sub(guide.points[guide.points.length - 1], guide.points[0]);
    const rotation = rotationBetween(sourceDirection, targetDirection),
      scale = Math.hypot(...targetDirection) / Math.max(1e-8, Math.hypot(...sourceDirection));
    const [qx, qy, qz, qw] = rotation;
    const m00 = 1 - 2 * (qy * qy + qz * qz),
      m01 = 2 * (qx * qy - qz * qw),
      m02 = 2 * (qx * qz + qy * qw);
    const m10 = 2 * (qx * qy + qz * qw),
      m11 = 1 - 2 * (qx * qx + qz * qz),
      m12 = 2 * (qy * qz - qx * qw);
    const m20 = 2 * (qx * qz - qy * qw),
      m21 = 2 * (qy * qz + qx * qw),
      m22 = 1 - 2 * (qx * qx + qy * qy);
    for (const vertex of binding.vertices[index]) {
      const t = binding.parameters[vertex - binding.bodyCount] * (offsets.length - 1),
        a = Math.min(offsets.length - 2, Math.floor(t)),
        weight = t - a;
      const x = offsets[a][0] + (offsets[a + 1][0] - offsets[a][0]) * weight;
      const y = offsets[a][1] + (offsets[a + 1][1] - offsets[a][1]) * weight;
      const z = offsets[a][2] + (offsets[a + 1][2] - offsets[a][2]) * weight;
      result[vertex * 3] = (m00 * x + m01 * y + m02 * z) * scale;
      result[vertex * 3 + 1] = (m10 * x + m11 * y + m12 * z) * scale;
      result[vertex * 3 + 2] = (m20 * x + m21 * y + m22 * z) * scale;
    }
  }
  return result;
}

export function validateCreatureGroomState(value: unknown, artifact: CompiledCharacter): CreatureGroomState {
  const state = value as CreatureGroomState,
    binding = bind(artifact),
    groom = artifact.creatureGroom;
  if (
    !state ||
    !Number.isSafeInteger(state.revision) ||
    state.revision < 0 ||
    !Array.isArray(state.guides) ||
    state.guides.length > 128 ||
    (!binding && state.guides.length) ||
    (state.guides.length > 0 && state.guides.length !== binding?.selected.length)
  )
    throw new Error("Invalid saved groom state");
  const ids = new Set<number>();
  for (const simulation of state.guides) {
    const count = groom?.guides[simulation.index]?.points.length;
    if (
      !binding?.selected.includes(simulation.index) ||
      ids.has(simulation.index) ||
      !count ||
      !Array.isArray(simulation.positions) ||
      !Array.isArray(simulation.previous) ||
      simulation.positions.length !== count ||
      simulation.previous.length !== count ||
      ![...simulation.positions, ...simulation.previous].every(
        (p) =>
          Array.isArray(p) && p.length === 3 && p.every((v) => typeof v === "number" && Number.isFinite(v)),
      )
    )
      throw new Error("Invalid saved groom guide");
    ids.add(simulation.index);
  }
  return structuredClone(state);
}
