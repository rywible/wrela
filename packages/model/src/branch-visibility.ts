import type { Vec3 } from "./math";
export type BranchPlate = {
  center: Vec3;
  tangent: Vec3;
  bitangent: Vec3;
  halfLength: number;
  halfWidth: number;
};
/** Rest-space candidate occluders for every point on each complete plate.
 * The consumer must inverse-transform rays for affine deformation. */
export type BranchVisibilityProduct = {
  sourceKey: string;
  algorithmVersion: 1;
  minimumRise: number;
  offsets: Uint32Array;
  candidates: Uint32Array;
  byteLength: number;
};
