import type { RadianceLightingField } from "./radiance-lighting";

/** Shared persistence/GPU boundary validation for the compact surface product. */
export function validRadianceSurfaceCache(cache: RadianceLightingField["surfaceDiffuse"]): boolean {
  if (!cache) return true;
  const count = cache.positions.length;
  if (
    !count ||
    count > 2048 ||
    cache.positions.some((p) => p.length !== 3 || !p.every(Number.isFinite)) ||
    !(cache.transfer instanceof Float32Array) ||
    cache.transfer.length !== count * 108 ||
    !cache.transfer.every(Number.isFinite) ||
    !(cache.receivers instanceof Float32Array) ||
    cache.receivers.length % 12 ||
    cache.receivers.length > 65536 * 12 ||
    !cache.receivers.every(Number.isFinite)
  )
    return false;
  for (let at = 0; at < cache.receivers.length; at += 12) {
    let total = 0;
    for (let j = 0; j < 3; j++) {
      const id = cache.receivers[at + j],
        weight = cache.receivers[at + 4 + j];
      if (
        !Number.isInteger(id) ||
        id < 0 ||
        id >= count ||
        weight < 0 ||
        weight > 1 ||
        cache.receivers[at + 8 + j] < 0
      )
        return false;
      total += weight;
    }
    if (Math.abs(total - 1) > 1e-5) return false;
  }
  if (cache.patches) {
    let next = 0;
    for (const patch of cache.patches) {
      const n = patch.resolution,
        samples = ((n + 1) * (n + 2)) / 2;
      if (
        !Number.isInteger(n) ||
        n < 1 ||
        n > 64 ||
        patch.offset !== next ||
        !Number.isInteger(patch.triangle) ||
        patch.triangle < 0 ||
        patch.triangle >= 200000 ||
        patch.normal.length !== 3 ||
        !patch.normal.every(Number.isFinite) ||
        Math.abs(Math.hypot(...patch.normal) - 1) > 1e-6 ||
        !(patch.validCells instanceof Uint8Array) ||
        patch.validCells.length !== n * n ||
        patch.validCells.some((v) => v > 1) ||
        !(patch.directEmission instanceof Float32Array) ||
        patch.directEmission.length !== samples * 3 ||
        !patch.directEmission.every(Number.isFinite)
      )
        return false;
      next += samples;
    }
    if (next !== count) return false;
  }
  return true;
}
