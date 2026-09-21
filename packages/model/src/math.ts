// Right handed, +Y up, metres / seconds / radians. Matrices are column major;
// quaternions use [x,y,z,w]. Authored colors are linear RGB; display is sRGB.
export const MAX_WORLD_COORDINATE = 1_000_000_000;
export type Vec3 = [number, number, number];
export type Quat = [number, number, number, number];
export type Bounds = { min: Vec3; max: Vec3 };
export const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
export const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
export const normalize = (a: Vec3): Vec3 => scale(a, 1 / (Math.hypot(...a) || 1));
/** Horizontal wind in m/s, capped at ten without overflow from finite authored components. */
export function normalizeWind(wind: Vec3): Vec3 {
  const x = Number.isFinite(wind[0]) ? wind[0] : 0;
  const z = Number.isFinite(wind[2]) ? wind[2] : 0;
  const largest = Math.max(Math.abs(x), Math.abs(z));
  if (largest === 0) return [0, 0, 0];
  const sx = x / largest,
    sz = z / largest;
  const length = Math.hypot(sx, sz);
  // Compare before multiplication, so even Number.MAX_VALUE components remain safe.
  if (largest <= 10 / length) return [x, 0, z];
  return [(sx / length) * 10, 0, (sz / length) * 10];
}
export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
export function hash32(x: number): number {
  x = Math.imul(x ^ (x >>> 16), 0x7feb352d);
  x = Math.imul(x ^ (x >>> 15), 0x846ca68b);
  return (x ^ (x >>> 16)) >>> 0;
}
export function coordinateHash(x: number, z: number, seed: number): number {
  return hash32(Math.imul(x | 0, 0x1f123bb5) ^ Math.imul(z | 0, 0x5f356495) ^ seed);
}
export const random01 = (seed: number) => (hash32(seed) >>> 8) / 16777216;
/** Canonical JSON, including JSON's omission/null policy for optional values.
 * This is an integrity/cache identity, not a cryptographic signature. */
export function canonical(value: unknown): string {
  const omitted = (entry: unknown) =>
    entry === undefined || typeof entry === "function" || typeof entry === "symbol";
  if (Array.isArray(value))
    return `[${Array.from(value, (entry) => (omitted(entry) ? "null" : canonical(entry))).join(",")}]`;
  if (value && typeof value === "object")
    return `{${Object.entries(value)
      .filter(([, entry]) => !omitted(entry))
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([key, entry]) => `${JSON.stringify(key)}:${canonical(entry)}`)
      .join(",")}}`;
  return JSON.stringify(value) ?? "null";
}
export function contentKey(value: unknown): string {
  return contentKeyFromCanonical(canonical(value));
}
/** Hash already canonical source, allowing immutable document serialization to be cached. */
export function contentKeyFromCanonical(s: string): string {
  let a = 0x811c9dc5,
    b = 0x9e3779b9;
  for (let i = 0; i < s.length; i++) {
    a = Math.imul(a ^ s.charCodeAt(i), 0x01000193);
    b = hash32(b ^ s.charCodeAt(i));
  }
  return `${(a >>> 0).toString(16).padStart(8, "0")}${(b >>> 0).toString(16).padStart(8, "0")}`;
}

/** Smooth geometric low-pass: four samples per wavelength are full strength; two are omitted. */
export function waterGeometryAttenuation(wavelength: number, spacing: number): number {
  const t = Math.max(0, Math.min(1, (spacing / wavelength - 0.25) / 0.25));
  return 1 - t * t * (3 - 2 * t);
}
/** Conservative world-height error against exact analytic water, including triangle interpolation. */
export function waterGeometryErrorBound(
  waves: { amplitude: number; wavelength: number }[],
  spacing: number,
): number {
  return waves.reduce((sum, wave) => {
    const attenuation = waterGeometryAttenuation(wave.wavelength, spacing);
    const k = (2 * Math.PI) / wave.wavelength;
    return (
      sum +
      Math.abs(wave.amplitude) *
        (1 - attenuation + attenuation * Math.min(2, (k * k * spacing * spacing) / 4))
    );
  }, 0);
}
