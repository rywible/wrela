import { assemblySchema } from "./assembly";
import {
  constructionRandom,
  constructionRecipeSchema,
  createConstructionBuilder,
} from "./construction-recipe";
import type { Document, ObjectDefinition, WorldDefinition } from "./documents";
import type { Vec3 } from "./math";
import { alpineSurfaceHistory, type SurfaceHistory, sampleSurfaceHistory } from "./surface-history";
import { type WorldPathGeometry, worldPathPolyline } from "./world-path";

export type WorldConstructionPatch = {
  id: string;
  center: Vec3;
  radius: [number, number];
  seed: number;
  stones: number;
  shelter: number;
};
export const ALPINE_CONSTRUCTION_PATCHES: WorldConstructionPatch[] = [
  { id: "gate-contact", center: [-14.9, 0, 6.8], radius: [1.3, 1.1], seed: 171, stones: 22, shelter: 0.5 },
  { id: "gate-collapse", center: [-8.9, 0, 8.1], radius: [1.4, 0.85], seed: 193, stones: 24, shelter: 0.2 },
  { id: "approach-verge", center: [-7.4, 0, -9], radius: [0.7, 1.7], seed: 211, stones: 16, shelter: 0.3 },
  { id: "creek-deposition", center: [6.8, 0, -6], radius: [1, 1.4], seed: 239, stones: 20, shelter: 0.1 },
];

/** Signed horizontal clearance from the graded core; shoulders may carry low growth. */
export function worldPathClearance(
  position: Vec3,
  paths: readonly (WorldPathGeometry & { width: number })[],
): number {
  let clearance = Infinity;
  for (const path of paths) {
    const points = worldPathPolyline(path);
    for (let index = 1; index < points.length; index++) {
      const a = points[index - 1],
        b = points[index];
      const dx = b[0] - a[0],
        dz = b[2] - a[2];
      const t = Math.max(
        0,
        Math.min(
          1,
          ((position[0] - a[0]) * dx + (position[2] - a[2]) * dz) / Math.max(1e-12, dx * dx + dz * dz),
        ),
      );
      clearance = Math.min(
        clearance,
        Math.hypot(position[0] - a[0] - t * dx, position[2] - a[2] - t * dz) - path.width / 2,
      );
    }
  }
  return clearance;
}

/** A bounded collection of ordinary editable debris assemblies; generated IDs are patch-local. */
export function createWorldConstruction(
  options: {
    history?: SurfaceHistory;
    patches?: readonly WorldConstructionPatch[];
    paths?: readonly (WorldPathGeometry & { width: number })[];
  } = {},
): {
  documents: Document[];
  placements: WorldDefinition["instances"];
  evidence: { patch: string; parts: number; wetness: number; damage: number; debris: number }[];
} {
  const history = options.history ?? alpineSurfaceHistory();
  const documents: Document[] = [],
    placements: WorldDefinition["instances"] = [],
    evidence = [];
  for (const patch of options.patches ?? ALPINE_CONSTRUCTION_PATCHES) {
    const signals = sampleSurfaceHistory(history, {
      position: patch.center,
      normal: [0, 1, 0],
      contact: 1,
      shelter: patch.shelter,
    });
    const material = signals.wetness > 0.45 ? "alpine-lookdev-rock-dark" : "alpine-lookdev-rock";
    const builder = createConstructionBuilder(
      constructionRecipeSchema.parse({ seed: patch.seed, history, stoneMaterial: material }),
    );
    builder.rubble({
      id: `${patch.id}-fragment`,
      center: [0, -0.025, 0],
      radius: patch.radius,
      count: Math.min(40, Math.max(1, Math.round(patch.stones * (0.55 + signals.debris)))),
      size: 0.14 + signals.damage * 0.18,
      exclude: (position) =>
        worldPathClearance(
          [position[0] + patch.center[0], position[1], position[2] + patch.center[2]],
          options.paths ?? [],
        ) < 0.25,
    });
    if (!builder.parts.length) continue;
    const id = `alpine-debris-${patch.id}`;
    const object: ObjectDefinition = {
      id,
      name: `${patch.id.replaceAll("-", " ")} deposited stone`,
      kind: "object",
      schemaVersion: 1,
      dependencies: [material],
      material,
      collision: "none",
      assembly: assemblySchema.parse({ parts: builder.parts, grid: 0.01, clearances: [] }),
      field: {
        root: "debris-envelope",
        resolution: 24,
        bounds: {
          min: [-patch.radius[0] - 0.3, -0.2, -patch.radius[1] - 0.3],
          max: [patch.radius[0] + 0.3, 0.5, patch.radius[1] + 0.3],
        },
        nodes: [
          {
            id: "debris-envelope",
            name: "Deposited stone envelope",
            kind: "box",
            position: [0, 0.1, 0],
            rotation: [0, 0, 0],
            size: [patch.radius[0], 0.1, patch.radius[1]],
            radius: 0.1,
            blend: 0,
            children: [],
          },
        ],
      },
    };
    documents.push(object);
    placements.push({
      id,
      definition: id,
      position: [...patch.center],
      rotation: [0, 0, 0],
      scale: 1,
      grounding: { offset: -0.015 },
    });
    evidence.push({
      patch: patch.id,
      parts: builder.parts.length,
      wetness: signals.wetness,
      damage: signals.damage,
      debris: signals.debris,
    });
  }
  return { documents, placements, evidence };
}

export type PlantCommunityMember = {
  id: string;
  position: Vec3;
  yaw: number;
  scale: number;
  species: "grass" | "fern" | "shrub";
  history: ReturnType<typeof sampleSurfaceHistory>;
};
/** Stable candidates respond to moisture/shelter; exclusions preserve the authored walking core. */
export function samplePlantCommunity(options: {
  id: string;
  center: [number, number];
  radius: [number, number];
  seed: number;
  counts: { grass: number; fern: number; shrub: number };
  shelter?: number;
  history: SurfaceHistory;
  paths?: readonly (WorldPathGeometry & { width: number })[];
}): PlantCommunityMember[] {
  const result: PlantCommunityMember[] = [];
  for (const species of ["grass", "fern", "shrub"] as const)
    for (let index = 0; index < options.counts[species]; index++) {
      const id = `${options.id}-${species}-${String(index + 1).padStart(2, "0")}`;
      const theta = index * 2.399963 + constructionRandom(options.seed, id, 1) * 0.75;
      const radius = Math.sqrt((index + 0.35) / options.counts[species]);
      const position: Vec3 = [
        options.center[0] + Math.cos(theta) * radius * options.radius[0],
        0,
        options.center[1] + Math.sin(theta) * radius * options.radius[1],
      ];
      if (worldPathClearance(position, options.paths ?? []) < (species === "shrub" ? 0.8 : 0.22)) continue;
      const history = sampleSurfaceHistory(options.history, {
        position,
        normal: [0, 1, 0],
        contact: 1,
        shelter: options.shelter ?? 0.3,
      });
      const vigor =
        species === "fern"
          ? 0.68 + history.wetness * 0.65
          : species === "grass"
            ? 0.84 + history.contact * 0.18 - history.damage * 0.2
            : 0.95;
      result.push({
        id,
        position,
        yaw: constructionRandom(options.seed, id, 3) * Math.PI * 2,
        scale: (0.8 + constructionRandom(options.seed, id, 4) * 0.35) * vigor,
        species,
        history,
      });
    }
  return result;
}
