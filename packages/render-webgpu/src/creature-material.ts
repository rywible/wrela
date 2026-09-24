import type { CreatureMaterial, Vec3 } from "@wrela/model";

/** Eight aligned vec4s appended to the existing object layout. No extra texture or storage bindings. */
export const CREATURE_MATERIAL_FLOATS = 32;
export const CREATURE_MATERIAL_OFFSET = 152;
export const CREATURE_FAMILY = { hard: 0, skin: 1, fiber: 2, cloth: 3, eye: 4, wet: 5 } as const;
const finite = (value: number | undefined, fallback: number) =>
  value !== undefined && Number.isFinite(value) ? value : fallback;
const unit = (value: number | undefined, fallback = 0) => Math.max(0, Math.min(1, finite(value, fallback)));
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
function normalized(vector: Vec3 | undefined, fallback: Vec3): Vec3 {
  if (!vector || !vector.every(Number.isFinite)) return fallback;
  const length = Math.hypot(...vector);
  return length > 1e-8 ? (vector.map((value) => value / length) as Vec3) : fallback;
}

/** A right-handed, orthonormal anatomical frame, including deterministic repair of parallel axes. */
export function creatureMaterialFrame(material?: CreatureMaterial): {
  origin: Vec3;
  x: Vec3;
  y: Vec3;
  z: Vec3;
} {
  const frame = material?.frame;
  const z = normalized(frame?.normal, [0, 0, 1]);
  const authoredX = normalized(frame?.tangent, [1, 0, 0]);
  const projection = dot(authoredX, z);
  const projected = authoredX.map((value, index) => value - projection * z[index]) as Vec3;
  const fallbackAxis: Vec3 = Math.abs(z[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
  const x = normalized(projected, normalized(cross(fallbackAxis, z), [1, 0, 0]));
  const y = cross(z, x);
  const origin = frame?.origin.every(Number.isFinite) ? frame.origin : ([0, 0, 0] as Vec3);
  return { origin, x, y, z };
}

/** Defaults are intentionally bounded approximations, not measured optical coefficients. Thickness is metres. */
export function packCreatureMaterial(material?: CreatureMaterial): Float32Array {
  const out = new Float32Array(CREATURE_MATERIAL_FLOATS);
  const family = material?.family ?? "hard";
  const skin = family === "skin";
  const fiber = family === "fiber";
  const coated = family === "eye" || family === "wet";
  out.set([
    CREATURE_FAMILY[family],
    unit(material?.subsurface, skin ? 0.35 : 0),
    unit(material?.transmission, skin ? 0.2 : 0),
    Math.min(1e6, Math.max(0, finite(material?.thickness, 0.005))),
  ]);
  out.set(
    [
      ...(material?.scatterColor ?? [1, 0.4, 0.25]).map((v) => unit(v)),
      unit(material?.sheen, family === "cloth" ? 0.35 : fiber ? 0.2 : 0),
    ],
    4,
  );
  out.set(
    [
      ...normalized(material?.fiberDirection, [0, 1, 0]),
      Math.max(-0.95, Math.min(0.95, finite(material?.anisotropy, fiber ? 0.65 : 0))),
    ],
    8,
  );
  out.set(
    [
      unit(material?.clearcoat, coated ? 0.85 : 0),
      Math.max(0.06, unit(material?.clearcoatRoughness, 0.12)),
      material?.frame ? 1 : 0,
      0,
    ],
    12,
  );
  const frame = creatureMaterialFrame(material);
  out.set(frame.origin, 16);
  out.set(frame.x, 20);
  out.set(frame.y, 24);
  out.set(frame.z, 28);
  return out;
}

/** Converts a rest-space position into the same stable anatomical coordinates used by the shader. */
export function creatureMaterialCoordinates(position: Vec3, material?: CreatureMaterial): Vec3 {
  if (!material?.frame) return [...position];
  const frame = creatureMaterialFrame(material);
  const offset = position.map((value, index) => value - frame.origin[index]) as Vec3;
  return [dot(offset, frame.x), dot(offset, frame.y), dot(offset, frame.z)];
}
