import { add, cross, dot, hash32, normalize, type SurfaceRelief, scale, type Vec3 } from "@wrela/model";

function noise(point: Vec3, seed: number): number {
  const base = point.map(Math.floor),
    f = point.map((value, axis) => value - base[axis]);
  const s = f.map((value) => value * value * (3 - 2 * value));
  let value = 0;
  for (let corner = 0; corner < 8; corner++) {
    const x = corner & 1,
      y = (corner >> 1) & 1,
      z = (corner >> 2) & 1;
    const random =
      (hash32(
        Math.imul(base[0] + x, 73856093) ^
          Math.imul(base[1] + y, 19349663) ^
          Math.imul(base[2] + z, 83492791) ^
          seed,
      ) >>>
        8) /
      16777216;
    value += random * (x ? s[0] : 1 - s[0]) * (y ? s[1] : 1 - s[1]) * (z ? s[2] : 1 - s[2]);
  }
  return value;
}

// Exact Fourier coefficients of ((1 + cos(theta)) / 2)^8. The DC survives filtering.
const RIB_DC = 12870 / 65536;
const RIB_COEFFICIENTS = [22880, 16016, 8736, 3640, 1120, 240, 32, 2].map((value) => value / 65536);
export type SurfaceReliefBands = { mean: number; bands: Vec3 };

/** Signed, complementary bands. Summing all bands reproduces the authored depth. */
export function surfaceReliefBands(position: Vec3, normal: Vec3, source: SurfaceRelief): SurfaceReliefBands {
  const point = position.map((value) => value / source.scale) as Vec3;
  if (source.kind === "stone") {
    const broad = noise(point, source.seed);
    const chips =
      Math.max(0, (noise(point.map((value) => value * 2.7) as Vec3, source.seed + 37) - 0.43) / 0.57) ** 1.5;
    const fine = noise(point.map((value) => value * 7.1) as Vec3, source.seed + 101);
    // A stable DC allocation, not a claim that thresholded coherent noise is uniform.
    return { mean: 0.3895, bands: [(broad - 0.5) * 0.28, (chips - 0.15) * 0.43, (fine - 0.5) * 0.21] };
  }
  const axis = normalize(source.direction);
  const u = normalize(cross(axis, Math.abs(axis[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0])),
    v = cross(axis, u);
  const a = dot(point, u),
    b = dot(point, v),
    along = dot(point, axis);
  const warp =
    Math.sin(along * 0.27 + source.seed) * 0.32 + noise([a * 0.5, along * 0.09, b * 0.5], source.seed) * 0.6;
  const wu = Math.abs(dot(normal, u)) ** 4,
    wv = Math.abs(dot(normal, v)) ** 4;
  const denominator = Math.max(1e-8, wu + wv);
  const bands: Vec3 = [0, 0, 0];
  for (let harmonic = 1; harmonic <= 8; harmonic++) {
    const wave =
      (Math.cos(harmonic * ((b + warp) * Math.PI * 2 - Math.PI / 2)) * wu +
        Math.cos(harmonic * ((a + warp) * Math.PI * 2 - Math.PI / 2)) * wv) /
      denominator;
    const band = harmonic === 1 ? 0 : harmonic <= 3 ? 1 : 2;
    bands[band] += 0.79 * RIB_COEFFICIENTS[harmonic - 1] * wave;
  }
  bands[2] += (noise([a * 3.7, along * 0.7, b * 3.7], source.seed + 73) - 0.5) * 0.175;
  // Axial end caps have no transverse rib projection, matching the original field.
  return { mean: 0.035 + (0.79 * RIB_DC * (wu + wv)) / denominator + 0.0875, bands };
}

export function surfaceReliefDepth(
  position: Vec3,
  normal: Vec3,
  source: SurfaceRelief,
  weights: Vec3 = [1, 1, 1],
): number {
  const { mean, bands } = surfaceReliefBands(position, normal, source);
  return Math.max(0, Math.min(1, mean + dot(bands, weights)));
}

/** Signed residual in metres; geometry and residual sum to the complete authored field. */
export function surfaceReliefResidual(
  position: Vec3,
  normal: Vec3,
  source: SurfaceRelief,
  geometryWeights: Vec3,
): number {
  return (
    source.amplitude *
    (surfaceReliefDepth(position, normal, source) -
      surfaceReliefDepth(position, normal, source, geometryWeights))
  );
}

/** Tangent gradient of inward depth, in metres/metre. Used by CPU evidence and shader parity. */
export function surfaceReliefGradient(
  position: Vec3,
  normal: Vec3,
  source: SurfaceRelief,
  weights: Vec3 = [1, 1, 1],
): Vec3 {
  const n = normalize(normal),
    epsilon = Math.max(1e-7, source.scale * 0.0001);
  const gradient = [0, 1, 2].map((axis) => {
    const delta: Vec3 = [0, 0, 0];
    delta[axis] = epsilon;
    return (
      (source.amplitude *
        (surfaceReliefDepth(add(position, delta), n, source, weights) -
          surfaceReliefDepth(add(position, scale(delta, -1)), n, source, weights))) /
      (2 * epsilon)
    );
  }) as Vec3;
  return add(gradient, scale(n, -dot(gradient, n)));
}

/** Nominal band slope moments. The fixed samples avoid pixel-scale noise in roughness.
 * This samples a representative tangent frame, not every orientation of a curved asset. */
export function surfaceReliefSlopeVariance(source: SurfaceRelief): Vec3 {
  if (source.amplitude === 0) return [0, 0, 0];
  const n =
    source.kind === "bark"
      ? normalize(
          cross(
            normalize(source.direction),
            Math.abs(normalize(source.direction)[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0],
          ),
        )
      : normalize([0.317, 0.811, 0.493]);
  const moments: Vec3 = [0, 0, 0],
    epsilon = source.scale * 0.0001;
  for (let sample = 1; sample <= 64; sample++) {
    const point = [0.754877666, 0.569840296, 0.438447187].map(
      (stride) => ((sample * stride) % 1) * source.scale * 17,
    ) as Vec3;
    const gradients: Vec3[] = [
      [0, 0, 0],
      [0, 0, 0],
      [0, 0, 0],
    ];
    for (let axis = 0; axis < 3; axis++) {
      const delta: Vec3 = [0, 0, 0];
      delta[axis] = epsilon;
      const positive = surfaceReliefBands(add(point, delta), n, source).bands;
      const negative = surfaceReliefBands(add(point, scale(delta, -1)), n, source).bands;
      for (let band = 0; band < 3; band++)
        gradients[band][axis] = (source.amplitude * (positive[band] - negative[band])) / (2 * epsilon);
    }
    for (let band = 0; band < 3; band++) {
      const tangent = add(gradients[band], scale(n, -dot(gradients[band], n)));
      moments[band] += dot(tangent, tangent) / 64;
    }
  }
  return moments;
}
