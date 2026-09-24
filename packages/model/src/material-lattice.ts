import type { Vec3 } from "./math";
/** Exact corner samples of the periodic material noise field, shared by all materials.
 * Two RGBA texels contain the eight lattice corners of each cell. Outside this
 * bounded domain the original analytic evaluator remains authoritative. */
export type MaterialLattice = {
  version: "material-lattice-1";
  key: string;
  origin: Vec3;
  edge: number;
  corners: Float32Array;
};
