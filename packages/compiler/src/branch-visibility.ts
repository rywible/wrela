import { type BranchPlate, type BranchVisibilityProduct, contentKey, type Vec3 } from "@wrela/model";

export function branchGeometryKey(plates: readonly BranchPlate[]) {
  return contentKey(plates);
}
/** If a ray rises by at least minimumRise per horizontal metre, two AABBs
 * farther apart horizontally than their possible height separation cannot shadow
 * one another. This is independent of receiver position within its plate. */
export function compileBranchVisibility(
  plates: readonly BranchPlate[],
  minimumRise = 0.4,
): BranchVisibilityProduct {
  if (!Number.isFinite(minimumRise) || minimumRise <= 0 || plates.length > 4096)
    throw Error("Invalid branch visibility domain");
  const boxes = plates.map((plate) => {
    const { center, tangent, bitangent, halfLength, halfWidth } = plate;
    if (
      ![...center, ...tangent, ...bitangent, halfLength, halfWidth].every(Number.isFinite) ||
      halfLength <= 0 ||
      halfWidth <= 0
    )
      throw Error("Invalid branch plate");
    const extent = tangent.map((v, i) => Math.abs(v) * halfLength + Math.abs(bitangent[i]) * halfWidth);
    return { min: center.map((v, i) => v - extent[i]), max: center.map((v, i) => v + extent[i]) };
  });
  const offsets = [0],
    candidates: number[] = [];
  for (let receiver = 0; receiver < boxes.length; receiver++) {
    const a = boxes[receiver];
    for (let blocker = 0; blocker < boxes.length; blocker++) {
      if (receiver === blocker) continue;
      const b = boxes[blocker];
      const height = b.max[1] - a.min[1];
      const dx = Math.max(0, a.min[0] - b.max[0], b.min[0] - a.max[0]);
      const dz = Math.max(0, a.min[2] - b.max[2], b.min[2] - a.max[2]);
      // Keep a conservative margin rather than trusting floating-point ties.
      if (height >= -1e-5 && Math.hypot(dx, dz) * minimumRise <= height + 1e-5) candidates.push(blocker);
    }
    offsets.push(candidates.length);
  }
  return {
    sourceKey: branchGeometryKey(plates),
    algorithmVersion: 1,
    minimumRise,
    offsets: new Uint32Array(offsets),
    candidates: new Uint32Array(candidates),
    byteLength: 4 * (offsets.length + candidates.length),
  };
}
export function branchVisibilityApplies(
  product: BranchVisibilityProduct,
  sourceKey: string,
  localDirection: Vec3,
): boolean {
  return (
    product.algorithmVersion === 1 &&
    product.sourceKey === sourceKey &&
    localDirection.every(Number.isFinite) &&
    Math.hypot(...localDirection) > 0 &&
    localDirection[1] >= product.minimumRise * Math.hypot(localDirection[0], localDirection[2]) + 1e-6
  );
}
