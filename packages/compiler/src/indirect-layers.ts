import type { Vec3 } from "@wrela/model";
import type { IndirectGeometry } from "./indirect-query";

/** A small moving BVH does not rebuild or copy the large static BVH. Only the
 * triangle-reference table is joined, preserving global hit IDs for transport.
 * This transient query product is never a persistent static-lighting artifact. */
export function indirectGeometryLayers(
  base: IndirectGeometry,
  additions: readonly IndirectGeometry[],
): IndirectGeometry {
  if (!additions.length) return base;
  const triangles = [...base.triangles],
    layers = [...(base.layers ?? [])];
  let nodes = base.report.nodes,
    bytes = base.report.bytes;
  const min = [...base.bounds.min] as Vec3,
    max = [...base.bounds.max] as Vec3;
  for (const geometry of additions) {
    layers.push({ geometry, offset: triangles.length });
    // Avoid argument-count limits on large layers.
    for (const triangle of geometry.triangles) triangles.push(triangle);
    nodes += geometry.report.nodes;
    bytes += geometry.report.bytes;
    for (let a = 0; a < 3; a++) {
      min[a] = Math.min(min[a], geometry.bounds.min[a]);
      max[a] = Math.max(max[a], geometry.bounds.max[a]);
    }
  }
  return {
    ...base,
    staticGeometry: base,
    triangles,
    layers,
    bounds: { min, max },
    report: { ...base.report, triangles: triangles.length, nodes, bytes: bytes + triangles.length * 8 },
  };
}
