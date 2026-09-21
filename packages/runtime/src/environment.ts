import {
  type EnvironmentDefinition,
  type LightingRigDefinition,
  normalize,
  type RenderEnvironment,
  type Vec3,
} from "@wrela/model";
export function evaluateEnvironment(
  environment?: EnvironmentDefinition,
  lighting?: LightingRigDefinition,
  exposure = 1,
  solarOffset = 0,
): RenderEnvironment {
  const elevation = (environment?.sunElevation ?? 0.7) + solarOffset,
    azimuth = environment?.sunAzimuth ?? 0.5;
  const sunDirection = normalize([
    Math.cos(elevation) * Math.sin(azimuth),
    Math.sin(elevation),
    Math.cos(elevation) * Math.cos(azimuth),
  ]);
  const altitude = Math.max(0, Math.sin(elevation)),
    daylight = Math.min(1, Math.max(0, (elevation + 0.1) / 0.3)),
    haze = (environment?.turbidity ?? 2) / 10;
  const sunColor: Vec3 = [1, 0.6 + 0.37 * Math.sqrt(altitude), 0.35 + 0.6 * Math.sqrt(altitude)];
  const directional = lighting?.lights.filter((l) => l.type === "directional");
  const totalIntensity = directional?.reduce((v, l) => v + l.intensity, 0) ?? 2.5;
  const sunIntensity = totalIntensity * daylight;
  if (directional?.length && totalIntensity > 0)
    for (let i = 0; i < 3; i++)
      sunColor[i] *=
        directional.reduce((sum, light) => sum + light.color[i] * light.intensity, 0) / totalIntensity;
  const tint = (color: Vec3, factor: number): Vec3 => color.map((v) => v * factor) as Vec3;
  return {
    pointLights:
      lighting?.lights
        .filter((light) => light.type === "point")
        .map(({ position, color, intensity }) => ({ position, color, intensity })) ?? [],
    sunDirection,
    sunColor,
    sunIntensity,
    ambient: (lighting?.ambient ?? 0.45) * (0.08 + daylight * 0.92),
    skyColor: tint(environment?.skyColor ?? [0.12, 0.3, 0.54], 0.035 + daylight * (1 - haze * 0.2)),
    horizonColor: tint(environment?.horizonColor ?? [0.65, 0.75, 0.78], 0.06 + daylight * 0.94),
    groundColor: environment?.groundColor ?? [0.12, 0.13, 0.15],
    fogDensity: environment?.fogDensity ?? 0.003,
    wind: environment?.wind ?? [0.3, 0, 0.1],
    exposure,
  };
}
