import { compileAuthoredAtmosphere } from "@wrela/compiler";
import {
  type EnvironmentDefinition,
  type EnvironmentGrade,
  type EnvironmentState,
  type LightingRigDefinition,
  normalize,
  type RenderEnvironment,
  type Vec3,
  type WaterDefinition,
} from "@wrela/model";

import { applyLightingZones, sampleDayCycle, sampleEnvironmentSequence } from "./environment-state";

const atmosphereCache = new Map<string, NonNullable<RenderEnvironment["atmosphere"]>>();
/** Composition cooking happens once on environment evaluation, before renderer
 * submission. A small cache bounds CPU bytes; solar/camera changes reuse it. */
export function evaluateAtmosphere(
  turbidity = 2,
  fogDensity = 0.003,
): NonNullable<RenderEnvironment["atmosphere"]> {
  const key = `${turbidity}/${fogDensity}`;
  const cached = atmosphereCache.get(key);
  if (cached) return cached;
  const table = compileAuthoredAtmosphere(turbidity, fogDensity);
  const product: NonNullable<RenderEnvironment["atmosphere"]> = {
    key: table.key,
    data: table.data,
    byteLength: table.byteLength,
    planetCenter: [0, -6371020, 0],
  };
  if (atmosphereCache.size >= 4) atmosphereCache.delete(atmosphereCache.keys().next().value as string);
  atmosphereCache.set(key, product);
  return product;
}
/** Weather sampling is independent of atmospheric tables and camera exposure. */
export function evaluateEnvironmentState(
  environment?: EnvironmentDefinition,
  lighting?: LightingRigDefinition,
  time = 0,
  position: Vec3 = [0, 0, 0],
): EnvironmentState {
  const defaultGrade: EnvironmentGrade = { exposureCompensation: 0, tint: [1, 1, 1] };
  const state: EnvironmentState = environment?.sequence
    ? sampleEnvironmentSequence(environment.sequence, time)
    : {
        sunElevation: environment?.sunElevation ?? 0.7,
        sunAzimuth: environment?.sunAzimuth ?? 0.5,
        turbidity: environment?.turbidity ?? 2,
        fogDensity: environment?.fogDensity ?? 0.003,
        cloudCover: environment?.cloudCover ?? 0,
        cloudscape: environment?.cloudscape,
        wind: environment?.wind ?? [0.3, 0, 0.1],
        ambient: lighting?.ambient ?? 0.45,
        sunIntensity:
          lighting?.lights.filter((l) => l.type === "directional").reduce((v, l) => v + l.intensity, 0) ??
          2.5,
        wetness: 0,
        grade: defaultGrade,
      };
  if (environment?.dayCycle) {
    const sun = sampleDayCycle(environment.dayCycle, time);
    state.sunElevation = sun.elevation;
    state.sunAzimuth = sun.azimuth;
  }
  return applyLightingZones(state, lighting?.zones ?? [], position);
}

const waterWeatherCache = new WeakMap<WaterDefinition, { speed: number; result: WaterDefinition }>();
/** Wind affects authored wave energy, never the carrier phase or channel geometry.
 * Seeking and playback therefore share a bounded, history-independent realization. */
export function evaluateWaterEnvironment(
  water: WaterDefinition,
  environment: Pick<EnvironmentState, "wind">,
): WaterDefinition {
  const response = water.flow?.weatherResponse;
  if (!response) return water;
  if (!environment.wind.every(Number.isFinite)) throw new Error("Water weather wind must be finite");
  const speed = Math.hypot(environment.wind[0], environment.wind[2]);
  const cached = waterWeatherCache.get(water);
  if (cached?.speed === speed) return cached.result;
  const delta = speed - response.referenceWindSpeed;
  const gain = Math.max(0.1, Math.min(4, 1 + delta * response.waveGain));
  const roughness = Math.max(0.02, Math.min(1, water.roughness + delta * response.roughnessGain));
  if (gain === 1 && roughness === water.roughness) return water;
  const result: WaterDefinition = {
    ...water,
    roughness,
    ...(water.spectrum
      ? { spectrum: { ...water.spectrum, amplitude: Math.min(3, water.spectrum.amplitude * gain) } }
      : {}),
    // Authored water limits still bound every evaluated realization.
    waves: water.waves.map((wave) => ({ ...wave, amplitude: Math.min(3, wave.amplitude * gain) })),
  };
  waterWeatherCache.set(water, { speed, result });
  return result;
}

export function evaluateEnvironment(
  environment?: EnvironmentDefinition,
  lighting?: LightingRigDefinition,
  exposure = 1,
  solarOffset = 0,
  options: { time?: number; position?: Vec3; grade?: EnvironmentGrade } = {},
): RenderEnvironment {
  const defaultGrade: EnvironmentGrade = { exposureCompensation: 0, tint: [1, 1, 1] };
  const state = evaluateEnvironmentState(
    environment,
    lighting,
    options.time ?? 0,
    options.position ?? [0, 0, 0],
  );
  const grades = [state.grade, environment?.grade ?? defaultGrade, options.grade ?? defaultGrade];
  const tint = grades.reduce<Vec3>(
    (color, grade) => color.map((v, i) => v * grade.tint[i]) as Vec3,
    [1, 1, 1],
  );
  const stops = grades.reduce((sum, grade) => sum + grade.exposureCompensation, 0);
  const elevation = state.sunElevation + solarOffset,
    azimuth = state.sunAzimuth;
  const sunDirection = normalize([
    Math.cos(elevation) * Math.sin(azimuth),
    Math.sin(elevation),
    Math.cos(elevation) * Math.cos(azimuth),
  ]);
  const nightProgress = Math.max(0, Math.min(1, (-state.sunElevation - 0.06) / 0.28));
  const nightFactor = nightProgress * nightProgress * (3 - 2 * nightProgress);
  const overcast = Math.max(0, Math.min(1, ((state.cloudCover ?? 0) - 0.25) / 0.6));
  const horizonExposure = environment?.dayCycle
    ? 0.85 * Math.max(0, 1 - Math.abs(state.sunElevation) / 0.27) * overcast * overcast * (3 - 2 * overcast)
    : 0;
  const moonDirection: Vec3 = sunDirection.map((value) => -value) as Vec3;
  // Solar irradiance before atmospheric transport. The same compiled extinction
  // colors direct sun, sky, reflections, and aerial perspective on the GPU.
  const sunColor: Vec3 = [...tint];
  const directional = lighting?.lights.filter((l) => l.type === "directional");
  const totalIntensity = directional?.reduce((v, l) => v + l.intensity, 0) ?? 2.5;
  const sunIntensity = state.sunIntensity;
  if (directional?.length && totalIntensity > 0)
    for (let i = 0; i < 3; i++)
      sunColor[i] *=
        directional.reduce((sum, light) => sum + light.color[i] * light.intensity, 0) / totalIntensity;
  return {
    atmosphere: evaluateAtmosphere(state.turbidity, state.fogDensity),
    pointLights:
      lighting?.lights
        .filter((light) => light.type === "point")
        .map(({ position, color, intensity, range, shadows }) => ({
          position,
          color: color.map((v, i) => v * tint[i]) as Vec3,
          intensity,
          range,
          shadows,
        })) ?? [],
    sunDirection,
    sunColor,
    sunIntensity,
    ambient: state.ambient * (environment?.dayCycle ? 1 - nightFactor * 0.82 : 1),
    skyColor: environment?.skyColor ?? [0.12, 0.3, 0.54],
    horizonColor: environment?.horizonColor ?? [0.65, 0.75, 0.78],
    groundColor: environment?.groundColor ?? [0.12, 0.13, 0.15],
    fogDensity: state.fogDensity,
    cloudCover: state.cloudCover ?? 0,
    cloudscape: state.cloudscape ?? environment?.cloudscape,
    moonDirection,
    moonIntensity: (environment?.dayCycle?.moonIntensity ?? 0.055) * nightFactor,
    nightFactor,
    starRotation: environment?.dayCycle
      ? 2 *
        Math.PI *
        ((environment.dayCycle.startHour / 24 + (options.time ?? 0) / environment.dayCycle.duration) % 1)
      : 0,
    starLatitude: environment?.dayCycle ? (environment.dayCycle.latitude * Math.PI) / 180 : 0,
    starNorthOffset: environment?.dayCycle?.northOffset ?? 0,
    wind: state.wind,
    wetness: state.wetness,
    exposure: Math.max(
      0.0001,
      Math.min(
        65536,
        exposure * 2 ** (stops + horizonExposure + nightFactor * (environment?.dayCycle?.nightExposure ?? 0)),
      ),
    ),
  };
}
