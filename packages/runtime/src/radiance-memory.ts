import type { RadianceLightingProduct } from "@wrela/compiler";

/** Only resources included in report.bytes participate in deduplication.
 * Equal content is insufficient: sharing requires the same immutable object. */
function sharedResources(product: RadianceLightingProduct): [object, number][] {
  const resources: [object, number][] = [[product.geometry, product.geometry.report.bytes]];
  for (const receiver of product.receivers.values()) {
    if (!receiver.sources) continue;
    for (const view of [
      receiver.sources,
      receiver.weights,
      receiver.mesh.positions,
      receiver.mesh.normals,
      receiver.mesh.indices,
      receiver.mesh.colors,
      receiver.mesh.materialCoordinates,
    ])
      if (view) resources.push([view, view.byteLength]);
  }
  for (const patch of product.field.surfaceDiffuse?.patches ?? []) {
    resources.push(
      [patch.validCells, patch.validCells.byteLength],
      [patch.directEmission, patch.directEmission.byteLength],
    );
  }
  return resources;
}

export function radianceRetainedBytes(products: Iterable<RadianceLightingProduct>): number {
  const seen = new Set<object>();
  let bytes = 0;
  for (const product of new Set(products)) {
    bytes += product.field.report.bytes;
    for (const [resource, size] of sharedResources(product)) {
      if (seen.has(resource)) bytes -= size;
      else seen.add(resource);
    }
  }
  return bytes;
}

/** Scheduling estimate, not a peak heap guarantee. Actual completed products
 * are admitted/evicted by their measured unique retained size. */
export function radianceNeighborEstimate(product: RadianceLightingProduct): number {
  const seen = new Set<object>();
  let shared = 0;
  for (const [resource, size] of sharedResources(product)) {
    if (seen.has(resource)) continue;
    seen.add(resource);
    shared += size;
  }
  return Math.max(0, product.field.report.bytes - shared);
}
