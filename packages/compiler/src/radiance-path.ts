import type { Vec3 } from "@wrela/model";
import { indirectSH } from "./indirect-probes";

/** Unprojected transport for one deterministic direction. Keeping this small
 * input-space result allows changed paths to be retraced without retaining a
 * complete 9x27 output matrix for every path. */
export type RadiancePath = {
  direction: Vec3;
  incident: Float64Array;
  /** Optional sparse input IDs; incident then stores contiguous RGB triples. */
  inputs?: Uint8Array;
  sky: boolean;
  emitter?: { direction: Vec3; energy: Vec3 };
  directHit?: Vec3;
};

export function compactRadiancePath(path: RadiancePath): RadiancePath {
  if (path.inputs) return path;
  const inputs: number[] = [],
    values: number[] = [];
  for (let i = 0; i < 27; i++) {
    const rgb = path.incident.subarray(i * 4, i * 4 + 3);
    if (rgb.some((v) => v !== 0)) {
      inputs.push(i);
      values.push(...rgb);
    }
  }
  return { ...path, inputs: new Uint8Array(inputs), incident: new Float64Array(values) };
}

/** Recomposition follows the original path order and Float32 additions. It
 * therefore has no accumulated subtraction drift after repeated edits. */
export function accumulateRadiancePath(
  path: RadiancePath,
  sample: number,
  rayCount: number,
  transfer: Float32Array,
  skyVisibility: Float32Array,
  directEmission?: Float32Array,
) {
  const basis = indirectSH(path.direction);
  if (path.emitter) {
    const emitter = path.emitter;
    indirectSH(emitter.direction).forEach((v, k) => {
      const offset = ((sample * 9 + k) * 27 + 26) * 4;
      for (let c = 0; c < 3; c++) {
        const contribution = emitter.energy[c] * v;
        transfer[offset + c] += contribution;
        if (directEmission) directEmission[(sample * 9 + k) * 3 + c] += contribution;
      }
    });
  }
  if (path.sky)
    basis.forEach((v, k) => {
      skyVisibility[sample * 9 + k] += (v * 4 * Math.PI) / rayCount;
    });
  if (path.directHit && directEmission) {
    const hit = path.directHit;
    for (let c = 0; c < 3; c++)
      basis.forEach((v, k) => {
        directEmission[(sample * 9 + k) * 3 + c] += (hit[c] * v * 4 * Math.PI) / rayCount;
      });
  }
  const inputs = path.inputs;
  for (let j = 0; j < (inputs?.length ?? 27); j++) {
    const i = inputs ? inputs[j] : j,
      start = inputs ? j * 3 : j * 4;
    const red = path.incident[start],
      green = path.incident[start + 1],
      blue = path.incident[start + 2];
    if (red === 0 && green === 0 && blue === 0) continue;
    for (let k = 0; k < 9; k++) {
      const offset = ((sample * 9 + k) * 27 + i) * 4;
      const factor = (basis[k] * 4 * Math.PI) / rayCount;
      transfer[offset] += factor * red;
      transfer[offset + 1] += factor * green;
      transfer[offset + 2] += factor * blue;
    }
  }
}
