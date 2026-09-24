import type {
  Document,
  EnvironmentDefinition,
  EnvironmentState,
  LightingRigDefinition,
  WaterDefinition,
} from "@wrela/model";

/** Art-directed lighting and creek for the alpine ruin review scene.
 * Metres, scene-linear light colors, seconds; all controls remain editable source. */
export function createEnvironmentLookdev(): {
  documents: Document[];
  environment: string;
  lighting: string;
  water: string;
} {
  const dry: EnvironmentState = {
    sunElevation: 0.58,
    sunAzimuth: -1.15,
    turbidity: 1.8,
    fogDensity: 0.00015,
    cloudCover: 0.42,
    cloudscape: {
      development: 0.8,
      storminess: 0.04,
      highCloudCover: 0.32,
      midCloudCover: 0.18,
      midCloudHeight: 6800,
      midCloudYaw: -0.55,
      background: 0.31,
      formations: [
        {
          id: "alpine-tower",
          kind: "tower",
          center: [-8200, 16500],
          base: 1100,
          size: [4000, 5200, 3500],
          yaw: -0.25,
          density: 1,
          erosion: 0.45,
          seed: 31,
          maturity: 0.28,
          shear: -0.35,
        },
        {
          id: "alpine-bank",
          kind: "bank",
          center: [5500, 18000],
          base: 1050,
          size: [7200, 1900, 3200],
          yaw: -0.25,
          density: 0.9,
          erosion: 0.55,
          seed: 12,
          maturity: 0.72,
          shear: 0.4,
        },
        {
          id: "alpine-remnants",
          kind: "wisp",
          center: [-800, 21500],
          base: 2700,
          size: [3100, 1000, 1700],
          yaw: -0.4,
          density: 0.65,
          erosion: 0.72,
          maturity: 0.85,
          shear: 0.6,
          seed: 89,
        },
      ],
      front: { origin: [-5000, 0], direction: [1, 0.35], width: 4500, strength: 0.25 },
    },
    wind: [0.55, 0, 0.2],
    ambient: 1.45,
    sunIntensity: 3.5,
    wetness: 0,
    grade: { exposureCompensation: 0.7, tint: [1, 1, 1] },
  };
  const cloudy: EnvironmentState = {
    ...dry,
    sunElevation: 0.5,
    sunAzimuth: -1.05,
    // A wet cloud deck is not automatically a polluted aerosol column.
    turbidity: 2.5,
    fogDensity: 0.00065,
    cloudCover: 0.92,
    cloudscape: {
      ...dry.cloudscape,
      background: 1,
      development: 0.62,
      storminess: 0.7,
      highCloudCover: 0.38,
      midCloudCover: 0.52,
      front: { origin: [1500, 0], direction: [1, 0.35], width: 5000, strength: 0.9 },
    },
    wind: [1.3, 0, 0.45],
    ambient: 1.7,
    sunIntensity: 3.5,
    wetness: 0.65,
    grade: { exposureCompensation: 0.75, tint: [1, 1, 1] },
  };
  const clearing: EnvironmentState = {
    ...dry,
    sunElevation: 0.42,
    sunAzimuth: -0.95,
    turbidity: 2,
    fogDensity: 0.0003,
    cloudCover: 0.58,
    cloudscape: {
      ...dry.cloudscape,
      background: 0.5,
      development: 0.78,
      storminess: 0.16,
      highCloudCover: 0.3,
      midCloudCover: 0.34,
      front: { origin: [8500, 0], direction: [1, 0.35], width: 4200, strength: -0.65 },
    },
    wetness: 0.85,
    grade: { exposureCompensation: 0.7, tint: [1, 1, 1] },
  };
  const environment: EnvironmentDefinition = {
    id: "alpine-lookdev-sky",
    name: "Alpine afternoon · clear, clouded, clearing",
    kind: "environment",
    schemaVersion: 1,
    dependencies: [],
    model: "analytic-sky",
    sunElevation: dry.sunElevation,
    sunAzimuth: dry.sunAzimuth,
    turbidity: dry.turbidity,
    fogDensity: dry.fogDensity,
    cloudCover: dry.cloudCover,
    cloudscape: dry.cloudscape,
    skyColor: [0.14, 0.3, 0.49],
    horizonColor: [0.64, 0.7, 0.74],
    groundColor: [0.14, 0.17, 0.13],
    wind: dry.wind,
    sequence: {
      duration: 90,
      loop: true,
      interpolation: "smooth",
      keyframes: [
        { time: 0, state: dry },
        { time: 32, state: cloudy },
        { time: 64, state: clearing },
      ],
    },
  };
  const lighting: LightingRigDefinition = {
    id: "alpine-lookdev-lighting",
    name: "Alpine warm side light and cool sky",
    kind: "lighting",
    schemaVersion: 1,
    dependencies: [],
    ambient: dry.ambient,
    lights: [
      {
        id: "alpine-sun",
        type: "directional",
        position: [-20, 16, -10],
        color: [1, 0.97, 0.92],
        intensity: dry.sunIntensity,
      },
    ],
  };
  const water: WaterDefinition = {
    id: "alpine-lookdev-creek",
    name: "Alpine creek · gentle flow and broken bank foam",
    kind: "water",
    schemaVersion: 1,
    dependencies: [],
    level: -0.2,
    color: [0.13, 0.26, 0.22],
    roughness: 0.22,
    waves: [
      { amplitude: 0.022, wavelength: 2.8, speed: 0.18, direction: 1.4, phase: 0.4 },
      { amplitude: 0.009, wavelength: 0.9, speed: 0.12, direction: 0.65, phase: 1.7 },
    ],
    flow: {
      velocity: [0, 0.7],
      weatherResponse: {
        referenceWindSpeed: Math.hypot(dry.wind[0], dry.wind[2]),
        waveGain: 0.18,
        roughnessGain: 0.018,
      },
      river: {
        points: [
          { position: [8, -40], width: 3.5, depth: 0.6 },
          { position: [8, -28], width: 3.4, depth: 0.8 },
          { position: [3, -17], width: 3.6, depth: 0.65 },
          { position: [7, -8], width: 3.2, depth: 0.8 },
          { position: [4, 2], width: 3.6, depth: 0.7 },
          { position: [9, 14], width: 3.3, depth: 0.65 },
          { position: [6, 27], width: 3.6, depth: 0.6 },
          { position: [10, 40], width: 3.5, depth: 0.55 },
        ],
        shoreWidth: 0.24,
        foam: 0.14,
      },
    },
  };
  return {
    documents: [environment, lighting, water],
    environment: environment.id,
    lighting: lighting.id,
    water: water.id,
  };
}

/** Compact review cycle: authored weather runs on its own clock from the sun. */
export function createDayNightLookdev(): ReturnType<typeof createEnvironmentLookdev> {
  const result = createEnvironmentLookdev();
  const sky = result.documents.find((document) => document.id === result.environment);
  if (sky?.kind !== "environment" || !sky.sequence) throw new Error("Missing review sky");
  const [clear, storm, clearing] = sky.sequence.keyframes.map((key) => key.state);
  const goldenClearing: EnvironmentState = {
    ...clearing,
    cloudCover: 0.38,
    cloudscape: clearing.cloudscape ? { ...clearing.cloudscape, highCloudCover: 0.2 } : undefined,
  };
  sky.name = "Alpine day, moving storm, and moonlit night";
  sky.dayCycle = {
    duration: 80,
    startHour: 6.2,
    latitude: 43,
    declination: 10,
    northOffset: -0.75,
    moonIntensity: 0.1,
    nightExposure: 2.25,
  };
  sky.sequence = {
    duration: 80,
    loop: true,
    interpolation: "smooth",
    keyframes: [
      { time: 0, state: clear },
      { time: 20, state: clear },
      { time: 30, state: storm },
      { time: 42, state: goldenClearing },
      { time: 70, state: clear },
    ],
  };
  return result;
}
