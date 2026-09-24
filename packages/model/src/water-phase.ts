/** Authored water remains a world-XZ height field, even when its support mesh is transformed. */
export interface WaterPhaseCarrier {
  waveIndex: number;
  spatial: [number, number];
  temporal: number;
  phase: number;
  slope: [number, number];
  amplitude: number;
}
export interface WaterPhaseProduct {
  sourceKey: string;
  algorithmVersion: 2;
  coordinateFrame: "world-xz";
  carriers: WaterPhaseCarrier[];
  glints?: {
    kind: "two-wave-glints";
    indices: [number, number];
    inverseSlope: [number, number, number, number];
  };
}
