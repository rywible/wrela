import { type Camera, cross, dot, hash32, normalize, type Vec3 } from "@wrela/model";

import {
  type IndirectGeometry,
  type IndirectHit,
  type IndirectLighting,
  traceIndirectRay,
} from "./indirect-query";

export type IndirectReferenceOptions = {
  /** Independent cosine-weighted first-bounce samples. */
  samples: number;
  /** Secondary hemisphere samples used to resolve the sky at each bounce. */
  skySamples?: number;
  seed?: number;
  /** World metres; applied along the geometric normal. */
  rayOffset?: number;
};
export type IndirectReferenceImage = { width: number; height: number; data: Float32Array };
export type IndirectReferenceRadiance = {
  direct: Vec3;
  /** Sky reaching the receiver without another surface interaction. */
  sky: Vec3;
  /** Exactly one diffuse surface bounce before reaching the receiver. */
  bounce: Vec3;
  total: Vec3;
};

function checkedCount(value: number, name: string, maximum: number): number {
  if (!Number.isSafeInteger(value) || value < 1 || value > maximum)
    throw new RangeError(`${name} must be an integer in [1, ${maximum}]`);
  return value;
}

/** Each sample owns its random stream, so image tiling/order cannot change the result. */
function randomStream(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = hash32(state + 0x9e3779b9);
    return (state + 0.5) / 4294967296;
  };
}

function cosineDirection(normal: Vec3, random: () => number): Vec3 {
  const tangent = normalize(cross(Math.abs(normal[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0], normal));
  const bitangent = cross(normal, tangent);
  const r = Math.sqrt(random()),
    phi = 2 * Math.PI * random();
  const x = r * Math.cos(phi),
    y = r * Math.sin(phi),
    z = Math.sqrt(Math.max(0, 1 - r * r));
  return [
    tangent[0] * x + bitangent[0] * y + normal[0] * z,
    tangent[1] * x + bitangent[1] * y + normal[1] * z,
    tangent[2] * x + bitangent[2] * y + normal[2] * z,
  ];
}

function offset(hit: IndirectHit, distance: number): Vec3 {
  return hit.position.map((v, axis) => v + hit.normal[axis] * distance) as Vec3;
}

function directSun(
  geometry: IndirectGeometry,
  hit: IndirectHit,
  lighting: IndirectLighting,
  rayOffset: number,
): Vec3 {
  const sun = normalize(lighting.sunDirection),
    cosine = Math.max(0, dot(hit.normal, sun));
  if (cosine === 0 || lighting.sunRadiance.every((value) => value === 0)) return [0, 0, 0];
  const blocked = traceIndirectRay(geometry, offset(hit, rayOffset), sun, Infinity, hit.triangle);
  return blocked
    ? [0, 0, 0]
    : (hit.albedo.map((v, axis) => (v * lighting.sunRadiance[axis] * cosine) / Math.PI) as Vec3);
}

function secondaryRadiance(
  geometry: IndirectGeometry,
  hit: IndirectHit,
  lighting: IndirectLighting,
  skySamples: number,
  random: () => number,
  rayOffset: number,
): Vec3 {
  const direct = directSun(geometry, hit, lighting, rayOffset);
  if (lighting.skyRadiance.every((value) => value === 0)) return direct;
  const origin = offset(hit, rayOffset);
  let visible = 0;
  for (let sample = 0; sample < skySamples; sample++) {
    const direction = cosineDirection(hit.normal, random);
    if (!traceIndirectRay(geometry, origin, direction, Infinity, hit.triangle)) visible++;
  }
  return direct.map(
    (v, axis) => v + (hit.albedo[axis] * lighting.skyRadiance[axis] * visible) / skySamples,
  ) as Vec3;
}

/**
 * Slow diffuse transport reference, independent of probe interpolation or cache coefficients.
 * Resolves direct sun, visible constant-radiance sky, and exactly one surface bounce.
 * It uses the same extracted triangles as production; it does not validate that extraction,
 * specular transport, transparency, dynamic geometry, or a directional atmosphere model.
 */
export function sampleIndirectReferenceAtHit(
  geometry: IndirectGeometry,
  hit: IndirectHit,
  lighting: IndirectLighting,
  options: IndirectReferenceOptions,
): IndirectReferenceRadiance {
  const samples = checkedCount(options.samples, "samples", 1_000_000);
  const skySamples = checkedCount(options.skySamples ?? 32, "skySamples", 1_000_000);
  const rayOffset = options.rayOffset ?? 0.0001;
  if (!Number.isFinite(rayOffset) || rayOffset <= 0)
    throw new RangeError("rayOffset must be positive and finite");
  const direct = directSun(geometry, hit, lighting, rayOffset),
    sky: Vec3 = [0, 0, 0],
    bounce: Vec3 = [0, 0, 0];
  const origin = offset(hit, rayOffset),
    random = randomStream(options.seed ?? 1);
  for (let sample = 0; sample < samples; sample++) {
    const direction = cosineDirection(hit.normal, random);
    const secondary = traceIndirectRay(geometry, origin, direction, Infinity, hit.triangle);
    const radiance = secondary
      ? secondaryRadiance(geometry, secondary, lighting, skySamples, random, rayOffset)
      : lighting.skyRadiance;
    const output = secondary ? bounce : sky;
    for (let axis = 0; axis < 3; axis++) output[axis] += (hit.albedo[axis] * radiance[axis]) / samples;
  }
  return { direct, sky, bounce, total: direct.map((v, axis) => v + sky[axis] + bounce[axis]) as Vec3 };
}

/** Linear RGB, top row first, with jittered pinhole camera samples and no tone mapping. */
export function renderIndirectReference(
  geometry: IndirectGeometry,
  camera: Camera,
  lighting: IndirectLighting,
  options: IndirectReferenceOptions & {
    width: number;
    height: number;
    mode?: "beauty" | "indirect";
    jitter?: boolean;
  },
): IndirectReferenceImage {
  const width = checkedCount(options.width, "width", 8192),
    height = checkedCount(options.height, "height", 8192);
  const samples = checkedCount(options.samples, "samples", 1_000_000);
  if (width * height > 16_777_216) throw new RangeError("Reference image exceeds 16 million pixels");
  if (
    !camera.position.every(Number.isFinite) ||
    !camera.target.every(Number.isFinite) ||
    !Number.isFinite(camera.fov) ||
    camera.fov <= 0 ||
    camera.fov >= 179
  )
    throw new RangeError("Reference camera must have finite coordinates and a field of view in (0, 179)");
  const forward = normalize(camera.target.map((v, axis) => v - camera.position[axis]) as Vec3);
  if (dot(forward, forward) === 0) throw new RangeError("Reference camera must look away from its position");
  const right = normalize(cross(forward, Math.abs(forward[1]) < 0.999 ? [0, 1, 0] : [0, 0, 1]));
  const up = cross(right, forward),
    halfHeight = Math.tan((camera.fov * Math.PI) / 360),
    aspect = width / height;
  const data = new Float32Array(width * height * 3);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const pixel = y * width + x,
        sum: Vec3 = [0, 0, 0];
      for (let sample = 0; sample < samples; sample++) {
        const seed = hash32((options.seed ?? 1) ^ hash32(pixel) ^ hash32(sample + 0x1234567));
        const random = randomStream(seed);
        const sx =
          ((2 * (x + (options.jitter === false ? 0.5 : random()))) / width - 1) * halfHeight * aspect;
        const sy = (1 - (2 * (y + (options.jitter === false ? 0.5 : random()))) / height) * halfHeight;
        const direction = normalize(forward.map((v, axis) => v + right[axis] * sx + up[axis] * sy) as Vec3);
        const hit = traceIndirectRay(geometry, camera.position, direction);
        const response = hit
          ? sampleIndirectReferenceAtHit(geometry, hit, lighting, {
              ...options,
              samples: 1,
              seed: hash32(seed ^ 0xabcd1234),
            })
          : undefined;
        const radiance = response
          ? options.mode === "indirect"
            ? response.sky.map((v, i) => v + response.bounce[i])
            : response.total
          : options.mode === "indirect"
            ? [0, 0, 0]
            : lighting.skyRadiance;
        for (let axis = 0; axis < 3; axis++) sum[axis] += radiance[axis] / samples;
      }
      data.set(sum, pixel * 3);
    }
  return { width, height, data };
}

/** Portable float-map bytes retain unclipped linear radiance for numerical/image review. */
export function indirectReferencePFM(image: IndirectReferenceImage): Uint8Array {
  if (image.data.length !== image.width * image.height * 3)
    throw new RangeError("Invalid reference image dimensions");
  const header = new TextEncoder().encode(`PF\n${image.width} ${image.height}\n-1.0\n`);
  const bytes = new Uint8Array(header.length + image.data.byteLength),
    view = new DataView(bytes.buffer);
  bytes.set(header);
  let output = header.length;
  for (let y = image.height - 1; y >= 0; y--)
    for (let x = 0; x < image.width * 3; x++) {
      view.setFloat32(output, image.data[y * image.width * 3 + x], true);
      output += 4;
    }
  return bytes;
}
