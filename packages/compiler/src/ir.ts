import type { Bounds, FieldDefinition, FieldNode, Vec3 } from "@wrela/model";

/** Bounds enclose the negative region. boundDistance additionally promises a
 * lower bound on the implicit value outside that box; it is never inferred for
 * an arbitrary implicit function (in particular, ellipsoid estimates). */
export type FieldInstruction = {
  id: string;
  kind: FieldNode["kind"];
  children: number[];
  bounds: Bounds | null;
  boundDistance: boolean;
  evaluationCost: number;
  depth: number;
  minimumFeatureSize: number | null;
  source: FieldNode;
};
export type FieldIR = { root: number; instructions: FieldInstruction[]; evaluationCost: number };

function transformed(bounds: Bounds, node: FieldNode): Bounds {
  const [rx, ry, rz] = node.rotation;
  const cx = Math.cos(rx),
    sx = Math.sin(rx),
    cy = Math.cos(ry),
    sy = Math.sin(ry),
    cz = Math.cos(rz),
    sz = Math.sin(rz);
  const result: Bounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
  for (const x of [bounds.min[0], bounds.max[0]])
    for (const y of [bounds.min[1], bounds.max[1]])
      for (const z of [bounds.min[2], bounds.max[2]]) {
        const yy = cx * y - sx * z,
          zz = sx * y + cx * z;
        const xx = cy * x + sy * zz,
          zzz = -sy * x + cy * zz;
        const p = [cz * xx - sz * yy, sz * xx + cz * yy, zzz];
        for (let axis = 0; axis < 3; axis++) {
          result.min[axis] = Math.min(result.min[axis], p[axis] + node.position[axis]);
          result.max[axis] = Math.max(result.max[axis], p[axis] + node.position[axis]);
        }
      }
  return result;
}

export function lowerField(field: FieldDefinition): FieldIR {
  if (field.nodes.length > 128) throw new Error("Field exceeds 128 nodes");
  const sources = new Map(field.nodes.map((node) => [node.id, node]));
  if (sources.size !== field.nodes.length) throw new Error("Duplicate field node identity");
  const instructions: FieldInstruction[] = [],
    known = new Map<string, number>(),
    active = new Set<string>();
  function visit(id: string): number {
    if (active.has(id)) throw new Error(`Cycle in field at ${id}`);
    const previous = known.get(id);
    if (previous !== undefined) return previous;
    const node = sources.get(id);
    if (!node) throw new Error(`Missing field node ${id}`);
    if (node.size.some((value) => value <= 0 || !Number.isFinite(value)))
      throw new Error(`Shape size must be positive at ${id}`);
    active.add(id);
    const children = node.children.map(visit),
      child = children.map((index) => instructions[index]);
    active.delete(id);
    const evaluationCost = 1 + child.reduce((sum, instruction) => sum + instruction.evaluationCost, 0);
    const depth = child.length ? 1 + Math.max(...child.map((instruction) => instruction.depth)) : 0;
    if (evaluationCost > 512) throw new Error(`Field evaluation exceeds 512 operations at ${id}`);
    if (depth > 32) throw new Error(`Field graph exceeds 32 levels at ${id}`);
    const composition = ["union", "intersect", "subtract", "smoothUnion"].includes(node.kind);
    if (composition && !child.length) throw new Error(`${id} composition requires at least one child`);
    if (!composition && child.length) throw new Error(`Primitive ${id} cannot contain children`);
    let bounds: Bounds | null,
      boundDistance = node.kind !== "ellipsoid" && node.kind !== "rock",
      minimumFeatureSize: number | null = null;
    if (composition) {
      const selected = node.kind === "subtract" || node.kind === "intersect" ? [child[0]] : child;
      const boxes = selected.map((c) => c.bounds);
      bounds = boxes.every((box) => box !== null)
        ? {
            min: [0, 1, 2].map((axis) => Math.min(...boxes.map((b) => b.min[axis]))) as Vec3,
            max: [0, 1, 2].map((axis) => Math.max(...boxes.map((b) => b.max[axis]))) as Vec3,
          }
        : null;
      boundDistance = selected.every((c) => c.boundDistance);
      if (node.kind === "smoothUnion" && node.blend > 0) {
        // A blend lowers the minimum by at most k/4 per fold. Only children
        // with distance bounds admit this finite support expansion.
        const expansion = (node.blend * Math.max(0, child.length - 1)) / 4;
        if (boundDistance && bounds)
          for (let axis = 0; axis < 3; axis++) {
            bounds.min[axis] -= expansion;
            bounds.max[axis] += expansion;
          }
        else bounds = null;
        // Generic smooth implicit values do not promise Euclidean distance.
        boundDistance = false;
      }
    } else {
      let extent: Vec3;
      const size = node.size.map((value) => Math.max(0.0001, value)) as Vec3;
      switch (node.kind) {
        case "sphere":
          extent = [node.radius, node.radius, node.radius];
          minimumFeatureSize = 2 * node.radius;
          break;
        case "capsule":
          extent = [node.radius, size[1] + node.radius, node.radius];
          minimumFeatureSize = 2 * node.radius;
          break;
        case "torus":
          extent = [size[0] + node.radius, node.radius, size[0] + node.radius];
          minimumFeatureSize = 2 * node.radius;
          break;
        default:
          extent = [...size];
          minimumFeatureSize = 2 * Math.min(...size);
      }
      bounds = { min: extent.map((value) => -value) as Vec3, max: extent };
    }
    // Null bounds explicitly mean the compiler has not established support.
    const index = instructions.length;
    instructions.push({
      id,
      kind: node.kind,
      children,
      bounds: bounds ? transformed(bounds, node) : null,
      boundDistance,
      evaluationCost,
      depth,
      minimumFeatureSize,
      source: node,
    });
    known.set(id, index);
    return index;
  }
  const root = visit(field.root);
  return { root, instructions, evaluationCost: instructions[root].evaluationCost };
}

export function outsideDistanceBound(instruction: FieldInstruction, point: Vec3): number {
  if (!instruction.boundDistance || !instruction.bounds) return -Infinity;
  const b = instruction.bounds;
  const x = Math.max(b.min[0] - point[0], 0, point[0] - b.max[0]),
    y = Math.max(b.min[1] - point[1], 0, point[1] - b.max[1]),
    z = Math.max(b.min[2] - point[2], 0, point[2] - b.max[2]);
  return x || y || z ? Math.hypot(x, y, z) : -Infinity;
}
