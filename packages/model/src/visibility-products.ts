import type { Bounds, Vec3 } from "./math";

/** Source-field facts only: these do not establish containment in an extracted
 * or deformed mesh. A realization and numeric evaluation proof is also needed
 * before an interior may remove potentially visible work. */
export type OpaqueVisibilityFacts = {
  sourceKey: string;
  interior: { center: Vec3; radius: number }[];
  exterior: Bounds | null;
  evidence: "real-bound";
  numericError: "unknown";
  rigid: true;
  opaque: true;
};
