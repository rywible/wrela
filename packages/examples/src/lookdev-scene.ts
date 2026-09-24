import { alpinePineLookdevDefinition } from "@wrela/examples/alpine-lookdev";
import { createArchitectureLookdev } from "@wrela/examples/architecture-lookdev";
import { createCharacterLookdev } from "@wrela/examples/character-lookdev";
import { createEnvironmentLookdev } from "@wrela/examples/environment-lookdev";
import { referenceProject } from "@wrela/examples/fixtures";
import { createLookdevMaterials } from "@wrela/examples/material-lookdev";
import { createTerrainLookdev } from "@wrela/examples/terrain-lookdev";
import { createWorldPathLookdev } from "@wrela/examples/world-path-lookdev";
import type {
  Camera,
  Document,
  Project,
  StageDefinition,
  Vec3,
  VegetationDefinition,
  WorldDefinition,
} from "@wrela/model";
import {
  alpineSurfaceHistory,
  createWorldConstruction,
  emptyWorldComposition,
  type SurfaceHistory,
} from "@wrela/model";
import { createAlpineHabitat } from "./alpine-habitat";

/** Intentional composition: sentinel at a creek crossing below an abandoned gate.
 * The warm stone gateway is the middle-distance landmark; the creature owns the foreground.
 * Separate source avoids changing existing gameplay and rendering benchmark fixtures. */
export const ALPINE_LOOKDEV_CAMERAS: { id: string; camera: Camera }[] = [
  { id: "near", camera: { position: [4.5, 2.4, -9.5], target: [0, 1.65, -4], fov: 43 } },
  { id: "gameplay", camera: { position: [-5, 2.2, -15], target: [-3.5, 1.8, 3], fov: 53 } },
  { id: "landscape", camera: { position: [14, 7, -24], target: [-6, 2.6, 5], fov: 48 } },
];
export const ALPINE_LOOKDEV_DETAIL_CAMERAS: { id: string; camera: Camera }[] = [
  { id: "gateway", camera: { position: [-7, 3.5, 0], target: [-12, 2.6, 8], fov: 46 } },
  { id: "creek", camera: { position: [7.5, 1.2, -14], target: [4.1, 0, -4], fov: 52 } },
];
export const ALPINE_LOOKDEV_TRAVERSAL: Vec3[] = [
  [-5, 0.5, -22],
  [-6.2, 0.5, -14],
  [-5.8, 0.5, -8],
  [-5.5, 0.5, 2],
  [-8.5, 0.7, 4.5],
  [-12, 1, 6.5],
];

/** Age and spacing produce several overlapping crowns along the route, with a
 * clear sightline to the gate. Terrain grounding is resolved after path grading. */
const treeLayout: {
  at: [number, number];
  scale: number;
  yaw: number;
  age: "mature" | "young" | "sapling";
}[] = [
  { at: [-11, -20], scale: 0.94, yaw: 0.5, age: "mature" },
  { at: [-11.5, -12], scale: 0.88, yaw: 1.8, age: "young" },
  { at: [-17, -7], scale: 1.05, yaw: 3.1, age: "mature" },
  { at: [-19, 1], scale: 0.91, yaw: 2.1, age: "mature" },
  { at: [-21, 9], scale: 1.12, yaw: 0.8, age: "mature" },
  { at: [-23, 21], scale: 0.82, yaw: 4.3, age: "young" },
  { at: [-7, 20], scale: 1.1, yaw: 2.6, age: "mature" },
  { at: [0, 21], scale: 0.95, yaw: 0.2, age: "mature" },
  { at: [13, 17], scale: 1.07, yaw: 5.4, age: "mature" },
  { at: [19, 18], scale: 0.88, yaw: 4.1, age: "young" },
  { at: [17, 9], scale: 1.12, yaw: 3.4, age: "mature" },
  { at: [12, 4], scale: 0.9, yaw: 0.9, age: "young" },
  { at: [15, -3], scale: 1.02, yaw: 2.7, age: "mature" },
  { at: [12, -12], scale: 1.05, yaw: 1.5, age: "mature" },
  { at: [18, -19], scale: 0.93, yaw: 4.8, age: "young" },
  { at: [-15, -3], scale: 1.02, yaw: 2.3, age: "sapling" },
  { at: [-9, 15], scale: 0.94, yaw: 4.1, age: "sapling" },
  { at: [9, -5], scale: 1.12, yaw: 0.2, age: "sapling" },
  { at: [15, 12], scale: 0.91, yaw: 3.8, age: "sapling" },
  { at: [-20, 7], scale: 0.84, yaw: 5.2, age: "young" },
];
function coniferVariants(): VegetationDefinition[] {
  const base = alpinePineLookdevDefinition();
  return (["mature", "young", "sapling"] as const).map((age, index) => {
    const definition = structuredClone(base);
    definition.id = `alpine-lookdev-pine-${age}`;
    definition.name = `Alpine grove ${age} pine`;
    definition.seed = [73, 193, 311][index];
    definition.material = "alpine-lookdev-needles";
    definition.trunkMaterial = "alpine-lookdev-bark";
    definition.dependencies = [definition.material, definition.trunkMaterial];
    definition.height = age === "mature" ? 9.5 : 8;
    definition.radius = age === "mature" ? 2.5 : 2.25;
    definition.branches = age === "mature" ? 60 : age === "young" ? 42 : 24;
    if (definition.botanical) definition.botanical.age = [0.95, 0.64, 0.35][index];
    return definition;
  });
}

export function createAlpineLookdevProject(options: { history?: SurfaceHistory } = {}): Project {
  const history = options.history ?? alpineSurfaceHistory();
  const base = referenceProject(),
    terrain = createTerrainLookdev(),
    architecture = createArchitectureLookdev({ history }),
    creature = createCharacterLookdev(),
    environment = createEnvironmentLookdev(),
    trail = createWorldPathLookdev({ history });
  const composition = emptyWorldComposition();
  composition.assemblies.push({
    id: "ruined-gateway",
    name: "Old mountain gateway",
    members: [{ id: "gateway", definition: architecture.object, position: [0, 0, 0], yaw: 0, scale: 1 }],
  });
  composition.placements.push({
    id: "ruin-on-terrace",
    assembly: "ruined-gateway",
    position: [-12, 1, 8],
    yaw: -0.18,
    scale: 1,
    grounding: { offset: 0 },
  });
  composition.paths.push(
    {
      id: "approach",
      kind: "path",
      points: [
        [-5, 0.5, -22],
        [-6.2, 0.5, -14],
        [-5.8, 0.5, -8],
        [-4.6, 0.5, -3],
        [-5.5, 0.5, 2],
      ],
      cornerRadius: 1.5,
      width: 2.4,
      shoulder: 0.8,
      flatten: true,
      spacing: 1.1,
      definition: trail.object,
      maxGrade: 0.3,
    },
    {
      id: "gate-branch",
      kind: "path",
      points: [
        [-5.5, 0.5, 2],
        [-8.5, 0.7, 4.5],
        [-12, 1, 6.5],
      ],
      cornerRadius: 1.5,
      width: 2,
      shoulder: 0.7,
      flatten: true,
      spacing: 1.1,
      definition: trail.object,
      maxGrade: 0.3,
    },
  );
  const habitat = createAlpineHabitat({ history, paths: composition.paths });
  const construction = createWorldConstruction({ history, paths: composition.paths });
  composition.spaces.push({
    id: "sentinel-clearing",
    kind: "encounter",
    center: [0, 0.4, -4],
    radius: 5,
    clearPopulation: true,
    members: [],
  });
  composition.spaces.push({
    id: "old-mountain-gate",
    kind: "landmark",
    center: [-12, 1, 8],
    radius: 3,
    clearPopulation: true,
    members: [],
  });
  composition.streaming.push({
    id: "grove",
    center: [-4, 0, 2],
    radius: 48,
    preloadDistance: 40,
    priority: 3,
    collision: true,
  });
  composition.review = {
    entryPath: "approach",
    actorRadius: 0.35,
    actorHeight: 1.8,
    maxStepHeight: 0.3,
    sightline: { from: [-5, 2.2, -15], to: [0, 2.1, -4] },
  };
  const world: WorldDefinition = {
    id: "alpine-lookdev-world",
    name: "Sentinel at the old mountain gate",
    kind: "world",
    schemaVersion: 1,
    dependencies: [],
    generatorVersion: "wrela-world-1",
    terrain: terrain.terrain,
    environment: environment.environment,
    lighting: environment.lighting,
    water: environment.water,
    populations: [],
    composition,
    instances: [
      {
        id: "sentinel",
        definition: creature.character,
        position: [0, 0, -4],
        rotation: [0, 2.2, 0],
        scale: 1,
        grounding: { offset: 0 },
      },
      ...terrain.placements,
      ...treeLayout.map((tree, index) => ({
        id: `grove-pine-${String(index + 1).padStart(2, "0")}`,
        definition: `alpine-lookdev-pine-${tree.age}`,
        position: [tree.at[0], 0, tree.at[1]] as Vec3,
        rotation: [0, tree.yaw, 0] as Vec3,
        scale: tree.scale,
        grounding: { offset: 0 },
      })),
      ...habitat.placements,
      ...construction.placements,
    ],
  };
  const stage: StageDefinition = {
    id: "alpine-lookdev-stage",
    name: "Alpine grove review",
    kind: "stage",
    schemaVersion: 1,
    dependencies: [],
    environment: environment.environment,
    lighting: environment.lighting,
    ground: false,
    exposure: 1,
    camera: ALPINE_LOOKDEV_CAMERAS[2].camera,
    subjects: [world.id],
  };
  const documents = new Map<string, Document>();
  // Bundles own their source definitions. Shared final material overrides supply one palette.
  for (const document of [
    ...base.documents,
    ...creature.documents,
    ...terrain.documents,
    ...architecture.documents,
    ...environment.documents,
    ...coniferVariants(),
    ...habitat.documents,
    ...construction.documents,
    ...createLookdevMaterials(),
    ...trail.documents,
    world,
  ]) {
    if (document.kind !== "stage") documents.set(document.id, document);
  }
  return {
    schemaVersion: 1,
    id: "alpine-lookdev-project",
    name: "Alpine grove · visual development",
    entry: world.id,
    documents: [stage, ...documents.values()],
  };
}
