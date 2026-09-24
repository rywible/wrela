import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import {
  type Cloudscape,
  dayCycleSchema,
  type EnvironmentDefinition,
  environmentSequenceSchema,
  environmentStateSchema,
  type LightingRigDefinition,
  lightingZoneSchema,
} from "@wrela/model";
import { evaluateEnvironment } from "./environment";
import {
  applyLightingZones,
  lightingZoneWeight,
  sampleDayCycle,
  sampleEnvironmentSequence,
} from "./environment-state";

const state = environmentStateSchema.parse({
  sunElevation: 0.5,
  sunAzimuth: 3,
  turbidity: 2,
  fogDensity: 0.002,
  wind: [0, 0, 0],
  ambient: 0.5,
  sunIntensity: 2,
});
const sequence = environmentSequenceSchema.parse({
  duration: 20,
  loop: true,
  interpolation: "linear",
  keyframes: [
    { time: 0, state },
    {
      time: 10,
      state: {
        ...state,
        sunAzimuth: -3,
        wind: [4, 0, 2],
        wetness: 1,
        grade: { exposureCompensation: 2, tint: [0.5, 0.75, 1] },
      },
    },
  ],
});

test("weather sampling coordinates channels, wraps time, and follows the short azimuth arc", () => {
  const middle = sampleEnvironmentSequence(sequence, 5);
  expect(middle.wind).toEqual([2, 0, 1]);
  expect(middle.wetness).toBe(0.5);
  expect(middle.sunAzimuth).toBeCloseTo(Math.PI);
  expect(middle.grade.exposureCompensation).toBe(1);
  expect(sampleEnvironmentSequence(sequence, 25)).toEqual(middle);
  expect(sampleEnvironmentSequence(sequence, -15)).toEqual(middle);
  expect(sampleEnvironmentSequence(sequence, 20)).toEqual(sampleEnvironmentSequence(sequence, 0));
  expect(sampleEnvironmentSequence({ ...sequence, loop: false }, 50)).toEqual(sequence.keyframes[1].state);
  expect(() => sampleEnvironmentSequence(sequence, NaN)).toThrow();
});

test("solar clock wraps continuously while authored weather stays on its own period", () => {
  const cycle = dayCycleSchema.parse({
    duration: 80,
    startHour: 6,
    latitude: 43,
    declination: 10,
    northOffset: -0.75,
  });
  expect(sampleDayCycle(cycle, 80)).toEqual(sampleDayCycle(cycle, 0));
  expect(sampleDayCycle(cycle, 20).elevation).toBeGreaterThan(sampleDayCycle(cycle, 0).elevation);
  expect(sampleDayCycle(cycle, 60).elevation).toBeLessThan(-0.5);
  expect(() => sampleDayCycle(cycle, Number.NaN)).toThrow();
  const project = referenceProject();
  const sky = project.documents.find((d): d is EnvironmentDefinition => d.kind === "environment");
  if (!sky) throw new Error("Missing reference sky");
  const day = evaluateEnvironment({ ...sky, dayCycle: cycle, sequence }, undefined, 1, 0, { time: 20 });
  const night = evaluateEnvironment({ ...sky, dayCycle: cycle, sequence }, undefined, 1, 0, { time: 60 });
  expect(day.nightFactor).toBe(0);
  expect(night.nightFactor).toBe(1);
  expect(night.moonIntensity).toBeGreaterThan(0);
  expect(night.starRotation).toBeCloseTo(0);
  expect(day.starRotation).toBeCloseTo(Math.PI);
  expect(night.starLatitude).toBeCloseTo((43 * Math.PI) / 180);
  expect(night.starNorthOffset).toBe(-0.75);
  expect(night.exposure).toBeGreaterThan(day.exposure);
  expect(night.ambient).toBeLessThan(day.ambient);
});

test("weather fronts and cloud types interpolate without changing the solar clock", () => {
  const first = {
    development: 0.8,
    storminess: 0,
    highCloudCover: 0.2,
    front: {
      origin: [0, 0] as [number, number],
      direction: [1, 0] as [number, number],
      width: 1000,
      strength: 0,
    },
  };
  const second = {
    development: 0.2,
    storminess: 1,
    highCloudCover: 0.6,
    front: {
      origin: [4000, 0] as [number, number],
      direction: [1, 0] as [number, number],
      width: 3000,
      strength: 1,
    },
  };
  const weather = {
    ...sequence,
    keyframes: [
      { time: 0, state: { ...state, cloudscape: first } },
      { time: 10, state: { ...state, cloudscape: second } },
    ],
  };
  const middle = sampleEnvironmentSequence(weather, 5);
  expect(middle.cloudscape?.front?.origin[0]).toBe(2000);
  expect(middle.cloudscape?.storminess).toBe(0.5);
  expect(middle.sunElevation).toBe(state.sunElevation);
});

test("lighting zones blend continuously and resolve overlaps deterministically", () => {
  const zone = lightingZoneSchema.parse({
    id: "interior",
    name: "Interior",
    center: [0, 0, 0],
    radius: 10,
    blendDistance: 2,
    ambient: 0.1,
    sunIntensity: 0,
    priority: 2,
  });
  expect(lightingZoneWeight(zone, [0, 0, 0])).toBe(1);
  expect(lightingZoneWeight(zone, [9, 0, 0])).toBe(0.5);
  expect(lightingZoneWeight(zone, [10, 0, 0])).toBe(0);
  const other = { ...zone, id: "outside", priority: 0, ambient: 2, sunIntensity: 8 };
  const result = applyLightingZones(state, [zone, other], [0, 0, 0]);
  expect(result.ambient).toBeCloseTo(0.1);
  expect(result.sunIntensity).toBe(0);
  expect(result).toEqual(applyLightingZones(state, [other, zone], [0, 0, 0]));
  expect(state.ambient).toBe(0.5);
});

test("environment output consumes sequence, zone, exposure, and color controls", () => {
  const project = referenceProject();
  const source = project.documents.find((d): d is EnvironmentDefinition => d.kind === "environment");
  const lights = project.documents.find((d): d is LightingRigDefinition => d.kind === "lighting");
  if (!source || !lights) throw new Error("Missing reference environment");
  const result = evaluateEnvironment({ ...source, sequence }, { ...lights, lights: [] }, 2, 0, {
    time: 5,
    grade: { exposureCompensation: 1, tint: [1, 0.5, 1] },
  });
  expect(result.exposure).toBe(8);
  expect(result.wind).toEqual([2, 0, 1]);
  expect(result.sunColor).toEqual([0.75, 0.4375, 1]);
  expect(result.wetness).toBe(0.5);
  expect(result.sunIntensity).toBe(2);
});

test("invalid sequence timings fail at authoring boundaries", () => {
  expect(environmentSequenceSchema.safeParse({ ...sequence, duration: 5 }).success).toBe(false);
  expect(
    environmentSequenceSchema.safeParse({
      ...sequence,
      keyframes: [sequence.keyframes[0], sequence.keyframes[0]],
    }).success,
  ).toBe(false);
  expect(environmentStateSchema.safeParse({ ...state, wind: [Infinity, 0, 0] }).success).toBe(false);
});

test("alpine lookdev source validates and supplies coherent clear/clouded/wet states", async () => {
  const { createDayNightLookdev, createEnvironmentLookdev } = await import(
    "@wrela/examples/environment-lookdev"
  );
  const { documentSchema } = await import("@wrela/model");
  const preset = createEnvironmentLookdev();
  for (const document of preset.documents) expect(documentSchema.safeParse(document).success).toBe(true);
  const environment = preset.documents.find((d) => d.id === preset.environment);
  if (environment?.kind !== "environment" || !environment.sequence)
    throw new Error("Missing lookdev environment sequence");
  expect(sampleEnvironmentSequence(environment.sequence, 0).wetness).toBe(0);
  // Clouds attenuate sunlight in transport; do not dim the source a second time.
  expect(sampleEnvironmentSequence(environment.sequence, 32).sunIntensity).toBe(
    sampleEnvironmentSequence(environment.sequence, 0).sunIntensity,
  );
  expect(sampleEnvironmentSequence(environment.sequence, 32).cloudCover).toBeGreaterThan(0.85);
  expect(sampleEnvironmentSequence(environment.sequence, 64).wetness).toBeGreaterThan(0.8);
  for (const document of createDayNightLookdev().documents)
    expect(documentSchema.safeParse(document).success).toBe(true);
});

test("room lighting volumes rotate in metres and retain a continuous interior blend", () => {
  const room = lightingZoneSchema.parse({
    id: "room",
    name: "Room",
    center: [10, 2, -4],
    radius: 10,
    box: { halfExtents: [4, 2, 1], yaw: Math.PI / 2 },
    blendDistance: 0.5,
    ambient: 0.1,
    sunIntensity: 0,
  });
  expect(lightingZoneWeight(room, [10, 2, -4])).toBe(1);
  expect(lightingZoneWeight(room, [10, 2, -0.25])).toBeCloseTo(0.5);
  expect(lightingZoneWeight(room, [11, 2, -4])).toBe(0);
  expect(lightingZoneWeight(room, [10, 4, -4])).toBe(0);
  expect(
    lightingZoneSchema.safeParse({ ...room, box: { ...room.box, halfExtents: [4, 0, 1] } }).success,
  ).toBe(false);
});

test("weather-driven water preserves carrier timing, support geometry, source ownership, and authored bounds", async () => {
  const { evaluateWaterEnvironment } = await import("./environment");
  const { waterSchema } = await import("@wrela/model");
  const { compileWaterPhases, queryWater } = await import("@wrela/compiler");
  const original = referenceProject().documents.find((d) => d.kind === "water");
  if (original?.kind !== "water") throw new Error("Water fixture missing");
  expect(evaluateWaterEnvironment(original, { wind: [10, 0, 0] })).toBe(original);
  const water = waterSchema.parse({
    ...original,
    flow: {
      velocity: [0, 0],
      weatherResponse: { referenceWindSpeed: 1, waveGain: 0.2, roughnessGain: 0.02 },
    },
  });
  expect(evaluateWaterEnvironment(water, { wind: [1, 0, 0] })).toBe(water);
  const storm = evaluateWaterEnvironment(water, { wind: [6, 0, 0] });
  expect(storm.waves[0].amplitude).toBe(water.waves[0].amplitude * 2);
  expect(storm.roughness).toBeCloseTo(water.roughness + 0.1);
  expect(storm.flow).toBe(water.flow);
  expect(storm.level).toBe(water.level);
  const before = compileWaterPhases(water),
    after = compileWaterPhases(storm);
  expect(after.carriers.map((carrier) => carrier.temporal)).toEqual(
    before.carriers.map((carrier) => carrier.temporal),
  );
  expect(after.carriers.map((carrier) => carrier.phase)).toEqual(
    before.carriers.map((carrier) => carrier.phase),
  );
  expect(queryWater(storm, 1, 2, 0).height - storm.level).toBeCloseTo(
    (queryWater(water, 1, 2, 0).height - water.level) * 2,
  );
  expect(evaluateWaterEnvironment(water, { wind: [6, 0, 0] })).toBe(storm);
  expect(() => evaluateWaterEnvironment(water, { wind: [NaN, 0, 0] })).toThrow("finite");
  expect(waterSchema.safeParse(evaluateWaterEnvironment(water, { wind: [1000, 0, 0] })).success).toBe(true);
});

test("semantic cloud formations preserve identity and fade across weather keys", () => {
  const form = {
    id: "hero",
    kind: "tower" as const,
    center: [0, 1000] as [number, number],
    base: 1200,
    size: [1500, 3500, 1800] as [number, number, number],
    yaw: 3.1,
    density: 1,
    erosion: 0.3,
    seed: 11,
  };
  const cloud: Cloudscape = { development: 0.8, storminess: 0, highCloudCover: 0.1, background: 0.5 };
  const sample = (forms: (typeof form)[]) =>
    sampleEnvironmentSequence(
      {
        ...sequence,
        loop: false,
        interpolation: "linear",
        keyframes: [
          { time: 0, state: { ...state, cloudscape: { ...cloud, formations: [form] } } },
          { time: 10, state: { ...state, cloudscape: { ...cloud, formations: forms } } },
        ],
      },
      5,
    ).cloudscape ?? cloud;
  const moving = sample([{ ...form, center: [2000, 1000], yaw: -3.1 }]);
  expect(moving.formations?.length).toBe(1);
  expect(moving.formations?.[0].center).toEqual([1000, 1000]);
  expect(Math.abs(moving.formations?.[0].yaw ?? 0)).toBeCloseTo(Math.PI);
  expect(sample([]).formations?.[0].density).toBe(0.5);
  const changed = sample([{ ...form, id: "new", density: 0.8 }]);
  expect(changed.formations?.map((f) => f.density)).toEqual([0.5, 0.4]);
});

test("cloud lifecycle and independent deck controls interpolate continuously", () => {
  const form = {
    id: "growth",
    kind: "tower" as const,
    center: [0, 6000] as [number, number],
    base: 1200,
    size: [2000, 4000, 2000] as [number, number, number],
    yaw: 0,
    density: 1,
    erosion: 0.4,
    seed: 7,
  };
  const cloud = { development: 0.8, storminess: 0, highCloudCover: 0.3 };
  const sampled = sampleEnvironmentSequence(
    {
      ...sequence,
      loop: false,
      interpolation: "linear",
      keyframes: [
        {
          time: 0,
          state: {
            ...state,
            cloudscape: {
              ...cloud,
              midCloudCover: 0,
              midCloudHeight: 6800,
              midCloudYaw: 3.1,
              formations: [{ ...form, maturity: 0, shear: -1 }],
            },
          },
        },
        {
          time: 10,
          state: {
            ...state,
            cloudscape: {
              ...cloud,
              midCloudCover: 0.8,
              midCloudHeight: 7800,
              midCloudYaw: -3.1,
              formations: [{ ...form, maturity: 1, shear: 1 }],
            },
          },
        },
      ],
    },
    5,
  ).cloudscape;
  expect(sampled?.midCloudCover).toBeCloseTo(0.4);
  expect(sampled?.midCloudHeight).toBe(7300);
  expect(Math.abs(sampled?.midCloudYaw ?? 0)).toBeCloseTo(Math.PI);
  expect(sampled?.formations?.[0].maturity).toBeCloseTo(0.5);
  expect(sampled?.formations?.[0].shear).toBeCloseTo(0);
});
