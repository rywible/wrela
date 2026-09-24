import type { Document, ObjectDefinition, TerrainDefinition, Vec3, WorldDefinition } from "@wrela/model";
import { buildGeologicalRockField, defaultTerrainGeology } from "@wrela/model";

const envelope = (id: string, name: string) => ({
  id,
  name,
  schemaVersion: 1 as const,
  dependencies: [] as string[],
});
/** The same directed bedrock recipe powers authored caves and landscape outcrops. */
function outcrop(id: string, dimensions: Vec3, lean: number, seed: number): ObjectDefinition {
  const field = buildGeologicalRockField(
    dimensions,
    {
      seed: 719 + seed * 37,
      layers: 5,
      fracture: 0.72,
      bedding: { dip: lean, strike: -0.4 },
      fractureScale: Math.max(0.6, dimensions[1] / 4),
    },
    Math.max(...dimensions) < 5 ? 48 : 64,
  );
  // Outcrop placement marks the exposed base, with an embedded root below it.
  // The common recipe keeps river rocks grounded after river-bank resculpting.
  const burial = dimensions[1] * 0.2;
  field.nodes[field.nodes.length - 1].position[1] -= burial;
  field.bounds.min[1] -= burial;
  field.bounds.max[1] -= burial;
  return {
    ...envelope(id, "Fractured alpine bedrock"),
    kind: "object",
    dependencies: ["alpine-lookdev-rock"],
    material: "alpine-lookdev-rock",
    collision: "mesh",
    field,
  };
}
export function createTerrainLookdev(): {
  documents: Document[];
  terrain: string;
  formations: ObjectDefinition[];
  placements: WorldDefinition["instances"];
} {
  const geology = defaultTerrainGeology();
  geology.landforms = [
    {
      id: "west-shoulder",
      kind: "ridge",
      points: [
        [-34, -36],
        [-28, -22],
        [-29, -9],
        [-26, 8],
        [-26, 23],
        [-20, 39],
      ],
      width: 14,
      height: 5.7,
      falloff: 1.15,
    },
    {
      id: "east-bank",
      kind: "ridge",
      points: [
        [27, -38],
        [26, -24],
        [30, -12],
        [27, 5],
        [30, 21],
        [28, 39],
      ],
      width: 15,
      height: 4.8,
      falloff: 1.3,
    },
    {
      id: "rear-escarpment",
      kind: "ridge",
      points: [
        [-32, 32],
        [-19, 30],
        [-8, 36],
        [4, 32],
        [19, 35],
        [31, 30],
      ],
      width: 18,
      height: 5.3,
      falloff: 1.2,
    },
    {
      id: "west-moraine",
      kind: "ridge",
      points: [
        [-18, -31],
        [-16, -20],
        [-20, -12],
        [-18, -2],
        [-21, 9],
      ],
      width: 4.8,
      height: 1.1,
      falloff: 1.1,
    },
    {
      id: "creek-bed",
      kind: "drainage",
      points: [
        [8, -40],
        [8, -28],
        [3, -17],
        [7, -8],
        [4, 2],
        [9, 14],
        [6, 27],
        [10, 40],
      ],
      width: 8.5,
      height: 1.5,
      falloff: 1.15,
      profile: {
        kind: "river",
        bedFraction: 0.21,
        bankFraction: 0.46,
        shoulderDepth: 0.16,
        asymmetry: 0.24,
        variation: { seed: 71, amplitude: 0.13, wavelength: 22 },
      },
    },
  ];
  geology.erosion = { strength: 0.15, radius: 1, talusAngle: 42 };
  geology.strata = { thickness: 2.2, strength: 0.08 };
  geology.bankWetness = {
    drainageId: "creek-bed",
    waterHalfWidth: 1.8,
    fadeWidth: 3.5,
    darkening: 0.31,
  };
  geology.review = {
    route: [
      [-5, -22],
      [-5, -8],
      [-5, 4],
      [-8, 10],
    ],
    maxSlope: 30,
    eyeHeight: 1.7,
    clearance: 2,
  };
  geology.corridors = [
    {
      id: "alpine-footpath",
      halfWidth: 1.25,
      shoulder: 2.5,
      points: [
        [-5, 0.5, -23],
        [-5, 0.5, -8],
        [-5, 0.5, 4],
        [-8, 0.9, 10],
      ],
    },
  ];
  const terrain: TerrainDefinition = {
    ...envelope("alpine-lookdev-terrain", "Alpine creek basin"),
    kind: "terrain",
    material: "alpine-lookdev-ground",
    dependencies: ["alpine-lookdev-ground"],
    seed: 81,
    amplitude: 1.12,
    frequency: 0.038,
    octaves: 4,
    baseHeight: 0.45,
    geology,
    interventions: [
      { id: "ruin-terrace", kind: "flatten", center: [-12, 8], radius: 9, strength: 1, targetHeight: 1 },
      { id: "hero-clearing", kind: "flatten", center: [0, -4], radius: 5, strength: 1, targetHeight: 0.4 },
    ],
  };
  const specifications: { id: string; size: Vec3; at: Vec3; rotation: number; lean: number }[] = [
    { id: "west-wall", size: [9, 4.1, 6.5], at: [-21, 6.0, 21], rotation: 0.15, lean: -0.2 },
    { id: "rear-crown", size: [7.5, 4.8, 6], at: [-14, 6.1, 29], rotation: -0.1, lean: 0.24 },
    { id: "rear-shelf", size: [8, 3.3, 5], at: [-4, 4.55, 30], rotation: 0.1, lean: -0.17 },
    { id: "creek-ledge", size: [4.8, 1.6, 3.1], at: [10.3, -0.3, 8], rotation: -0.16, lean: 0.18 },
    { id: "near-bank-rock", size: [2.2, 1.4, 1.8], at: [8.8, -0.55, -10], rotation: 0.28, lean: -0.23 },
    { id: "left-foreground", size: [3.8, 2.6, 2.5], at: [-17, 0.9, -13], rotation: -0.3, lean: 0.16 },
    { id: "path-marker-rock", size: [1.7, 1.5, 1.4], at: [-8.8, 0.15, -7], rotation: 0.32, lean: 0.22 },
  ];
  const formations = specifications.map((specification, index) =>
    outcrop(`alpine-rock-${specification.id}`, specification.size, specification.lean, index),
  );
  const placements = specifications.map((specification, index) => ({
    id: `placed-${formations[index].id}`,
    definition: formations[index].id,
    position: specification.at,
    rotation: [0, specification.rotation, 0] as Vec3,
    scale: 1,
  }));
  return { documents: [terrain, ...formations], terrain: terrain.id, formations, placements };
}

/** Dedicated tool-output study: the rocks are built from geology controls by the
 * compiler, and the visible route is also the source for traversal diagnostics. */
export function createGeologyAuthoringStudy(
  variant: "alpine" | "cross-bedded" | "confined" = "alpine",
): TerrainDefinition {
  const geology = defaultTerrainGeology();
  geology.landforms = [
    {
      id: "back-ridge",
      kind: "ridge",
      points: [
        [-26, 17],
        [-10, 18],
        [7, 22],
        [28, 17],
      ],
      width: 9,
      height: 6,
      falloff: 1.15,
    },
    {
      id: "western-cliff",
      kind: "cliff",
      points: [
        [-18, 25],
        [-22, 5],
        [-24, -14],
      ],
      width: 7,
      height: 5.5,
      falloff: 0.85,
      profile: {
        kind: "cliff",
        faceFraction: 0.16,
        toeHeight: 0.18,
        crestFraction: 0.7,
        variation: { seed: 92, amplitude: 0.15, wavelength: 18 },
      },
    },
    {
      id: "runoff",
      kind: "drainage",
      points: [
        [21, 24],
        [25, 10],
        [24, -12],
      ],
      width: 5.8,
      height: 1.2,
      falloff: 1.1,
      profile: { kind: "river", bedFraction: 0.2, bankFraction: 0.48, shoulderDepth: 0.12, asymmetry: -0.3 },
    },
  ];
  geology.erosion = { strength: 0.3, radius: 0.8, talusAngle: 40 };
  geology.strata = { thickness: 1.6, strength: 0.25 };
  geology.formations = [
    {
      id: "layered-passage",
      kind: "cave",
      position: [-6, 0, 0],
      size: [14, 9, 10],
      opening: 0.62,
      resolution: 64,
      heading: 0.15,
      material: "alpine-lookdev-rock",
      rock: {
        seed: 491,
        layers: 6,
        fracture: 0.9,
        bedding: { dip: 0.24, strike: -0.35 },
        fractureScale: 1.7,
      },
    },
    {
      id: "shelter",
      kind: "overhang",
      position: [11, 0, 3],
      size: [11, 7, 8],
      opening: 0.58,
      resolution: 56,
      heading: -0.35,
      material: "alpine-lookdev-rock-dark",
      rock: {
        seed: 291,
        layers: 5,
        fracture: 0.85,
        bedding: { dip: 0.24, strike: -0.35 },
        fractureScale: 1.7,
      },
    },
  ];
  geology.corridors = [
    {
      id: "passage-trail",
      halfWidth: 1.4,
      shoulder: 2.8,
      points: [
        [-6, 0, -18],
        [-6, 0, -8],
        [-6, 0, 12],
        [-9, 1, 26],
      ],
    },
  ];
  geology.review = {
    route: [
      [-6, -18],
      [-6, -8],
      [-6, 12],
      [-9, 26],
    ],
    maxSlope: 30,
    eyeHeight: 1.7,
    clearance: 2,
    bodyRadius: 0.4,
  };
  if (variant !== "alpine") {
    const crossBedded = variant === "cross-bedded";
    for (const formation of geology.formations) {
      formation.rock = {
        seed: crossBedded ? 1709 : 8317,
        layers: crossBedded ? 4 : 7,
        fracture: crossBedded ? 0.8 : 0.55,
        bedding: { dip: crossBedded ? -0.48 : 0.12, strike: crossBedded ? 1.05 : -1.1 },
        fractureScale: crossBedded ? 2.4 : 0.9,
      };
      formation.size = [
        formation.size[0] * (crossBedded ? 1.12 : 0.78),
        formation.size[1] * (crossBedded ? 0.88 : 1.18),
        formation.size[2] * (crossBedded ? 1.2 : 0.86),
      ];
      formation.heading = crossBedded ? -0.12 : 0.08;
    }
    const river = geology.landforms.find((form) => form.kind === "drainage");
    if (river)
      river.profile = {
        kind: "river",
        bedFraction: crossBedded ? 0.3 : 0.12,
        bankFraction: crossBedded ? 0.5 : 0.3,
        shoulderDepth: 0.16,
        asymmetry: crossBedded ? 0.45 : -0.5,
        variation: { seed: crossBedded ? 1801 : 9901, amplitude: 0.2, wavelength: crossBedded ? 25 : 8 },
      };
  }
  return {
    ...envelope(
      variant === "alpine" ? "geology-study-terrain" : `geology-study-${variant}`,
      `Stratified passage and protected trail (${variant})`,
    ),
    kind: "terrain",
    material: "alpine-lookdev-ground",
    dependencies: ["alpine-lookdev-ground"],
    seed: 51,
    amplitude: 0.55,
    frequency: 0.04,
    octaves: 3,
    baseHeight: 0,
    geology,
    interventions: [],
  };
}
