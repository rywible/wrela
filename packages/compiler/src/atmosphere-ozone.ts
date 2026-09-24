/** Piecewise-linear absorption density, altitude in kilometres. Ozone scatters no light. */
export type AtmosphereOzoneLayer = {
  baseHeight: number;
  peakHeight: number;
  topHeight: number;
  extinction: readonly [number, number, number];
};

/** Hillaire's Earth setup, originally wavelength-integrated by Bruneton.
 * https://github.com/sebh/UnrealEngineSkyAtmosphere/blob/183ead5bdacc701b3b626347a680a2f3cd3d4fbd/Application/SkyAtmosphereCommon.cpp */
export const EARTH_OZONE: AtmosphereOzoneLayer = {
  baseHeight: 10,
  peakHeight: 25,
  topHeight: 40,
  extinction: [0.00065, 0.001881, 0.000085],
};

export function validateOzoneLayer(layer: AtmosphereOzoneLayer, atmosphereHeight: number): void {
  if (
    ![layer.baseHeight, layer.peakHeight, layer.topHeight, ...layer.extinction].every(Number.isFinite) ||
    layer.baseHeight < 0 ||
    layer.peakHeight <= layer.baseHeight ||
    layer.topHeight <= layer.peakHeight ||
    layer.topHeight > atmosphereHeight ||
    layer.extinction.some((v) => v < 0 || v > 1000)
  )
    throw new RangeError("Ozone layer outside atmosphere domain");
}

export function ozoneDensity(height: number, layer: AtmosphereOzoneLayer = EARTH_OZONE): number {
  return Math.max(
    0,
    Math.min(
      (height - layer.baseHeight) / (layer.peakHeight - layer.baseHeight),
      (layer.topHeight - height) / (layer.topHeight - layer.peakHeight),
    ),
  );
}

/** Analytic shell-split integral: each density segment is affine in spherical radius.
 * This is an exact real-arithmetic formula; floating-point roundoff is not certified.
 * The caller validates ray/planet intersection and atmosphere bounds. */
export function integrateOzoneRay(
  planetRadius: number,
  ray: { height: number; cosine: number; length: number },
  layer: AtmosphereOzoneLayer = EARTH_OZONE,
): number {
  const radius = planetRadius + ray.height,
    projection = radius * ray.cosine;
  const impactSquared = Math.max(0, radius * radius * (1 - ray.cosine * ray.cosine));
  const impact = Math.sqrt(impactSquared),
    splits = [0, ray.length];
  for (const altitude of [layer.baseHeight, layer.peakHeight, layer.topHeight]) {
    const shell = planetRadius + altitude,
      squared = shell * shell - impactSquared;
    if (squared < 0) continue;
    const root = Math.sqrt(squared);
    for (const distance of [-projection - root, -projection + root])
      if (distance > 0 && distance < ray.length) splits.push(distance);
  }
  splits.sort((a, b) => a - b);
  const radialPrimitive = (u: number) =>
    impact < 1e-8
      ? 0.5 * u * Math.abs(u)
      : 0.5 * (u * Math.hypot(u, impact) + impactSquared * Math.asinh(u / impact));
  let integral = 0;
  for (let i = 1; i < splits.length; i++) {
    const start = splits[i - 1],
      end = splits[i],
      length = end - start;
    const height = Math.hypot(projection + (start + end) * 0.5, impact) - planetRadius;
    if (height <= layer.baseHeight || height >= layer.topHeight || length <= 0) continue;
    const radial = radialPrimitive(projection + end) - radialPrimitive(projection + start);
    const altitudeIntegral = radial - planetRadius * length;
    integral +=
      height <= layer.peakHeight
        ? (altitudeIntegral - layer.baseHeight * length) / (layer.peakHeight - layer.baseHeight)
        : (layer.topHeight * length - altitudeIntegral) / (layer.topHeight - layer.peakHeight);
  }
  return Math.max(0, integral);
}
