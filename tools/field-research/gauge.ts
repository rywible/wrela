/** Zero-preserving approximation: approximate the positive metric multiplier,
 * never the primitive's known zero set. For Wrela's ellipsoid estimator,
 * f = Q w, Q = sum((p_i/s_i)^2)-1, w = q/(r(q+1)). */
import type { Bounds, Vec3 } from "@wrela/model";
import {
  add,
  apply,
  constant,
  type Expression,
  type Jet,
  mul,
  norm,
  quadratic,
  scale,
  smoothMin,
  unary,
} from "./local-program";

type GaugeExpression =
  | {
      kind: "leaf";
      source: Extract<Expression, { kind: "leaf" }>;
      center: Vec3;
      gauge: Jet;
      order: 0 | 1 | 2;
    }
  | { kind: "negate"; child: GaugeExpression }
  | { kind: "min" | "max" | "blend"; a: GaugeExpression; b: GaugeExpression; k: number };
const inverse = (a: Jet) => unary(a, 1 / a.v, -1 / a.v ** 2, 2 / a.v ** 3);
export function compileGauge(expression: Expression, bounds: Bounds, order: 0 | 1 | 2): GaugeExpression {
  const center = bounds.min.map((v, i) => (v + bounds.max[i]) / 2) as Vec3;
  if (expression.kind === "material") return compileGauge(expression.child, bounds, order);
  if (expression.kind === "negate")
    return { kind: "negate", child: compileGauge(expression.child, bounds, order) };
  if (expression.kind !== "leaf")
    return {
      ...expression,
      a: compileGauge(expression.a, bounds, order),
      b: compileGauge(expression.b, bounds, order),
    };
  const { node, transform } = expression;
  if (node.kind !== "ellipsoid" && node.kind !== "sphere")
    throw new Error("Gauge prototype supports ellipsoids and spheres");
  const p = apply(transform, center),
    s =
      node.kind === "sphere"
        ? [node.radius, node.radius, node.radius]
        : node.size.map((v) => Math.max(0.0001, Math.abs(v)));
  const xyz = p.map((v, i) => ({ ...constant(v), g: transform.m.slice(i * 3, i * 3 + 3) as Vec3 }));
  const q = norm(xyz.map((x, i) => scale(x, 1 / s[i])));
  if (q.v < 1e-10) throw new Error("Gauge requires a region away from the primitive center");
  const r = norm(xyz.map((x, i) => scale(x, 1 / s[i] ** 2)));
  const gauge =
    node.kind === "sphere"
      ? scale(inverse(add(q, constant(1))), node.radius)
      : mul(q, inverse(mul(r, add(q, constant(1)))));
  const radius = bounds.min.map((v, i) => (bounds.max[i] - v) / 2);
  const variation =
    (order >= 1 ? gauge.g.reduce((sum, v, i) => sum + Math.abs(v) * radius[i], 0) : 0) +
    (order === 2
      ? gauge.h.reduce((sum, v, i) => sum + 0.5 * Math.abs(v) * radius[Math.floor(i / 3)] * radius[i % 3], 0)
      : 0);
  if (gauge.v <= variation) throw new Error("Gauge polynomial not proven positive throughout the region");
  return { kind: "leaf", source: expression, center, gauge, order };
}
export function gaugeValue(expression: GaugeExpression, p: Vec3): number {
  if (expression.kind === "negate") return -gaugeValue(expression.child, p);
  if (expression.kind !== "leaf") {
    const a = gaugeValue(expression.a, p),
      b = gaugeValue(expression.b, p);
    return expression.kind === "min"
      ? Math.min(a, b)
      : expression.kind === "max"
        ? Math.max(a, b)
        : smoothMin(a, b, expression.k);
  }
  const { node, transform } = expression.source;
  const local = apply(transform, p),
    s =
      node.kind === "sphere"
        ? [node.radius, node.radius, node.radius]
        : node.size.map((v) => Math.max(0.0001, Math.abs(v)));
  const q = local.reduce((sum, v, i) => sum + (v / s[i]) ** 2, -1);
  const delta = p.map((v, i) => v - expression.center[i]) as Vec3;
  const w =
    expression.order === 2
      ? quadratic(expression.gauge, delta)
      : expression.gauge.v +
        (expression.order === 1 ? expression.gauge.g.reduce((sum, v, i) => sum + v * delta[i], 0) : 0);
  return q * w;
}
