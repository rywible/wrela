import { generateTerrainPatch } from "@wrela/compiler";
import { defaultTerrainGeology, type TerrainDefinition } from "@wrela/model";

// Same maximum-source fixture used during geology authoring implementation.
// Cold and warm timings are reported separately; this is CPU patch generation.
const geology = defaultTerrainGeology();
geology.erosion.strength = 0.5;
geology.landforms = Array.from({ length: 24 }, (_, index) => ({
  id: `ridge-${index}`,
  kind: "ridge",
  points: Array.from({ length: 32 }, (_, point): [number, number] => [point * 3, index * 2]),
  width: 4,
  height: 2,
  falloff: 1,
}));
const withProfiles = process.argv.includes("--profiles");
if (withProfiles)
  for (const [index, landform] of geology.landforms.entries()) {
    landform.kind = index % 2 ? "drainage" : "cliff";
    const variation = { seed: index * 71, amplitude: 0.2, wavelength: 17 };
    landform.profile =
      index % 2
        ? {
            kind: "river",
            bedFraction: 0.2,
            bankFraction: 0.45,
            shoulderDepth: 0.16,
            asymmetry: 0.35,
            variation,
          }
        : { kind: "cliff", faceFraction: 0.12, toeHeight: 0.2, crestFraction: 0.7, variation };
  }
const withCorridors = process.argv.includes("--corridors");
if (withCorridors)
  geology.corridors = Array.from({ length: 16 }, (_, index) => ({
    id: `trail-${index}`,
    halfWidth: 1.5,
    shoulder: 3,
    points: Array.from({ length: 32 }, (_, point): [number, number, number] => [
      point * 3,
      point * 0.1,
      index * 4,
    ]),
  }));
const terrain: TerrainDefinition = {
  id: "bench",
  name: "Geology benchmark",
  kind: "terrain",
  schemaVersion: 1,
  dependencies: [],
  seed: 1,
  amplitude: 3,
  frequency: 0.02,
  octaves: 4,
  baseHeight: 0,
  material: "rock",
  interventions: [],
  geology,
};
const milliseconds: number[] = [];
for (let iteration = 0; iteration < 8; iteration++) {
  const start = performance.now();
  generateTerrainPatch(terrain, 0, 0, 32, 32);
  milliseconds.push(performance.now() - start);
}
console.log(
  JSON.stringify(
    {
      fixture: "24 landforms, 744 segments, erosion, 32 × 32 cells",
      corridors: withCorridors ? 16 : 0,
      profiles: withProfiles ? 24 : 0,
      milliseconds,
    },
    null,
    2,
  ),
);
