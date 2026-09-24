import { createEnvironmentLookdev } from "@wrela/examples/environment-lookdev";
import type { Camera } from "@wrela/model";
import { parseProject } from "@wrela/model";

export const waterStudyCameras: Record<string, Camera> = {
  "water-study-creek": { position: [9, 7, 13], target: [-1, 0, -2], fov: 49 },
  "water-study-surf": { position: [11, 3.8, 13], target: [0, 0.4, 1], fov: 53 },
  "water-study-ocean": { position: [0, 3.2, 0], target: [-18, 1, -45], fov: 56 },
};

export const waterLakeCamera: Camera = { position: [12, 9, 22], target: [1, 0, 9], fov: 48 };

/** Editable source fixtures also used by the standalone player and hardware review. */
export function createWaterLookdev(entry: "creek" | "ocean" | "surf" = "creek") {
  const lighting = createEnvironmentLookdev();
  const documents = lighting.documents.filter((document) => document.kind !== "water");
  const sky = documents.find((document) => document.kind === "environment");
  if (sky?.kind === "environment") {
    delete sky.sequence;
    sky.sunAzimuth = -2.6;
    sky.sunElevation = 0.48;
    sky.cloudCover = 0.32;
  }
  const envelope = { schemaVersion: 1, dependencies: [] };
  return parseProject({
    schemaVersion: 1,
    id: "water-study",
    name: "Wrela · Living water",
    entry: `water-study-${entry}`,
    documents: [
      ...documents,
      {
        ...envelope,
        kind: "water",
        id: "water-study-creek",
        name: "Creek into lake",
        level: 0,
        color: [0.085, 0.25, 0.22],
        roughness: 0.085,
        waves: [],
        spectrum: {
          seed: 23,
          windSpeed: 3,
          amplitude: 0.022,
          wavelength: 1.5,
          direction: 1.2,
          spread: 1,
          choppiness: 0,
        },
        optics: { absorption: [0.23, 0.075, 0.05], caustics: 0.6, foam: 0.8 },
        flow: {
          velocity: [0, 0.45],
          river: {
            shoreWidth: 0.4,
            foam: 0.5,
            points: [
              { position: [-5, -22], width: 3.3, depth: 0.65, level: 0.9 },
              { position: [-3, -14], width: 3.8, depth: 0.65, level: 0.6 },
              { position: [0, -8], width: 4.3, depth: 0.8, level: 0.3 },
              { position: [-2, -2], width: 4.4, depth: 1, level: 0.08 },
              { position: [0, 5], width: 6.2, depth: 1.3, level: 0 },
            ],
          },
        },
        domain: {
          min: [-14, -25],
          size: [28, 48],
          resolution: 128,
          basins: [{ center: [1, 11], radii: [10, 10], depth: 2.1 }],
          obstacles: [
            { center: [-2, -13], radius: 0.8, height: 1.1 },
            { center: [-0.8, -6.4], radius: 1.2, height: 1.3 },
            { center: [1.4, 1.5], radius: 1.7, height: 1.65 },
            { center: [5.8, 13], radius: 2, height: 2.1 },
          ],
          sources: [
            { position: [-5, -21], radius: 1.1, rate: 0.28, velocity: [0, 0.8] },
            { position: [2, 16], radius: 2, rate: -0.28 },
          ],
          bankHeight: 1.1,
          bankWidth: 3,
          friction: 0.2,
        },
      },
      {
        ...envelope,
        kind: "water",
        id: "water-study-ocean",
        name: "Open ocean",
        level: 0,
        color: [0.055, 0.19, 0.23],
        roughness: 0.09,
        waves: [],
        spectrum: {
          seed: 17,
          windSpeed: 8,
          amplitude: 0.48,
          wavelength: 14,
          direction: 0.8,
          spread: 1.1,
          choppiness: 0.6,
        },
        optics: { absorption: [0.22, 0.075, 0.035], caustics: 0, foam: 0.75 },
      },
      {
        ...envelope,
        kind: "water",
        id: "water-study-surf",
        name: "Breaking beach",
        level: 0,
        color: [0.055, 0.19, 0.23],
        roughness: 0.09,
        waves: [],
        spectrum: {
          seed: 41,
          windSpeed: 5,
          amplitude: 0.12,
          wavelength: 8,
          direction: 1.57,
          spread: 0.5,
          choppiness: 0,
        },
        optics: {
          absorption: [0.18, 0.06, 0.035],
          scattering: [0.016, 0.032, 0.035],
          anisotropy: 0.45,
          foam: 1.2,
          foamLifetime: 6,
          caustics: 0.3,
        },
        effects: [
          {
            id: "front-a",
            kind: "breaker",
            start: [-8, 2],
            end: [5, 2],
            height: 1.05,
            width: 5,
            period: 6.3,
            phase: 0.35,
          },
          {
            id: "front-b",
            kind: "breaker",
            start: [-4, -3],
            end: [10, -2],
            height: 0.75,
            width: 4,
            period: 7.7,
            phase: 0.73,
          },
        ],
        domain: {
          min: [-18, -18],
          size: [36, 36],
          resolution: 64,
          renderResolution: 256,
          bedDetail: 0.02,
          basins: [{ center: [0, -16], radii: [30, 26], depth: 3.2 }],
          bankHeight: 1.2,
          bankWidth: 6,
          simulate: false,
        },
      },
      {
        ...envelope,
        id: "neutral-stage",
        name: "Water daylight",
        kind: "stage",
        ground: false,
        exposure: 1,
        environment: lighting.environment,
        lighting: lighting.lighting,
        subjects: [],
        camera: waterStudyCameras[`water-study-${entry}`],
      },
    ],
  });
}
