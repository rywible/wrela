import { clamp, type FieldDefinition, type FieldNode, normalize, type Vec3 } from "@wrela/model";

import { type FieldIR, lowerField, outsideDistanceBound } from "./ir";

export type FieldSample = { distance: number; source: string; material?: string };
export type FieldProgram = {
  sample(point: Vec3): FieldSample;
  distance(point: Vec3): number;
  normal(point: Vec3, epsilon?: number): Vec3;
  classification: "implicit";
  ir: FieldIR;
  metrics: { evaluatedNodes: number; prunedNodes: number };
};

/** XYZ intrinsic rotations; group transforms compose with each child. size is a
 * primitive's half extent, capsule segment half length, or torus major radius.
 * Values are implicit: ellipsoids are distance estimates, never tracing steps. */
export function compileField(field: FieldDefinition, options: { prune?: boolean } = {}): FieldProgram {
  const ir = lowerField(field),
    instructions = new Map(ir.instructions.map((instruction) => [instruction.id, instruction]));
  const metrics = { evaluatedNodes: 0, prunedNodes: 0 };
  if (field.nodes.length > 128) throw new Error("Field exceeds 128 nodes");
  const nodes = new Map(field.nodes.map((n) => [n.id, n]));
  if (nodes.size !== field.nodes.length) throw new Error("Duplicate field node identity");
  const active = new Set<string>();
  const cache = new Map<string, (p: Vec3) => FieldSample>();
  const costs = new Map<string, number>();
  const compile = (id: string, depth = 0): ((p: Vec3) => FieldSample) => {
    if (depth > 32) throw new Error(`Field graph exceeds 32 levels at ${id}`);
    const cached = cache.get(id);
    if (cached) return cached;
    const node = nodes.get(id);
    if (!node) throw new Error(`Missing field node ${id}`);
    if (active.has(id)) throw new Error(`Cycle in field at ${id}`);
    active.add(id);
    if (node.size.some((value) => value <= 0 || !Number.isFinite(value)))
      throw new Error(`Shape size must be positive at ${id}`);
    const children = node.children.map((child) => compile(child, depth + 1));
    active.delete(id);
    const cost = 1 + node.children.reduce((sum, child) => sum + (costs.get(child) ?? 0), 0);
    if (cost > 512) throw new Error(`Field evaluation exceeds 512 operations at ${id}`);
    costs.set(id, cost);
    const operation = ["union", "intersect", "subtract", "smoothUnion"].includes(node.kind);
    if (operation && children.length < 1) throw new Error(`${id} composition requires at least one child`);

    const [rx, ry, rz] = node.rotation;
    const cx = Math.cos(rx),
      sx = Math.sin(rx),
      cy = Math.cos(ry),
      sy = Math.sin(ry),
      cz = Math.cos(rz),
      sz = Math.sin(rz);
    const position = node.position;
    const size = node.size.map((value) => Math.max(0.0001, Math.abs(value)));
    const evaluate = (point: Vec3): FieldSample => {
      metrics.evaluatedNodes++;
      // Inverse Rz Ry Rx: first undo Z, then Y, then X.
      const x = point[0] - position[0],
        y = point[1] - position[1],
        z = point[2] - position[2];
      const xx = cz * x + sz * y,
        yy = -sz * x + cz * y;
      const xxx = cy * xx - sy * z,
        zz = sy * xx + cy * z;
      const p: Vec3 = [xxx, cx * yy + sx * zz, -sx * yy + cx * zz];
      if (!operation) return { distance: primitive(node, p, size), source: id, material: node.material };
      let result = children[0](p);
      for (let i = 1; i < children.length; i++) {
        const instruction = instructions.get(node.children[i]);
        if (
          options.prune !== false &&
          instruction &&
          (node.kind === "union" || node.kind === "smoothUnion")
        ) {
          const margin = node.kind === "smoothUnion" ? Math.max(0, node.blend) : 0;
          if (outsideDistanceBound(instruction, p) > result.distance + margin) {
            metrics.prunedNodes += instruction.evaluationCost;
            continue;
          }
        }
        const b = children[i](p);
        if (node.kind === "union") {
          if (b.distance < result.distance) result = b;
        } else if (node.kind === "intersect") {
          if (b.distance > result.distance) result = b;
        } else if (node.kind === "subtract") {
          if (-b.distance > result.distance) result = { ...b, distance: -b.distance };
        } else {
          const k = node.blend;
          if (k <= 0) {
            if (b.distance < result.distance) result = b;
            continue;
          }
          const h = clamp(0.5 + (0.5 * (b.distance - result.distance)) / k, 0, 1);
          const distance = b.distance * (1 - h) + result.distance * h - k * h * (1 - h);
          result = { ...(h >= 0.5 ? result : b), distance };
        }
      }
      return node.material ? { ...result, material: node.material } : result;
    };
    cache.set(id, evaluate);
    return evaluate;
  };
  const sample = compile(field.root);
  const distance = (p: Vec3) => sample(p).distance;
  return {
    sample,
    distance,
    classification: "implicit",
    ir,
    metrics,
    normal(p, e = 0.001) {
      return normalize([
        distance([p[0] + e, p[1], p[2]]) - distance([p[0] - e, p[1], p[2]]),
        distance([p[0], p[1] + e, p[2]]) - distance([p[0], p[1] - e, p[2]]),
        distance([p[0], p[1], p[2] + e]) - distance([p[0], p[1], p[2] - e]),
      ]);
    },
  };
}
function primitive(n: FieldNode, p: Vec3, s: number[]): number {
  const [x, y, z] = p;
  switch (n.kind) {
    case "sphere":
      return Math.hypot(x, y, z) - n.radius;
    case "ellipsoid": {
      const k0 = Math.hypot(x / s[0], y / s[1], z / s[2]);
      const k1 = Math.hypot(x / (s[0] * s[0]), y / (s[1] * s[1]), z / (s[2] * s[2]));
      return k1 > 1e-12 ? (k0 * (k0 - 1)) / k1 : -Math.min(...s);
    }
    case "rock": {
      // Intersected mineral cleavage planes form a bounded polyhedron. A small
      // continuous fracture term breaks straight silhouettes at close range;
      // the six axis planes keep it inside the authored half extents.
      const qx = x / s[0],
        qy = y / s[1],
        qz = z / s[2];
      let face = -Infinity;
      for (const [nx, ny, nz, limit] of ROCK_PLANES)
        face = Math.max(face, nx * qx + ny * qy + nz * qz - limit);
      const fracture =
        0.027 * Math.sin(qx * 9.7 + qz * 4.3) * Math.sin(qy * 11.2 - qz * 6.1) +
        0.012 * Math.sin(qx * 19.1 - qy * 7.3) * Math.sin(qz * 16.7 + qy * 8.1);
      return (face + fracture) * Math.min(...s);
    }
    case "box": {
      const q = [Math.abs(x) - s[0], Math.abs(y) - s[1], Math.abs(z) - s[2]];
      return Math.hypot(...q.map((v) => Math.max(v, 0))) + Math.min(Math.max(...q), 0);
    }
    case "capsule":
      return Math.hypot(x, y - clamp(y, -s[1], s[1]), z) - n.radius;
    case "torus":
      return Math.hypot(Math.hypot(x, z) - s[0], y) - n.radius;
    default:
      throw new Error(`Unknown primitive ${n.kind}`);
  }
}

const ROCK_PLANES: readonly (readonly [number, number, number, number])[] = [
  [1, 0, 0, 0.84],
  [-1, 0, 0, 0.94],
  [0, 1, 0, 0.82],
  [0, -1, 0, 0.9],
  [0, 0, 1, 0.85],
  [0, 0, -1, 0.92],
  [0.78, 0.63, 0, 0.91],
  [-0.77, 0.64, 0, 0.96],
  [0.69, -0.72, 0, 0.95],
  [-0.64, -0.77, 0, 0.94],
  [0.71, 0, 0.71, 0.91],
  [-0.72, 0, 0.69, 0.92],
  [0.62, 0, -0.78, 0.96],
  [-0.74, 0, -0.67, 0.91],
  [0, 0.68, 0.73, 0.97],
  [0, 0.75, -0.66, 0.91],
  [0, -0.71, 0.71, 0.94],
  [0, -0.66, -0.75, 0.94],
];
export function evaluateField(field: FieldDefinition, point: Vec3): number {
  return compileField(field).distance(point);
}
