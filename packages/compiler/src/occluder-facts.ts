import type { Bounds, FieldDefinition, FieldNode, OpaqueVisibilityFacts, Vec3 } from "@wrela/model";

import { lowerField } from "./ir";

type Sign = "inside" | "outside" | "unknown";
const subtract = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

/** Conservative sign facts over a box, retaining the complete ordered CSG.
 * Unsupported rotations/primitives and active smooth-union exterior queries
 * deliberately return unknown. This is a real-arithmetic source certificate,
 * never permission to cull the separately extracted polygonal approximation. */
export function opaqueFieldFacts(field: FieldDefinition, sourceKey: string): OpaqueVisibilityFacts {
  const ir = lowerField(field);
  const nodes = new Map(field.nodes.map((node) => [node.id, node]));
  function classify(id: string, box: Bounds): Sign {
    const node = nodes.get(id);
    if (!node || node.rotation.some((v) => v !== 0)) return "unknown";
    if (![...node.position, ...node.size, node.radius, node.blend].every(Number.isFinite)) return "unknown";
    const local = { min: subtract(box.min, node.position), max: subtract(box.max, node.position) };
    if (node.children.length) {
      let result = classify(node.children[0], local);
      for (const child of node.children.slice(1)) {
        const b = classify(child, local);
        switch (node.kind) {
          case "union":
          case "smoothUnion":
            result =
              result === "inside" || b === "inside"
                ? "inside"
                : result === "outside" && b === "outside" && (node.kind === "union" || node.blend <= 0)
                  ? "outside"
                  : "unknown";
            break;
          case "intersect":
            result =
              result === "outside" || b === "outside"
                ? "outside"
                : result === "inside" && b === "inside"
                  ? "inside"
                  : "unknown";
            break;
          case "subtract":
            result =
              result === "outside" || b === "inside"
                ? "outside"
                : result === "inside" && b === "outside"
                  ? "inside"
                  : "unknown";
            break;
          default:
            return "unknown";
        }
      }
      return result;
    }
    const minimum = [0, 1, 2].map((axis) => Math.max(local.min[axis], -local.max[axis], 0));
    const maximum = [0, 1, 2].map((axis) => Math.max(Math.abs(local.min[axis]), Math.abs(local.max[axis])));
    if (node.kind === "box") {
      if (maximum.every((v, axis) => v < node.size[axis])) return "inside";
      if (minimum.some((v, axis) => v > node.size[axis])) return "outside";
    }
    if (node.kind === "sphere" || node.kind === "ellipsoid") {
      const size = node.kind === "sphere" ? [node.radius, node.radius, node.radius] : node.size;
      if (size.some((v) => v <= 0)) return "unknown";
      const squared = (values: number[]) =>
        values.reduce((sum, value, axis) => sum + (value / size[axis]) ** 2, 0);
      // A deliberate strict margin rejects ties; numericError remains unknown.
      if (squared(maximum) < 1 - 1e-10) return "inside";
      if (squared(minimum) > 1 + 1e-10) return "outside";
    }
    return "unknown";
  }
  const candidates: { center: Vec3; radius: number }[] = [];
  function visit(node: FieldNode, parent: Vec3) {
    if (candidates.length >= 32 || node.rotation.some((v) => v !== 0)) return;
    const center = node.position.map((v, axis) => v + parent[axis]) as Vec3;
    if (node.children.length) {
      for (const child of node.children) {
        const source = nodes.get(child);
        if (source) visit(source, center);
      }
    } else if (["sphere", "ellipsoid", "box"].includes(node.kind)) {
      const maximum = node.kind === "sphere" ? node.radius : Math.min(...node.size);
      if (![...center, maximum].every(Number.isFinite) || maximum <= 0) return;
      let lo = 0,
        hi = maximum;
      for (let i = 0; i < 32; i++) {
        const radius = (lo + hi) / 2;
        const box: Bounds = {
          min: center.map((value) => value - radius) as Vec3,
          max: center.map((value) => value + radius) as Vec3,
        };
        if (classify(field.root, box) === "inside") lo = radius;
        else hi = radius;
      }
      if (lo > 1e-8) candidates.push({ center, radius: lo });
    }
  }
  const root = nodes.get(field.root);
  if (root) visit(root, [0, 0, 0]);
  return {
    sourceKey,
    interior: candidates,
    exterior: ir.instructions[ir.root].bounds,
    evidence: "real-bound",
    numericError: "unknown",
    rigid: true,
    opaque: true,
  };
}
