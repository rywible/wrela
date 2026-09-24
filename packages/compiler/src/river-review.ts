import type { WaterAuthoring } from "@wrela/model";

import { riverSections } from "./river-channel";

type River = NonNullable<WaterAuthoring["river"]>;
type Pair = [number, number];
export type RiverDiagnostic = {
  code: "folded-bank" | "crossing-channel" | "tight-bend" | "wide-shore";
  severity: "error" | "warning";
  points: number[];
  message: string;
};
const cross = (a: Pair, b: Pair, c: Pair) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
function overlaps(a: Pair[], b: Pair[]): boolean {
  for (const polygon of [a, b]) {
    for (let index = 0; index < polygon.length; index++) {
      const p = polygon[index],
        q = polygon[(index + 1) % polygon.length];
      const axis: Pair = [p[1] - q[1], q[0] - p[0]];
      const project = (points: Pair[]) => points.map((point) => point[0] * axis[0] + point[1] * axis[1]);
      const pa = project(a),
        pb = project(b);
      if (Math.min(...pa) >= Math.max(...pb) - 1e-8 || Math.min(...pb) >= Math.max(...pa) - 1e-8)
        return false;
    }
  }
  return true;
}
/** Review uses the exact realized miter sections so an accepted ribbon has upward faces. */
export function reviewRiverChannel(river: River): RiverDiagnostic[] {
  const diagnostics: RiverDiagnostic[] = [];
  const sections = riverSections(river);
  for (let index = 1; index < sections.length; index++) {
    const a = sections[index - 1],
      b = sections[index];
    // The grid has bilinear sections. Positive corner Jacobians establish its
    // orientation everywhere, not only at the two coarse triangle centroids.
    const corners = [
      cross(a.left, a.right, b.left),
      cross(a.left, a.right, b.right),
      cross(b.left, b.right, a.left),
      cross(b.left, b.right, a.right),
    ];
    if (corners[0] >= -1e-8 || corners[1] >= -1e-8 || corners[2] <= 1e-8 || corners[3] <= 1e-8)
      diagnostics.push({
        code: "folded-bank",
        severity: "error",
        points: [index - 1, index],
        message: `River section ${index} folds or collapses its banks. Increase point spacing, reduce width, or soften the bend.`,
      });
    for (let other = index + 2; other < sections.length; other++) {
      const c = sections[other - 1],
        d = sections[other];
      if (overlaps([a.left, a.right, b.right, b.left], [c.left, c.right, d.right, d.left]))
        diagnostics.push({
          code: "crossing-channel",
          severity: "error",
          points: [index - 1, index, other - 1, other],
          message: `River sections ${index} and ${other} overlap. Move the centerline or narrow their banks.`,
        });
    }
  }
  for (let index = 1; index < river.points.length - 1; index++) {
    const a = river.points[index - 1].position,
      b = river.points[index].position,
      c = river.points[index + 1].position;
    const before: Pair = [b[0] - a[0], b[1] - a[1]],
      after: Pair = [c[0] - b[0], c[1] - b[1]];
    const cosine =
      (before[0] * after[0] + before[1] * after[1]) / (Math.hypot(...before) * Math.hypot(...after));
    if (cosine < 0.2)
      diagnostics.push({
        code: "tight-bend",
        severity: "warning",
        points: [index],
        message: `River point ${index + 1} has a sharp bend. Add more points around the bend for a natural channel.`,
      });
  }
  if (river.points.some((point) => river.shoreWidth >= point.width / 2))
    diagnostics.push({
      code: "wide-shore",
      severity: "warning",
      points: [],
      message:
        "Shore response spans the whole width of at least one section; flow never reaches full speed there.",
    });
  return diagnostics;
}
