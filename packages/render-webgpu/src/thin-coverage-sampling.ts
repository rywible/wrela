/** CPU reference for the production WGSL hash and scale blend. Used to audit
 * coarse footprints against a uniform threshold distribution on CPU and GPU. */
export function thinCoverageHashReference(x: number, y: number): number {
  let n = Math.imul(x | 0, 1597334677) ^ Math.imul(y | 0, 3812015801);
  n = Math.imul(n ^ (n >>> 16), 2246822519);
  n = Math.imul(n ^ (n >>> 13), 3266489917);
  return ((n ^ (n >>> 16)) & 16777215) / 16777216;
}

export function thinCoverageThresholdReference(
  position: readonly [number, number],
  footprint: number,
  blend: number,
  salt: readonly [number, number] = [0, 0],
): number {
  const level = Math.round(Math.log2(footprint));
  const hash = (size: number, level: number) =>
    thinCoverageHashReference(
      Math.floor(position[0] / size) + salt[0] + level * 17473,
      Math.floor(position[1] / size) + salt[1] + level * 28753,
    );
  const value = hash(footprint, level) * (1 - blend) + hash(footprint * 2, level + 1) * blend;
  const small = Math.max(Math.min(blend, 1 - blend), 0.00001),
    large = 1 - small;
  if (value < small) return (value * value) / (2 * small * large);
  if (value < large) return (value - small * 0.5) / large;
  return 1 - (1 - value) ** 2 / (2 * small * large);
}
