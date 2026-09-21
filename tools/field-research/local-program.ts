/** Research prototype. Not installed in the production compiler or renderer.
 * Spatial reduction preserves Wrela's ordered CSG, values, and provenance.
 * Certificates are conservative in real arithmetic; the numerical padding here
 * is NOT a replacement for a directed-rounding interval implementation. */
import { compileField, type FieldSample } from "@wrela/compiler";
import type { Bounds, FieldDefinition, FieldNode, Vec3 } from "@wrela/model";

export type Matrix = [number, number, number, number, number, number, number, number, number];
type Transform = { m: Matrix; b: Vec3 };
type Leaf = { kind: "leaf"; node: FieldNode; transform: Transform };
export type Expression =
  | Leaf
  | { kind: "material"; material: string; child: Expression }
  | { kind: "negate"; child: Expression }
  | { kind: "min" | "max" | "blend"; a: Expression; b: Expression; k: number };
export type Range = { lo: number; hi: number };
export type LocalProgram = {
  expression: Expression;
  range: Range;
  primitives: number;
  smooth: boolean;
  canonical: Leaf | undefined;
};
const identity: Matrix = [1, 0, 0, 0, 1, 0, 0, 0, 1];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const matvec = (m: Matrix, p: Vec3): Vec3 => [
  m[0] * p[0] + m[1] * p[1] + m[2] * p[2],
  m[3] * p[0] + m[4] * p[1] + m[5] * p[2],
  m[6] * p[0] + m[7] * p[1] + m[8] * p[2],
];
export const apply = (t: Transform, p: Vec3): Vec3 => {
  const q = matvec(t.m, p);
  return [q[0] + t.b[0], q[1] + t.b[1], q[2] + t.b[2]];
};
function compose(parent: Transform, node: FieldNode): Transform {
  const [x, y, z] = node.rotation;
  const cx = Math.cos(x),
    sx = Math.sin(x),
    cy = Math.cos(y),
    sy = Math.sin(y),
    cz = Math.cos(z),
    sz = Math.sin(z);
  const r: Matrix = [
    cy * cz,
    cy * sz,
    -sy,
    sx * sy * cz - cx * sz,
    sx * sy * sz + cx * cz,
    sx * cy,
    cx * sy * cz + sx * sz,
    cx * sy * sz - sx * cz,
    cx * cy,
  ];
  const m = Array<number>(9).fill(0) as Matrix;
  for (let i = 0; i < 3; i++)
    for (let j = 0; j < 3; j++)
      for (let k = 0; k < 3; k++) m[i * 3 + j] += r[i * 3 + k] * parent.m[k * 3 + j];
  return { m, b: matvec(r, parent.b.map((v, i) => v - node.position[i]) as Vec3) };
}

export function lower(field: FieldDefinition): Expression {
  compileField(field); // Use the production graph validation, including cost/depth limits.
  const nodes = new Map(field.nodes.map((node) => [node.id, node]));
  function visit(id: string, parent: Transform): Expression {
    const node = nodes.get(id);
    if (!node) throw new Error(`Missing node ${id}`);
    const transform = compose(parent, node);
    if (!["union", "intersect", "subtract", "smoothUnion"].includes(node.kind))
      return { kind: "leaf", node, transform };
    const children = node.children.map((child) => visit(child, transform));
    let result = children[0];
    for (let i = 1; i < children.length; i++) {
      const kind =
        node.kind === "intersect" || node.kind === "subtract"
          ? "max"
          : node.kind === "smoothUnion" && node.blend > 0
            ? "blend"
            : "min";
      result = {
        kind,
        a: result,
        b: node.kind === "subtract" ? { kind: "negate", child: children[i] } : children[i],
        k: node.blend,
      };
    }
    return node.material ? { kind: "material", material: node.material, child: result } : result;
  }
  return visit(field.root, { m: identity, b: [0, 0, 0] });
}

function primitive(leaf: Leaf, point: Vec3): number {
  const [x, y, z] = apply(leaf.transform, point);
  const n = leaf.node;
  const s = n.size.map((v) => Math.max(0.0001, Math.abs(v)));
  if (n.kind === "sphere") return Math.hypot(x, y, z) - n.radius;
  if (n.kind === "ellipsoid") {
    const q = Math.hypot(x / s[0], y / s[1], z / s[2]);
    const r = Math.hypot(x / s[0] ** 2, y / s[1] ** 2, z / s[2] ** 2);
    return r > 1e-12 ? (q * (q - 1)) / r : -Math.min(...s);
  }
  if (n.kind === "capsule") return Math.hypot(x, y - Math.max(-s[1], Math.min(s[1], y)), z) - n.radius;
  if (n.kind === "torus") return Math.hypot(Math.hypot(x, z) - s[0], y) - n.radius;
  const q = [Math.abs(x) - s[0], Math.abs(y) - s[1], Math.abs(z) - s[2]];
  return Math.hypot(...q.map((v) => Math.max(0, v))) + Math.min(Math.max(...q), 0);
}
export function smoothMin(a: number, b: number, k: number): number {
  const h = Math.max(0, Math.min(1, 0.5 + (0.5 * (b - a)) / k));
  return b * (1 - h) + a * h - k * h * (1 - h);
}
export function value(expression: Expression, p: Vec3): number {
  if (expression.kind === "leaf") return primitive(expression, p);
  if (expression.kind === "material") return value(expression.child, p);
  if (expression.kind === "negate") return -value(expression.child, p);
  const a = value(expression.a, p),
    b = value(expression.b, p);
  return expression.kind === "min"
    ? Math.min(a, b)
    : expression.kind === "max"
      ? Math.max(a, b)
      : smoothMin(a, b, expression.k);
}
export function sample(expression: Expression, p: Vec3): FieldSample {
  if (expression.kind === "leaf")
    return {
      distance: primitive(expression, p),
      source: expression.node.id,
      material: expression.node.material,
    };
  if (expression.kind === "material")
    return { ...sample(expression.child, p), material: expression.material };
  if (expression.kind === "negate") {
    const a = sample(expression.child, p);
    return { ...a, distance: -a.distance };
  }
  const a = sample(expression.a, p),
    b = sample(expression.b, p);
  if (expression.kind === "min") return b.distance < a.distance ? b : a;
  if (expression.kind === "max") return b.distance > a.distance ? b : a;
  return { ...(a.distance <= b.distance ? a : b), distance: smoothMin(a.distance, b.distance, expression.k) };
}
function padded(lo: number, hi: number): Range {
  const e = 1e-11 * (1 + Math.max(Math.abs(lo), Math.abs(hi)));
  return { lo: lo - e, hi: hi + e };
}
function localBox(leaf: Leaf, bounds: Bounds) {
  const center = bounds.min.map((v, i) => (v + bounds.max[i]) / 2) as Vec3;
  const radius = bounds.min.map((v, i) => (bounds.max[i] - v) / 2) as Vec3;
  const m = leaf.transform.m;
  return {
    center: apply(leaf.transform, center),
    radius: [0, 1, 2].map(
      (i) =>
        Math.abs(m[i * 3]) * radius[0] +
        Math.abs(m[i * 3 + 1]) * radius[1] +
        Math.abs(m[i * 3 + 2]) * radius[2],
    ) as Vec3,
    worldCenter: center,
    worldRadius: Math.hypot(...radius),
  };
}
function leafRange(leaf: Leaf, bounds: Bounds): { range: Range; smooth: boolean } {
  const box = localBox(leaf, bounds),
    n = leaf.node;
  if (n.kind !== "ellipsoid") {
    const d = primitive(leaf, box.worldCenter);
    // These primitives have exact unit-Lipschitz distances under rigid transforms.
    return {
      range: padded(d - box.worldRadius, d + box.worldRadius),
      smooth: n.kind === "sphere" && Math.hypot(...box.center) > box.worldRadius,
    };
  }
  const s = n.size.map((v) => Math.max(0.0001, Math.abs(v)));
  const near = box.center.map((v, i) => Math.max(0, Math.abs(v) - box.radius[i]));
  const far = box.center.map((v, i) => Math.abs(v) + box.radius[i]);
  const qlo = Math.hypot(...near.map((v, i) => v / s[i]));
  const qhi = Math.hypot(...far.map((v, i) => v / s[i]));
  const rlo = Math.hypot(...near.map((v, i) => v / s[i] ** 2));
  const rhi = Math.hypot(...far.map((v, i) => v / s[i] ** 2));
  // d = (q - 1) * (q / r). Homogeneity bounds q/r by the axis lengths,
  // including boxes that contain the singular center of the current estimator.
  const a = Math.max(Math.min(...s), rhi > 0 ? qlo / rhi : 0);
  const b = Math.min(Math.max(...s), rlo > 0 ? qhi / rlo : Infinity);
  const products = [(qlo - 1) * a, (qlo - 1) * b, (qhi - 1) * a, (qhi - 1) * b];
  if (rlo <= 1e-12) products.push(-Math.min(...s));
  return { range: padded(Math.min(...products), Math.max(...products)), smooth: rlo > 1e-12 };
}
export function specialize(expression: Expression, bounds: Bounds): LocalProgram {
  if (expression.kind === "leaf") {
    const { range, smooth } = leafRange(expression, bounds);
    return {
      expression,
      range,
      smooth,
      primitives: 1,
      canonical: ["ellipsoid", "sphere"].includes(expression.node.kind) ? expression : undefined,
    };
  }
  if (expression.kind === "material" || expression.kind === "negate") {
    const child = specialize(expression.child, bounds);
    return {
      ...child,
      expression: { ...expression, child: child.expression },
      range: expression.kind === "negate" ? { lo: -child.range.hi, hi: -child.range.lo } : child.range,
      canonical: expression.kind === "negate" ? undefined : child.canonical,
    };
  }
  const a = specialize(expression.a, bounds),
    b = specialize(expression.b, bounds),
    k = expression.k;
  if (expression.kind === "min" || expression.kind === "blend") {
    const gap = expression.kind === "blend" ? k : 0;
    if (a.range.hi + gap < b.range.lo) return a;
    if (b.range.hi + gap < a.range.lo) return b;
  } else {
    if (a.range.lo > b.range.hi) return a;
    if (b.range.lo > a.range.hi) return b;
  }
  const op =
    expression.kind === "min"
      ? Math.min
      : expression.kind === "max"
        ? Math.max
        : (x: number, y: number) => smoothMin(x, y, k);
  return {
    expression: { ...expression, a: a.expression, b: b.expression },
    range: padded(op(a.range.lo, b.range.lo), op(a.range.hi, b.range.hi)),
    primitives: a.primitives + b.primitives,
    smooth:
      expression.kind === "blend" &&
      a.smooth &&
      b.smooth &&
      a.range.hi - b.range.lo < k &&
      b.range.hi - a.range.lo < k,
    canonical: undefined,
  };
}

type Cell = { center: Vec3; program: LocalProgram; children?: Cell[] };
export function buildAtlas(field: FieldDefinition, depth = 5) {
  const expression = lower(field);
  let cells = 0,
    leaves = 0,
    boundaryLeaves = 0,
    canonicalBoundaryLeaves = 0,
    boundaryPrimitives = 0;
  function build(bounds: Bounds, expr: Expression, level: number): Cell {
    const program = specialize(expr, bounds);
    const center = bounds.min.map((v, i) => (v + bounds.max[i]) / 2) as Vec3;
    const cell: Cell = { center, program };
    cells++;
    const boundary = program.range.lo <= 0 && program.range.hi >= 0;
    if (level < depth && program.primitives > 1 && boundary) {
      cell.children = Array.from({ length: 8 }, (_, octant) =>
        build(
          {
            min: bounds.min.map((v, i) => (octant & (1 << i) ? center[i] : v)) as Vec3,
            max: bounds.max.map((v, i) => (octant & (1 << i) ? v : center[i])) as Vec3,
          },
          program.expression,
          level + 1,
        ),
      );
    } else {
      leaves++;
      if (boundary) {
        boundaryLeaves++;
        boundaryPrimitives += program.primitives;
        if (program.canonical) canonicalBoundaryLeaves++;
      }
    }
    return cell;
  }
  const root = build(field.bounds, expression, 0);
  function at(p: Vec3): LocalProgram {
    if (p.some((v, i) => v < field.bounds.min[i] || v > field.bounds.max[i]))
      throw new RangeError("Query outside atlas bounds");
    let c = root;
    while (c.children)
      c =
        c.children[
          (p[0] >= c.center[0] ? 1 : 0) | (p[1] >= c.center[1] ? 2 : 0) | (p[2] >= c.center[2] ? 4 : 0)
        ];
    return c.program;
  }
  return {
    expression,
    at,
    stats: { cells, leaves, boundaryLeaves, canonicalBoundaryLeaves, boundaryPrimitives },
  };
}

/** Second-order automatic differentiation. Only smooth sphere/ellipsoid regions
 * are eligible; hard CSG choices must be eliminated by region specialization. */
export type Jet = { v: number; g: Vec3; h: Matrix };
export const constant = (v: number): Jet => ({ v, g: [0, 0, 0], h: Array<number>(9).fill(0) as Matrix });
export const add = (a: Jet, b: Jet): Jet => ({
  v: a.v + b.v,
  g: a.g.map((v, i) => v + b.g[i]) as Vec3,
  h: a.h.map((v, i) => v + b.h[i]) as Matrix,
});
export const scale = (a: Jet, s: number): Jet => ({
  v: a.v * s,
  g: a.g.map((v) => v * s) as Vec3,
  h: a.h.map((v) => v * s) as Matrix,
});
export function mul(a: Jet, b: Jet): Jet {
  return {
    v: a.v * b.v,
    g: a.g.map((v, i) => v * b.v + b.g[i] * a.v) as Vec3,
    h: a.h.map(
      (v, i) =>
        v * b.v + b.h[i] * a.v + a.g[Math.floor(i / 3)] * b.g[i % 3] + b.g[Math.floor(i / 3)] * a.g[i % 3],
    ) as Matrix,
  };
}
export function unary(a: Jet, v: number, first: number, second: number): Jet {
  return {
    v,
    g: a.g.map((x) => first * x) as Vec3,
    h: a.h.map((x, i) => first * x + second * a.g[Math.floor(i / 3)] * a.g[i % 3]) as Matrix,
  };
}
export const norm = (v: Jet[]) => {
  const a = v.map((x) => mul(x, x)).reduce(add);
  const r = Math.sqrt(a.v);
  return unary(a, r, 0.5 / r, -0.25 / r ** 3);
};
export function jet(expression: Expression, p: Vec3): Jet {
  if (expression.kind === "material") return jet(expression.child, p);
  if (expression.kind === "negate") return scale(jet(expression.child, p), -1);
  if (expression.kind === "leaf") {
    const { node, transform } = expression;
    const local = apply(transform, p);
    const xyz = local.map((v, i) => ({ ...constant(v), g: transform.m.slice(i * 3, i * 3 + 3) as Vec3 }));
    if (node.kind === "sphere") return add(norm(xyz), constant(-node.radius));
    if (node.kind !== "ellipsoid") throw new Error(`Jets not implemented for ${node.kind}`);
    const s = node.size.map((v) => Math.max(0.0001, Math.abs(v)));
    const q = norm(xyz.map((x, i) => scale(x, 1 / s[i])));
    const r = norm(xyz.map((x, i) => scale(x, 1 / s[i] ** 2)));
    if (r.v <= 1e-12) throw new Error("No smooth ellipsoid jet at its center");
    return mul(mul(q, add(q, constant(-1))), unary(r, 1 / r.v, -1 / r.v ** 2, 2 / r.v ** 3));
  }
  const a = jet(expression.a, p),
    b = jet(expression.b, p);
  if (expression.kind === "min") return a.v <= b.v ? a : b;
  if (expression.kind === "max") return a.v >= b.v ? a : b;
  const h = Math.max(0, Math.min(1, 0.5 + (b.v - a.v) / (2 * expression.k)));
  if (h === 0) return b;
  if (h === 1) return a;
  // H(smin) = h Ha + (1-h) Hb - (ga-gb)(ga-gb)^T / (2k).
  const result = add(scale(a, h), scale(b, 1 - h));
  result.v = smoothMin(a.v, b.v, expression.k);
  for (let i = 0; i < 9; i++)
    result.h[i] -=
      ((a.g[Math.floor(i / 3)] - b.g[Math.floor(i / 3)]) * (a.g[i % 3] - b.g[i % 3])) / (2 * expression.k);
  return result;
}
export function quadratic(j: Jet, delta: Vec3): number {
  return j.v + dot(j.g, delta) + 0.5 * dot(delta, matvec(j.h, delta));
}
export function correct(j: Jet, center: Vec3, p: Vec3, n: Vec3): Vec3 | undefined {
  const delta = p.map((v, i) => v - center[i]) as Vec3;
  const a = 0.5 * dot(n, matvec(j.h, n));
  const b = dot(j.g, n) + dot(n, matvec(j.h, delta));
  const c = quadratic(j, delta),
    discriminant = b * b - 4 * a * c;
  if (discriminant < 0 || Math.abs(b) < 1e-10) return undefined;
  const t = Math.abs(a) < 1e-14 ? -c / b : (-2 * c) / (b + Math.sign(b) * Math.sqrt(discriminant));
  if (!Number.isFinite(t)) return undefined;
  return p.map((v, i) => v + t * n[i]) as Vec3;
}

/** Canonical zero-set lowering is legal only AFTER all blends and hard choices
 * have been eliminated on the query region. Ellipsoid distance values are not
 * interchangeable with this polynomial inside a smooth union. */
export function quadricHit(program: LocalProgram, origin: Vec3, direction: Vec3): number[] {
  const leaf = program.canonical;
  if (!leaf) throw new Error("Region is not a single positive quadric");
  const o = apply(leaf.transform, origin),
    d = matvec(leaf.transform.m, direction);
  const s =
    leaf.node.kind === "sphere"
      ? [leaf.node.radius, leaf.node.radius, leaf.node.radius]
      : leaf.node.size.map((v) => Math.max(0.0001, Math.abs(v)));
  const u = o.map((v, i) => v / s[i]) as Vec3,
    v = d.map((x, i) => x / s[i]) as Vec3;
  const a = dot(v, v),
    b = 2 * dot(u, v),
    c = dot(u, u) - 1;
  const discriminant = b * b - 4 * a * c;
  if (discriminant < 0 || a === 0) return [];
  const q = -0.5 * (b + (b >= 0 ? 1 : -1) * Math.sqrt(discriminant));
  return q === 0 ? [-b / (2 * a)] : [q / a, c / q].sort((a, b) => a - b);
}
