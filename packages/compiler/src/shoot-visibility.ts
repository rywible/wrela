import type { Vec3 } from "@wrela/model";

/** Low-frequency, rest-pose escape probability through the authored canopy.
 * Projected needle area accumulates optical depth in a sparse field. This is an
 * isotropic local sky approximation, never a substitute for direct shadows or
 * environment GI, and it follows the shoot during bounded wind motion. */
export function shootSkyVisibility(shoots: { center: Vec3; projectedArea: number }[]): Float32Array {
  const cell = 0.24,
    field = new Map<string, number>();
  const address = (x: number, y: number, z: number) =>
    `${Math.floor(x / cell)},${Math.floor(y / cell)},${Math.floor(z / cell)}`;
  for (const shoot of shoots) {
    const key = address(...shoot.center);
    field.set(key, (field.get(key) ?? 0) + shoot.projectedArea / (cell * cell));
  }
  const directions = Array.from({ length: 12 }, (_, i) => {
    const y = (i + 0.5) / 12,
      a = i * 2.399963229728653,
      r = Math.sqrt(1 - y * y);
    return [Math.cos(a) * r, y, Math.sin(a) * r] as Vec3;
  });
  return Float32Array.from(shoots, (shoot) => {
    let escaped = 0;
    for (const d of directions) {
      let depth = 0,
        last = address(...shoot.center);
      // Local transport only: surrounding trees and terrain belong to GI.
      for (let step = 1; step <= 16; step++) {
        const at = address(
          shoot.center[0] + d[0] * cell * step,
          shoot.center[1] + d[1] * cell * step,
          shoot.center[2] + d[2] * cell * step,
        );
        if (at !== last) depth += field.get(at) ?? 0;
        last = at;
      }
      escaped += Math.exp(-depth);
    }
    return escaped / directions.length;
  });
}
