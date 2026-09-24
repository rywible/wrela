import { contentKey } from "@wrela/model";

import {
  type AtmosphereOzoneLayer,
  EARTH_OZONE,
  integrateOzoneRay,
  validateOzoneLayer,
} from "./atmosphere-ozone";

/** Distances in kilometres; extinction in inverse kilometres. This is an
 * exponential extinction table, not a multiple-scattering or cloud solution. */
export interface AtmosphereComposition {
  planetRadius: number;
  topHeight: number;
  scaleHeight: number;
  extinction: number;
}
export interface AtmosphereRay {
  height: number;
  cosine: number;
  length: number;
}
export interface AtmosphereIntegrationOptions {
  /** Certify transmission over every extinction from zero through this bound. */
  extinctionUpperBound?: number;
  transmissionBudget?: number;
  maxSegments?: number;
  cancelled?: () => boolean;
}
export interface AtmosphereIntegral {
  status: "ready" | "exhausted" | "cancelled";
  lower: number;
  upper: number;
  transmissionLower: number;
  transmissionUpper: number;
  segmentCount: number;
  evidence: "real-arithmetic";
  numericError: "unknown";
}
export const ATMOSPHERE_TABLE_VERSION = 1;
export const MAX_ATMOSPHERE_TABLE_BYTES = (6 + 64 * 128) * 16;
function validateComposition(a: AtmosphereComposition) {
  if (
    ![a.planetRadius, a.topHeight, a.scaleHeight, a.extinction].every(Number.isFinite) ||
    a.planetRadius < 1 ||
    a.planetRadius > 1e8 ||
    a.topHeight < 1e-3 ||
    a.topHeight > a.planetRadius ||
    a.scaleHeight < 1e-4 ||
    a.scaleHeight > 1e5 ||
    a.extinction < 0 ||
    a.extinction > 1e4
  )
    throw new RangeError("Atmosphere composition outside supported domain");
}
function controls(options: AtmosphereIntegrationOptions) {
  const budget = options.transmissionBudget ?? 1e-5;
  const maximum = options.maxSegments ?? 256;
  const extinctionUpperBound = options.extinctionUpperBound;
  if (
    !Number.isFinite(budget) ||
    budget < 1e-10 ||
    budget > 1 ||
    (extinctionUpperBound !== undefined &&
      (!Number.isFinite(extinctionUpperBound) || extinctionUpperBound < 0 || extinctionUpperBound > 10000)) ||
    !Number.isInteger(maximum) ||
    maximum < 1 ||
    maximum > 1024
  )
    throw new RangeError("Atmosphere integration budget outside supported domain");
  return {
    budget,
    maximum,
    ...(extinctionUpperBound === undefined ? {} : { extinctionUpperBound }),
  };
}
/** Maximize exp(-k*lower)-exp(-k*upper) analytically on 0 <= k <= maximum. */
export function atmosphereTransmissionWidth(lower: number, upper: number, maximum: number): number {
  if (upper <= lower) return 0;
  const k = lower > 0 ? Math.min(maximum, Math.log(upper / lower) / (upper - lower)) : maximum;
  return Math.exp(-k * lower) - Math.exp(-k * upper);
}
function altitude(a: AtmosphereComposition, ray: AtmosphereRay, distance: number) {
  const r = a.planetRadius + ray.height;
  const delta = distance * (2 * r * ray.cosine + distance);
  return ray.height + delta / (Math.sqrt(Math.max(0, r * r + delta)) + r);
}
/** Integrates a linear log-density segment without cancellation. */
function exponentialIntegral(logA: number, logB: number, length: number) {
  const delta = Math.abs(logB - logA);
  const mean = delta < 1e-5 ? 1 - delta / 2 + (delta * delta) / 6 : -Math.expm1(-delta) / delta;
  return length * Math.exp(Math.max(logA, logB)) * mean;
}
interface Segment {
  start: number;
  end: number;
  lower: number;
  upper: number;
}
function segment(a: AtmosphereComposition, ray: AtmosphereRay, start: number, end: number): Segment {
  const mid = (start + end) / 2,
    r = a.planetRadius + ray.height;
  const radial = Math.sqrt(r * r + mid * (2 * r * ray.cosine + mid));
  const logMid = -altitude(a, ray, mid) / a.scaleHeight;
  const slope = -(r * ray.cosine + mid) / (a.scaleHeight * radial);
  const half = (end - start) / 2;
  const lower = exponentialIntegral(
    -altitude(a, ray, start) / a.scaleHeight,
    -altitude(a, ray, end) / a.scaleHeight,
    end - start,
  );
  return {
    start,
    end,
    lower,
    // Valid rays remain outside the planet, hence density <= 1. This second
    // real bound also prevents exponential overflow in extreme low scale heights.
    upper: Math.max(
      lower,
      Math.min(end - start, exponentialIntegral(logMid - slope * half, logMid + slope * half, end - start)),
    ),
  };
}
export function integrateAtmosphereRay(
  a: AtmosphereComposition,
  ray: AtmosphereRay,
  options: AtmosphereIntegrationOptions = {},
): AtmosphereIntegral {
  validateComposition(a);
  const { budget, maximum, extinctionUpperBound } = controls(options);
  if (
    ![ray.height, ray.cosine, ray.length].every(Number.isFinite) ||
    ray.height < 0 ||
    ray.height > a.topHeight ||
    Math.abs(ray.cosine) > 1 ||
    ray.length < 0 ||
    ray.length > 4 * (a.planetRadius + a.topHeight)
  )
    throw new RangeError("Atmosphere ray outside supported domain");
  const nearest = Math.max(0, Math.min(ray.length, -(a.planetRadius + ray.height) * ray.cosine));
  if (altitude(a, ray, nearest) < -1e-9 || altitude(a, ray, ray.length) > a.topHeight + 1e-7)
    throw new RangeError("Atmosphere ray intersects planet or exits table domain");
  const segments = [segment(a, ray, 0, ray.length)];
  let lower = segments[0].lower,
    upper = segments[0].upper;
  let status: AtmosphereIntegral["status"] = "ready";
  while (
    (extinctionUpperBound === undefined
      ? Math.exp(-a.extinction * lower) - Math.exp(-a.extinction * upper)
      : atmosphereTransmissionWidth(lower, upper, extinctionUpperBound)) > budget
  ) {
    if (options.cancelled?.()) {
      status = "cancelled";
      break;
    }
    if (segments.length === maximum) {
      status = "exhausted";
      break;
    }
    let worst = 0;
    for (let i = 1; i < segments.length; i++)
      if (segments[i].upper - segments[i].lower > segments[worst].upper - segments[worst].lower) worst = i;
    const old = segments[worst],
      mid = (old.start + old.end) / 2;
    const left = segment(a, ray, old.start, mid),
      right = segment(a, ray, mid, old.end);
    lower += left.lower + right.lower - old.lower;
    upper = Math.max(lower, upper + left.upper + right.upper - old.upper);
    segments[worst] = left;
    segments.push(right);
  }
  if (options.cancelled?.()) status = "cancelled";
  return {
    status,
    lower,
    upper,
    transmissionLower: Math.exp(-a.extinction * upper),
    transmissionUpper: Math.exp(-a.extinction * lower),
    segmentCount: segments.length,
    evidence: "real-arithmetic",
    numericError: "unknown",
  };
}
function distanceToTop(a: AtmosphereComposition, height: number, cosine: number) {
  const r = a.planetRadius + height,
    projected = r * cosine;
  const delta = (a.topHeight - height) * (2 * a.planetRadius + a.topHeight + height);
  // Use the rationalized branch only where its denominator cannot cancel.
  return projected >= 0
    ? delta === 0
      ? 0
      : delta / (Math.sqrt(projected * projected + delta) + projected)
    : Math.sqrt(projected * projected + delta) - projected;
}
export interface AtmosphereTableOptions extends AtmosphereIntegrationOptions {
  heightCount?: number;
  cosineCount?: number;
  maxBytes?: number;
}
export interface CompiledAtmosphereTable {
  kind: "atmosphere-optical-depth";
  version: 1;
  key: string;
  sourceKey: string;
  status: "ready" | "exhausted";
  /** Header[0]: radius/top/scale/extinction. Header[1]: dimensions/version/reserved.
   * Cells: estimated/lower/upper density integral, status (0 ready, 1 exhausted,
   * 2 planet occluded). Row is height, column is zenith cosine [-1,1]. */
  data: Float32Array;
  byteLength: number;
  heightCount: number;
  cosineCount: number;
  exhaustedCells: number;
  maximumTransmissionWidth: number;
  evidence: "real-arithmetic";
  numericError: "unknown";
  interpolationError: "unknown";
  fallback: "existing-sky";
}
/** Bounded and cancellable during worker cooking. The integration bound applies
 * at table nodes only; FP32 storage and bilinear interpolation are not certified. */
export function compileAtmosphereTable(
  a: AtmosphereComposition,
  options: AtmosphereTableOptions = {},
): CompiledAtmosphereTable | { status: "cancelled" } {
  validateComposition(a);
  const { budget, maximum } = controls(options);
  const heightCount = options.heightCount ?? 32,
    cosineCount = options.cosineCount ?? 64;
  const maxBytes = options.maxBytes ?? MAX_ATMOSPHERE_TABLE_BYTES;
  if (
    !Number.isInteger(heightCount) ||
    heightCount < 2 ||
    heightCount > 64 ||
    !Number.isInteger(cosineCount) ||
    cosineCount < 2 ||
    cosineCount > 128 ||
    !Number.isInteger(maxBytes) ||
    maxBytes < 0 ||
    maxBytes > MAX_ATMOSPHERE_TABLE_BYTES
  )
    throw new RangeError("Atmosphere table allocation outside limits");
  const byteLength = (2 + heightCount * cosineCount) * 16;
  if (byteLength > maxBytes) throw new RangeError("Atmosphere table exceeds byte budget");
  if (options.cancelled?.()) return { status: "cancelled" };
  const data = new Float32Array(byteLength / 4);
  data.set([
    a.planetRadius,
    a.topHeight,
    a.scaleHeight,
    a.extinction,
    heightCount,
    cosineCount,
    ATMOSPHERE_TABLE_VERSION,
    0,
  ]);
  let exhaustedCells = 0,
    maximumTransmissionWidth = 0;
  for (let y = 0; y < heightCount; y++)
    for (let x = 0; x < cosineCount; x++) {
      if (options.cancelled?.()) return { status: "cancelled" };
      const height = (a.topHeight * y) / (heightCount - 1),
        cosine = -1 + (2 * x) / (cosineCount - 1);
      const length = distanceToTop(a, height, cosine),
        ray = { height, cosine, length };
      const index = (2 + y * cosineCount + x) * 4;
      const nearest = Math.max(0, Math.min(length, -(a.planetRadius + height) * cosine));
      if (altitude(a, ray, nearest) < -1e-9) {
        data[index + 3] = 2;
        continue;
      }
      const integral = integrateAtmosphereRay(a, ray, options);
      if (integral.status === "cancelled") return { status: "cancelled" };
      if (integral.status === "exhausted") exhaustedCells++;
      maximumTransmissionWidth = Math.max(
        maximumTransmissionWidth,
        integral.transmissionUpper - integral.transmissionLower,
      );
      data.set(
        [
          (integral.lower + integral.upper) / 2,
          integral.lower,
          integral.upper,
          integral.status === "ready" ? 0 : 1,
        ],
        index,
      );
    }
  const sourceKey = contentKey({
    algorithm: "spherical-exponential",
    version: ATMOSPHERE_TABLE_VERSION,
    composition: a,
  });
  return {
    kind: "atmosphere-optical-depth",
    version: ATMOSPHERE_TABLE_VERSION,
    key: contentKey({ sourceKey, heightCount, cosineCount, budget, maximum }),
    sourceKey,
    status: exhaustedCells ? "exhausted" : "ready",
    data,
    byteLength,
    heightCount,
    cosineCount,
    exhaustedCells,
    maximumTransmissionWidth,
    evidence: "real-arithmetic",
    numericError: "unknown",
    interpolationError: "unknown",
    fallback: "existing-sky",
  };
}

export interface ScatteringAtmosphereComposition {
  planetRadius: number;
  topHeight: number;
  rayleighScaleHeight: number;
  aerosolScaleHeight: number;
  mistScaleHeight: number;
  rayleighExtinction: readonly [number, number, number];
  aerosolExtinction: number;
  aerosolAlbedo: number;
  mistExtinction: number;
  anisotropy: number;
  sunAngularRadius: number;
  /** Optional extinction-only constituent; absence preserves an ozone-free medium. */
  ozone?: AtmosphereOzoneLayer;
}
export interface ScatteringAtmosphereTable {
  key: string;
  data: Float32Array;
  byteLength: number;
  version: 3;
  exhaustedCells: number;
  maximumTransmissionWidth: number;
  evidence: "real-arithmetic";
  numericError: "unknown";
  interpolationError: "unknown";
}
/** Three exponential constituents and an optional ozone tent share a horizon-relative domain.
 * Height uses a fourth-power warp to concentrate resolution near ground; cosine
 * uses another quadratic warp to resolve long near-tangent sunlight paths.
 * V3 preserves the four-vector header/cell offset: cell.w is ozone column;
 * two trailing vectors hold absorption RGB and base/peak/top layer altitudes. */
export function compileScatteringAtmosphereTable(
  a: ScatteringAtmosphereComposition,
  options: AtmosphereTableOptions = {},
): ScatteringAtmosphereTable | { status: "cancelled" } {
  const rows = options.heightCount ?? 48,
    columns = options.cosineCount ?? 64;
  const maximumBytes = options.maxBytes ?? MAX_ATMOSPHERE_TABLE_BYTES;
  if (
    !Number.isInteger(rows) ||
    rows < 2 ||
    rows > 64 ||
    !Number.isInteger(columns) ||
    columns < 2 ||
    columns > 128 ||
    (6 + rows * columns) * 16 > maximumBytes ||
    maximumBytes > MAX_ATMOSPHERE_TABLE_BYTES
  )
    throw new RangeError("Scattering atmosphere table exceeds allocation domain");
  const coefficients = [...a.rayleighExtinction, a.aerosolExtinction, a.mistExtinction];
  if (
    coefficients.some((v) => !Number.isFinite(v) || v < 0 || v > 1000) ||
    !Number.isFinite(a.aerosolAlbedo) ||
    a.aerosolAlbedo < 0 ||
    a.aerosolAlbedo > 1 ||
    !Number.isFinite(a.anisotropy) ||
    Math.abs(a.anisotropy) > 0.95 ||
    !Number.isFinite(a.sunAngularRadius) ||
    a.sunAngularRadius < 0.0001 ||
    a.sunAngularRadius > 0.1
  )
    throw new RangeError("Scattering atmosphere coefficients outside domain");
  const scales = [a.rayleighScaleHeight, a.aerosolScaleHeight, a.mistScaleHeight];
  const extinctions = [Math.max(...a.rayleighExtinction), a.aerosolExtinction, a.mistExtinction];
  const components = scales.map((scaleHeight, i) => ({
    planetRadius: a.planetRadius,
    topHeight: a.topHeight,
    scaleHeight,
    extinction: extinctions[i],
  }));
  for (const component of components) validateComposition(component);
  if (a.ozone) validateOzoneLayer(a.ozone, a.topHeight);
  const control = controls(options);
  if (options.cancelled?.()) return { status: "cancelled" };
  const data = new Float32Array((6 + rows * columns) * 4);
  data.set([
    a.planetRadius,
    a.topHeight,
    a.rayleighScaleHeight,
    a.aerosolScaleHeight,
    ...a.rayleighExtinction,
    a.aerosolExtinction * a.aerosolAlbedo,
    a.aerosolExtinction,
    a.mistExtinction,
    a.anisotropy,
    a.sunAngularRadius,
    rows,
    columns,
    3,
    a.mistScaleHeight,
  ]);
  const ozoneOffset = (4 + rows * columns) * 4;
  data.set(
    a.ozone
      ? [...a.ozone.extinction, 0, a.ozone.baseHeight, a.ozone.peakHeight, a.ozone.topHeight, 0]
      : [0, 0, 0, 0, 0, a.topHeight * 0.5, a.topHeight, 0],
    ozoneOffset,
  );
  let exhaustedCells = 0,
    maximumTransmissionWidth = 0;
  for (let y = 0; y < rows; y++)
    for (let x = 0; x < columns; x++) {
      if (options.cancelled?.()) return { status: "cancelled" };
      const height = a.topHeight * (y / (rows - 1)) ** 4;
      const radius = a.planetRadius + height;
      const horizon = -Math.sqrt(Math.max(0, height * (2 * a.planetRadius + height))) / radius;
      // A tiny inward-domain displacement avoids a rounded underground tangent.
      const cosine = Math.min(1, horizon + (1 - horizon) * (x / (columns - 1)) ** 2 + 1e-10);
      const length = distanceToTop(components[0], height, cosine);
      const index = (4 + y * columns + x) * 4;
      for (let k = 0; k < 3; k++) {
        const result = integrateAtmosphereRay(components[k], { height, cosine, length }, options);
        if (result.status === "cancelled") return { status: "cancelled" };
        if (result.status === "exhausted") exhaustedCells++;
        maximumTransmissionWidth = Math.max(
          maximumTransmissionWidth,
          options.extinctionUpperBound === undefined
            ? result.transmissionUpper - result.transmissionLower
            : atmosphereTransmissionWidth(result.lower, result.upper, options.extinctionUpperBound),
        );
        data[index + k] = (result.lower + result.upper) / 2;
      }
      if (a.ozone) data[index + 3] = integrateOzoneRay(a.planetRadius, { height, cosine, length }, a.ozone);
    }
  return {
    key: contentKey({ version: 3, composition: a, rows, columns, control }),
    data,
    byteLength: data.byteLength,
    version: 3,
    exhaustedCells,
    maximumTransmissionWidth,
    evidence: "real-arithmetic",
    numericError: "unknown",
    interpolationError: "unknown",
  };
}

export function authoredAtmosphereComposition(
  turbidity = 2,
  fogDensity = 0.003,
): ScatteringAtmosphereComposition {
  if (
    !Number.isFinite(turbidity) ||
    turbidity < 1 ||
    turbidity > 10 ||
    !Number.isFinite(fogDensity) ||
    fogDensity < 0 ||
    fogDensity > 0.1
  )
    throw new RangeError("Invalid authored atmosphere");
  return {
    planetRadius: 6371,
    topHeight: 100,
    rayleighScaleHeight: 8,
    aerosolScaleHeight: 1.2,
    mistScaleHeight: 0.012,
    rayleighExtinction: [0.005802, 0.013558, 0.0331],
    aerosolExtinction: (0.003996 * turbidity) / 2,
    aerosolAlbedo: 0.9,
    // Authored fog density is measured at world Y=0, twenty metres above the
    // atmosphere datum. A shallow valley layer must not become a 120m fog bank.
    mistExtinction: fogDensity * 1000 * Math.exp(20 / 12),
    anisotropy: 0.76,
    sunAngularRadius: 0.00465,
    ozone: EARTH_OZONE,
  };
}
