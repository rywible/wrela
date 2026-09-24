/** Deterministic independent dimensions for continuation/source/roulette
 * decisions. The path index stays stable when a subset is retraced. */
export function radianceRandom(path: number, dimension: number): number {
  let value = ((path + 1) ^ Math.imul(dimension + 1, 0x9e3779b9)) >>> 0;
  value = Math.imul(value ^ (value >>> 16), 0x7feb352d);
  value = Math.imul(value ^ (value >>> 15), 0x846ca68b);
  return ((value ^ (value >>> 16)) >>> 0) / 4294967296;
}
