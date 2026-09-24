import type { Bounds, Vec3 } from "./math";

/** Light-independent projected coverage and orientation of one complete crown.
 * A single view owns the crown in each pass; camera and light choose separately. */
export type VegetationCrownProduct = {
  version: 1;
  kind: "multiview-crown";
  key: string;
  sourceKey: string;
  algorithmVersion: string;
  fallback: "source-mesh";
  byteLength: number;
  windEnvelope: number;
  sourceOrgans: string[];
  clusters?: { bounds: Bounds; sourceOrgans: string[] }[];
  views: { direction: Vec3; firstIndex: number; indexCount: number }[];
  /** Candidate measurements are never silently promoted to reference accuracy. */
  qualification: {
    status: "candidate" | "qualified";
    maximumPixels: number;
    maxWind: number;
    evidence: string;
  };
};
