import { evaluateCreatureChart } from "@wrela/compiler";
import { type CompiledCharacter, type CreatureCloth, contentKey, type Vec3 } from "@wrela/model";

import { rotateVector } from "./animation";
import type { CreatureRuntimeContext } from "./creature-runtime";

const FIXED_STEP = 1 / 120;
const MAX_PARTICLES = 4096;
const add = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v + b[i]) as Vec3;
const sub = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v - b[i]) as Vec3;
const mul = (a: Vec3, s: number): Vec3 => a.map((v) => v * s) as Vec3;
const dot = (a: Vec3, b: Vec3) => a.reduce((n, v, i) => n + v * b[i], 0);
const distance = (a: Vec3, b: Vec3) => Math.hypot(...sub(a, b));
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const unit = (p: Vec3, fallback: Vec3 = [0, 1, 0]): Vec3 =>
  Math.hypot(...p) > 1e-9 ? mul(p, 1 / Math.hypot(...p)) : fallback;
const finiteVector = (p: unknown): p is Vec3 =>
  Array.isArray(p) &&
  p.length === 3 &&
  p.every((v) => typeof v === "number" && Number.isFinite(v) && Math.abs(v) <= 1e9);
const meshPoint = (artifact: CompiledCharacter, v: number): Vec3 => [
  artifact.mesh.positions[v * 3],
  artifact.mesh.positions[v * 3 + 1],
  artifact.mesh.positions[v * 3 + 2],
];
function skinMatrix(artifact: CompiledCharacter, matrices: Float32Array, v: number): Float64Array {
  const m = new Float64Array(16);
  for (let i = 0; i < 4; i++) {
    const weight = artifact.weights[v * 4 + i],
      index = artifact.jointIndices[v * 4 + i] * 16;
    for (let j = 0; j < 16; j++) m[j] += matrices[index + j] * weight;
  }
  return m;
}
function point(m: Float64Array, p: Vec3): Vec3 {
  return [
    m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
    m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
    m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
  ];
}
function inverseVector(m: Float64Array, v: Vec3): Vec3 {
  const a = m[0],
    b = m[4],
    c = m[8],
    d = m[1],
    e = m[5],
    f = m[9],
    g = m[2],
    h = m[6],
    i = m[10];
  const det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
  if (Math.abs(det) < 1e-8) return [0, 0, 0];
  return [
    ((e * i - f * h) * v[0] + (c * h - b * i) * v[1] + (b * f - c * e) * v[2]) / det,
    ((f * g - d * i) * v[0] + (a * i - c * g) * v[1] + (c * d - a * f) * v[2]) / det,
    ((d * h - e * g) * v[0] + (b * g - a * h) * v[1] + (a * e - b * d) * v[2]) / det,
  ];
}
function world(p: Vec3, context: CreatureRuntimeContext): Vec3 {
  return add(context.position, rotateVector(context.rotation, mul(p, context.scale)));
}
function inverseWorldVector(p: Vec3, context: CreatureRuntimeContext): Vec3 {
  return mul(
    rotateVector([-context.rotation[0], -context.rotation[1], -context.rotation[2], context.rotation[3]], p),
    1 / context.scale,
  );
}
type Constraint = { a: number; b: number; length: number; bend: boolean };
type Binding = {
  source: CreatureCloth;
  key: string;
  rest: Vec3[];
  uv: [number, number][];
  vertices: number[][];
  followers: { vertex: number; particles: [number, number, number]; weights: Vec3 }[];
  triangles: [number, number, number][];
  constraints: Constraint[];
  pins: Map<number, { weight: number; anchor?: { position: Vec3; vertex: number } }>;
  offsets: Map<number, number>;
};
const bindings = new WeakMap<CompiledCharacter, Binding[]>();
function normals(points: Vec3[], triangles: [number, number, number][]): Vec3[] {
  const result = points.map(() => [0, 0, 0] as Vec3);
  for (const [a, b, c] of triangles) {
    const n = cross(sub(points[b], points[a]), sub(points[c], points[a]));
    for (const i of [a, b, c]) result[i] = add(result[i], n);
  }
  return result.map((n) => unit(n));
}
function bind(artifact: CompiledCharacter): Binding[] {
  const cached = bindings.get(artifact);
  if (cached) return cached;
  const result: Binding[] = [];
  let totalParticles = 0;
  for (const source of artifact.creature?.cloth ?? []) {
    const creature = artifact.creature;
    if (!creature) continue;
    const chart = creature.charts.find((c) => c.id === source.chart);
    if (
      !chart ||
      chart.kind !== "patch" ||
      chart.revision !== source.chartRevision ||
      chart.region !== source.region
    )
      throw new Error(`Invalid cloth chart ${source.id}`);
    // A chart-defined physical lattice is independent of display tessellation.
    // Render vertices retain exact rest geometry and follow interpolated motion.
    const renderVertices: { vertex: number; u: number; v: number }[] = [];
    const renderedU = new Set<number>(),
      renderedV = new Set<number>();
    for (
      let vertex = 0;
      vertex < (artifact.creatureBodyVertexCount ?? artifact.mesh.positions.length / 3);
      vertex++
    ) {
      const c = artifact.creatureCoordinates?.[vertex];
      if (c?.chart !== source.chart || c.chartRevision !== source.chartRevision) continue;
      renderVertices.push({ vertex, u: c.coordinates[0], v: c.coordinates[1] });
      renderedU.add(c.coordinates[0]);
      renderedV.add(c.coordinates[1]);
    }
    if (renderVertices.length < 3) throw new Error(`Cloth ${source.id} has no realized chart geometry`);
    if (renderedU.size < 2 || renderedV.size < 2)
      throw new Error(`Cloth ${source.id} has degenerate chart support`);
    const resolution = source.simulationResolution ?? 4;
    const knots = (count: number, axis: 0 | 1) =>
      [
        ...new Set([
          ...Array.from(
            { length: Math.min(resolution, count - 1) + 1 },
            (_, i) => i / Math.min(resolution, count - 1),
          ),
          ...source.pins.map((pin) => pin.coordinates[axis]),
        ]),
      ].sort((a, b) => a - b);
    const us = knots(renderedU.size, 0),
      vs = knots(renderedV.size, 1),
      uv: [number, number][] = [],
      rest: Vec3[] = [],
      vertices: number[][] = [];
    if (totalParticles + us.length * vs.length > MAX_PARTICLES)
      throw new Error(`Creature cloth exceeds total ${MAX_PARTICLES} particle budget`);
    for (const v of vs)
      for (const u of us) {
        uv.push([u, v]);
        rest.push(evaluateCreatureChart(creature, chart.id, [u, v, 0]).position);
        let nearest = renderVertices[0].vertex,
          best = Infinity;
        for (const rendered of renderVertices) {
          const distance = (rendered.u - u) ** 2 + (rendered.v - v) ** 2;
          if (distance < best) {
            best = distance;
            nearest = rendered.vertex;
          }
        }
        vertices.push([nearest]);
      }
    totalParticles += vertices.length;
    if (totalParticles > MAX_PARTICLES)
      throw new Error(`Creature cloth exceeds total ${MAX_PARTICLES} particle budget`);
    const triangles: [number, number, number][] = [];
    for (let v = 0; v < vs.length - 1; v++)
      for (let u = 0; u < us.length - 1; u++) {
        const a = v * us.length + u,
          b = a + 1,
          c = a + us.length,
          d = c + 1;
        triangles.push([a, c, b], [b, c, d]);
      }
    const cell = (knots: number[], value: number) => {
      let i = 0;
      while (i < knots.length - 2 && value > knots[i + 1]) i++;
      return i;
    };
    const followers = renderVertices.map((item) => {
      const u = cell(us, item.u),
        v = cell(vs, item.v),
        fu = (item.u - us[u]) / (us[u + 1] - us[u]),
        fv = (item.v - vs[v]) / (vs[v + 1] - vs[v]);
      const a = v * us.length + u,
        b = a + 1,
        c = a + us.length,
        d = c + 1;
      return fu + fv <= 1
        ? {
            vertex: item.vertex,
            particles: [a, b, c] as [number, number, number],
            weights: [1 - fu - fv, fu, fv] as Vec3,
          }
        : {
            vertex: item.vertex,
            particles: [b, c, d] as [number, number, number],
            weights: [1 - fv, 1 - fu, fu + fv - 1] as Vec3,
          };
    });
    const edges = new Map<string, { a: number; b: number; opposites: number[] }>();
    for (const [a, b, c] of triangles)
      for (const [p, q, opposite] of [
        [a, b, c],
        [b, c, a],
        [c, a, b],
      ]) {
        const key = [p, q].sort((x, y) => x - y).join(":");
        const edge = edges.get(key) ?? { a: p, b: q, opposites: [] };
        edge.opposites.push(opposite);
        edges.set(key, edge);
      }
    const constraints: Constraint[] = [];
    for (const edge of edges.values()) {
      const length = distance(rest[edge.a], rest[edge.b]);
      if (length > 1e-8) constraints.push({ a: edge.a, b: edge.b, length, bend: false });
      if (edge.opposites.length === 2) {
        const [a, b] = edge.opposites;
        const length = distance(rest[a], rest[b]);
        if (length > 1e-8) constraints.push({ a, b, length, bend: true });
      }
    }
    const pins = new Map<number, { weight: number; anchor?: { position: Vec3; vertex: number } }>();
    for (let i = 0; i < uv.length; i++)
      if (source.pinEdges.some((edge) => Math.abs(uv[i][edge[0] === "u" ? 0 : 1] - Number(edge[1])) < 1e-6))
        pins.set(i, { weight: 1 });
    for (const pin of source.pins) {
      const anchor = artifact.creatureAnchors?.find((a) => a.id === pin.anchor);
      if (anchor?.status !== "resolved" || !anchor.position)
        throw new Error(`Unresolved cloth pin anchor ${pin.anchor}`);
      let nearest = 0,
        best = Infinity;
      for (let i = 0; i < uv.length; i++) {
        const d = Math.hypot(uv[i][0] - pin.coordinates[0], uv[i][1] - pin.coordinates[1]);
        if (d < best) {
          best = d;
          nearest = i;
        }
      }
      let vertex = 0,
        error = Infinity;
      const anchorSource = artifact.creature?.anchors.find((a) => a.id === pin.anchor);
      for (let v = 0; v < (artifact.creatureBodyVertexCount ?? artifact.mesh.positions.length / 3); v++) {
        if (anchorSource && artifact.creatureRegions && artifact.creatureRegions[v] !== anchorSource.region)
          continue;
        const d = distance(meshPoint(artifact, v), anchor.position);
        if (d < error) {
          error = d;
          vertex = v;
        }
      }
      if (!Number.isFinite(error))
        throw new Error(`Cloth anchor ${pin.anchor} has no anatomical skin binding`);
      pins.set(nearest, { weight: pin.weight ?? 1, anchor: { position: [...anchor.position], vertex } });
    }
    for (const [index, pin] of pins) if (pin.weight === 0) pins.delete(index);
    if (!pins.size) throw new Error(`Cloth ${source.id} has no realized pins`);
    const restNormals = normals(rest, triangles),
      offsets = new Map<number, number>();
    for (const follower of followers) {
      const c = artifact.creatureCoordinates?.[follower.vertex];
      if (!c) continue;
      const sample = evaluateCreatureChart(creature, chart.id, [c.coordinates[0], c.coordinates[1], 0]);
      const normal = unit(
        follower.particles.reduce(
          (sum, index, corner) => add(sum, mul(restNormals[index], follower.weights[corner])),
          [0, 0, 0] as Vec3,
        ),
      );
      offsets.set(follower.vertex, dot(sub(meshPoint(artifact, follower.vertex), sample.position), normal));
    }
    const key = contentKey({ algorithm: "chart-lattice-pbd-2", chart, uv, triangles, pins: [...pins], rest });
    result.push({ source, key, rest, uv, vertices, followers, triangles, constraints, pins, offsets });
  }
  bindings.set(artifact, result);
  return result;
}

export type CreatureClothState = {
  revision: number;
  accumulator: number;
  panels: { id: string; key: string; positions: Vec3[]; previous: Vec3[] }[];
};
export type CreatureClothContext = CreatureRuntimeContext & {
  capsules?: { a: Vec3; b: Vec3; radius: number }[];
};
export const createCreatureClothState = (): CreatureClothState => ({
  revision: 0,
  accumulator: 0,
  panels: [],
});
export function resetCreatureCloth(state: CreatureClothState) {
  state.accumulator = 0;
  state.panels = [];
  state.revision++;
}
function targets(
  artifact: CompiledCharacter,
  matrices: Float32Array,
  binding: Binding,
  context: CreatureClothContext,
): Vec3[] {
  return binding.rest.map((p, i) => {
    const pin = binding.pins.get(i),
      anchor = pin?.anchor;
    return world(
      point(skinMatrix(artifact, matrices, anchor?.vertex ?? binding.vertices[i][0]), anchor?.position ?? p),
      context,
    );
  });
}
function collision(p: Vec3, radius: number, context: CreatureClothContext): Vec3 {
  let result = p;
  const ground = context.ground?.(p[0], p[2]);
  if (ground) {
    if (!Number.isFinite(ground.height)) throw new Error("Nonfinite cloth ground sample");
    result = [result[0], Math.max(result[1], ground.height + radius), result[2]];
  }
  const sphere = (center: Vec3, r: number, fallback: Vec3 = [0, 1, 0]) => {
    const delta = sub(result, center),
      minimum = r + radius;
    if (Math.hypot(...delta) < minimum) result = add(center, mul(unit(delta, fallback), minimum));
  };
  for (const shape of context.colliders ?? []) sphere(shape.center, shape.radius);
  for (const shape of context.capsules ?? []) {
    const edge = sub(shape.b, shape.a),
      length2 = dot(edge, edge),
      t = Math.max(0, Math.min(1, length2 > 1e-12 ? dot(sub(result, shape.a), edge) / length2 : 0));
    const axis = unit(edge);
    const fallback = unit(cross(axis, Math.abs(axis[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0]));
    sphere(add(shape.a, mul(edge, t)), shape.radius, fallback);
  }
  return result;
}
/** Fixed 120 Hz Verlet/PBD with bounded iterations. Pin targets have authority
 * over collision, so authored attachments never detach to satisfy an obstacle. */
export function stepCreatureCloth(
  artifact: CompiledCharacter,
  matrices: Float32Array,
  state: CreatureClothState,
  context: CreatureClothContext,
  dt: number,
): void {
  if (!Number.isFinite(dt) || dt <= 0 || dt > 0.1 || !Number.isFinite(context.scale) || context.scale <= 0)
    throw new RangeError("Invalid cloth step or scale");
  const products = bind(artifact);
  if (!products.length) return;
  if (
    matrices.length !== artifact.joints.length * 16 ||
    !matrices.every(Number.isFinite) ||
    !finiteVector(context.position) ||
    !context.rotation.every(Number.isFinite)
  )
    throw new Error("Invalid cloth frame input");
  for (const shape of context.colliders ?? [])
    if (!finiteVector(shape.center) || !Number.isFinite(shape.radius) || shape.radius < 0)
      throw new Error("Invalid cloth sphere collision");
  for (const shape of context.capsules ?? [])
    if (
      !finiteVector(shape.a) ||
      !finiteVector(shape.b) ||
      !Number.isFinite(shape.radius) ||
      shape.radius < 0
    )
      throw new Error("Invalid cloth capsule collision");
  const previous = new Map(state.panels.map((p) => [p.id, p]));
  const poses = products.map((product) => targets(artifact, matrices, product, context));
  state.panels = products.map((product, i) => {
    const prior = previous.get(product.source.id);
    if (prior?.key === product.key) return prior;
    return {
      id: product.source.id,
      key: product.key,
      positions: poses[i].map((p) => [...p]),
      previous: poses[i].map((p) => [...p]),
    };
  });
  state.accumulator += dt;
  let steps = 0;
  while (state.accumulator + 1e-12 >= FIXED_STEP && steps++ < 12) {
    for (let panelIndex = 0; panelIndex < products.length; panelIndex++) {
      const product = products[panelIndex],
        panel = state.panels[panelIndex],
        source = product.source,
        pose = poses[panelIndex];
      const attenuation = Math.exp(-source.damping * 60 * FIXED_STEP),
        acceleration = add(source.gravity, source.wind);
      const before = panel.positions.map((p) => [...p] as Vec3);
      for (let i = 0; i < panel.positions.length; i++) {
        if (product.pins.get(i)?.weight === 1) {
          panel.positions[i] = [...pose[i]];
          continue;
        }
        panel.positions[i] = add(
          add(panel.positions[i], mul(sub(panel.positions[i], panel.previous[i]), attenuation)),
          mul(acceleration, FIXED_STEP * FIXED_STEP),
        );
      }
      const stretchCoefficient = 1 - (1 - source.stiffness) ** (1 / source.iterations);
      const bendCoefficient = 1 - (1 - source.bendStiffness) ** (1 / source.iterations);
      const solve = (constraint: Constraint, hard = false) => {
        const a = panel.positions[constraint.a],
          b = panel.positions[constraint.b];
        const dx = b[0] - a[0],
          dy = b[1] - a[1],
          dz = b[2] - a[2],
          length = Math.hypot(dx, dy, dz),
          rest = constraint.length * context.scale;
        if (length < 1e-10) return;
        const target = hard ? Math.min(length, rest * source.maxStretch) : rest;
        const coefficient = hard ? 1 : constraint.bend ? bendCoefficient : stretchCoefficient;
        const wa = 1 - (product.pins.get(constraint.a)?.weight ?? 0),
          wb = 1 - (product.pins.get(constraint.b)?.weight ?? 0);
        if (wa + wb === 0) return;
        const factor = (((length - target) / length) * coefficient) / (wa + wb),
          fa = factor * wa,
          fb = factor * wb;
        a[0] += dx * fa;
        a[1] += dy * fa;
        a[2] += dz * fa;
        b[0] -= dx * fb;
        b[1] -= dy * fb;
        b[2] -= dz * fb;
      };
      for (let iteration = 0; iteration < source.iterations; iteration++) {
        for (const constraint of product.constraints) solve(constraint);
        for (const constraint of product.constraints) if (!constraint.bend) solve(constraint, true);
        for (let i = 0; i < panel.positions.length; i++) {
          const weight = product.pins.get(i)?.weight ?? 0;
          if (weight === 1) panel.positions[i] = [...pose[i]];
          else {
            if (weight > 0)
              panel.positions[i] = add(
                panel.positions[i],
                mul(sub(pose[i], panel.positions[i]), 1 - (1 - weight) ** (1 / source.iterations)),
              );
            // Interleave collision with every four strain passes, and always finish
            // with collision projection. This bounds expensive world queries while
            // retaining an exact final obstacle projection each 120 Hz substep.
            if (iteration % 4 === 3 || iteration === source.iterations - 1)
              panel.positions[i] = collision(
                panel.positions[i],
                source.collisionRadius * context.scale,
                context,
              );
          }
        }
      }
      if (!panel.positions.every(finiteVector))
        throw new Error(`Cloth simulation exceeded finite bounds: ${source.id}`);
      panel.previous = before;
      for (const [i, pin] of product.pins) if (pin.weight === 1) panel.previous[i] = [...pose[i]];
    }
    state.accumulator = Math.max(0, state.accumulator - FIXED_STEP);
  }
  state.revision++;
}
/** Dense pre-skin deltas. The same patch body indices survive groom detail changes. */
export function creatureClothOffsets(
  artifact: CompiledCharacter,
  matrices: Float32Array,
  state: CreatureClothState,
  context: CreatureClothContext,
): Float32Array | undefined {
  const products = bind(artifact);
  if (!products.length || !state.panels.length) return;
  const result = new Float32Array(artifact.mesh.positions.length),
    panels = new Map(state.panels.map((p) => [p.id, p]));
  for (const product of products) {
    const panel = panels.get(product.source.id);
    if (!panel || panel.key !== product.key) continue;
    const dynamicNormals = normals(panel.positions, product.triangles);
    const posedRest = product.rest.map((p, i) =>
      world(point(skinMatrix(artifact, matrices, product.vertices[i][0]), p), context),
    );
    const restNormals = normals(posedRest, product.triangles);
    for (const follower of product.followers) {
      const vertex = follower.vertex,
        m = skinMatrix(artifact, matrices, vertex);
      const delta = follower.particles.reduce(
        (sum, index, corner) =>
          add(sum, mul(sub(panel.positions[index], posedRest[index]), follower.weights[corner])),
        [0, 0, 0] as Vec3,
      );
      const normalChange = follower.particles.reduce(
        (sum, index, corner) =>
          add(sum, mul(sub(dynamicNormals[index], restNormals[index]), follower.weights[corner])),
        [0, 0, 0] as Vec3,
      );
      const displacement = add(delta, mul(normalChange, (product.offsets.get(vertex) ?? 0) * context.scale));
      result.set(inverseVector(m, inverseWorldVector(displacement, context)), vertex * 3);
    }
  }
  return result;
}
export function validateCreatureClothState(value: unknown, artifact: CompiledCharacter): CreatureClothState {
  const state = value as CreatureClothState,
    products = bind(artifact);
  if (
    !state ||
    !Number.isSafeInteger(state.revision) ||
    state.revision < 0 ||
    !Number.isFinite(state.accumulator) ||
    state.accumulator < 0 ||
    state.accumulator >= FIXED_STEP + 1e-9 ||
    !Array.isArray(state.panels) ||
    state.panels.length > products.length
  )
    throw new Error("Invalid saved cloth state");
  const seen = new Set<string>();
  for (const panel of state.panels) {
    const product = products.find((p) => p.source.id === panel.id);
    if (
      !product ||
      seen.has(panel.id) ||
      product.key !== panel.key ||
      !Array.isArray(panel.positions) ||
      !Array.isArray(panel.previous) ||
      panel.positions.length !== product.rest.length ||
      panel.previous.length !== product.rest.length ||
      ![...panel.positions, ...panel.previous].every(finiteVector)
    )
      throw new Error("Invalid saved cloth panel");
    seen.add(panel.id);
  }
  return structuredClone(state);
}

/** Iterative constraints can remain unsatisfied when pins and collisions conflict.
 * Report measured residuals rather than treating the iteration budget as a proof. */
export function creatureClothDiagnostics(
  artifact: CompiledCharacter,
  matrices: Float32Array,
  state: CreatureClothState,
  context: CreatureClothContext,
): {
  id: string;
  particles: number;
  maximumStretch: number;
  stretchResidual: number;
  maximumPinError: number;
}[] {
  const panels = new Map(state.panels.map((panel) => [panel.id, panel]));
  return bind(artifact).flatMap((product) => {
    const panel = panels.get(product.source.id);
    if (!panel || panel.key !== product.key) return [];
    let maximumStretch = 0,
      maximumPinError = 0;
    for (const constraint of product.constraints)
      if (!constraint.bend)
        maximumStretch = Math.max(
          maximumStretch,
          distance(panel.positions[constraint.a], panel.positions[constraint.b]) /
            (constraint.length * context.scale),
        );
    const pose = targets(artifact, matrices, product, context);
    for (const index of product.pins.keys())
      maximumPinError = Math.max(maximumPinError, distance(panel.positions[index], pose[index]));
    return [
      {
        id: product.source.id,
        particles: panel.positions.length,
        maximumStretch,
        stretchResidual: Math.max(0, maximumStretch - product.source.maxStretch),
        maximumPinError,
      },
    ];
  });
}
