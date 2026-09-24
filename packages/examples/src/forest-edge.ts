import { shapedPineLookdevDefinition } from "@wrela/examples/alpine-lookdev";
import { referenceProject } from "@wrela/examples/fixtures";
import type { Camera, Document, Project, VegetationDefinition, WorldDefinition } from "@wrela/model";
import {
  alpineConiferMaterials,
  botanicalPreset,
  createSurfaceAppearance,
  random01,
  surfaceLayerSchema,
  type Vec3,
} from "@wrela/model";

export const forestTrailX = (z: number) => Math.sin(z * 0.18) * 1.2;
/** Fixed eye-height traversal exposes ground attachment, silhouette changes and
 * disocclusions. The authored clearing remains at least 2.4 metres wide. */
export function forestEdgeCamera(seconds = 0): Camera {
  const z = 15 - Math.min(1, Math.max(0, seconds / 24)) * 24;
  return { position: [forestTrailX(z), 1.65, z], target: [forestTrailX(z - 5), 2.15, z - 5], fov: 60 };
}
export function createForestEdge(seed = 73): Project {
  const project = referenceProject(),
    base = (id: string, name: string) => ({ id, name, schemaVersion: 1 as const, dependencies: [] });
  const materials = alpineConiferMaterials(),
    documents: Document[] = [materials.bark, materials.needles];
  const instances: WorldDefinition["instances"] = [];
  const random = (i: number, stream: number) => random01(seed + i * 7919 + stream * 104729);
  const trees: Vec3[] = [
    [-4, 0, 9],
    [5, 0, 7],
    [-4.7, 0, 2],
    [4.2, 0, 0],
    [-5, 0, -5],
    [4.6, 0, -7],
    [-8, 0, -10],
    [8, 0, -12],
    [-3.8, 0, -14],
    [1.8, 0, -17],
    [10, 0, 1],
    [-10, 0, 4],
  ];
  for (let i = 0; i < 4; i++) {
    const tree = shapedPineLookdevDefinition();
    tree.id = `forest-pine-${i}`;
    tree.name = `Forest pine ${i + 1}`;
    tree.seed = seed + i * 7919;
    tree.height = [8, 6.8, 4.3, 9][i];
    tree.radius = [2.25, 2, 1.25, 2.5][i];
    if (!tree.botanical) throw Error("Missing pine recipe");
    tree.botanical.age = [0.9, 0.77, 0.55, 0.94][i];
    documents.push(tree);
  }
  trees.forEach((position, i) => {
    instances.push({
      id: `forest-tree-${i}`,
      definition: `forest-pine-${i % 4}`,
      position,
      rotation: [0, random(i, 0) * Math.PI * 2, 0],
      scale: 0.9 + random(i, 1) * 0.2,
    });
  });
  const rock = structuredClone(project.documents.find((d) => d.id === "river-stone"));
  if (rock?.kind === "object") {
    rock.id = "forest-stone";
    rock.material = "forest-stone-material";
    documents.push(rock);
    documents.push({
      ...base("forest-stone-material", "Weathered forest stone"),
      kind: "material",
      color: [0.1, 0.093, 0.074],
      secondary: [0.17, 0.16, 0.13],
      roughness: 0.98,
      metallic: 0,
      pattern: "noise",
      scale: 14,
      normalStrength: 0.14,
    });
  }
  const understory = {
    ...materials.needles,
    id: "forest-understory",
    name: "Understory leaves",
    color: [0.063, 0.11, 0.032] as Vec3,
    secondary: [0.13, 0.18, 0.053] as Vec3,
  };
  documents.push(understory);
  for (const species of ["grass", "fern"] as const) {
    const source: VegetationDefinition = {
      ...shapedPineLookdevDefinition(),
      id: `forest-${species}`,
      name: species === "grass" ? "Tufted forest grass" : "Shade fern",
      seed: seed + 103,
      height: species === "grass" ? 0.5 : 0.55,
      radius: species === "grass" ? 0.2 : 0.38,
      branches: 6,
      material: understory.id,
      trunkMaterial: understory.id,
      windResponse: 0.4,
      botanical: botanicalPreset(species),
    };
    if (species === "fern" && source.botanical) {
      source.botanical.canopy.clusters = 1;
      source.botanical.canopy.leavesPerCluster = 12;
      source.botanical.canopy.leafLength = 0.13;
      source.botanical.canopy.leafWidth = 0.018;
    }
    documents.push(source);
  }
  // Flat ground is deliberate: slope=0 is known exactly. Shade comes from
  // distance to mature crowns; a smooth moisture field clusters the ferns.
  // The deterministic placement is ordinary editable world instances.
  for (let i = 0; i < 720 && instances.length < 230; i++) {
    const x = (random(i, 2) - 0.5) * 25,
      z = (random(i, 3) - 0.5) * 34;
    if (Math.abs(x - forestTrailX(z)) < 1.4) continue;
    const nearest = Math.min(...trees.map((t) => Math.hypot(x - t[0], z - t[2])));
    if (nearest < 0.55 || nearest > 5.2) continue;
    const shade = Math.exp((-nearest * nearest) / 12),
      moisture = 0.5 + 0.5 * Math.sin(x * 0.35 + z * 0.13);
    const species = shade * moisture > 0.48 && i % 4 === 0 ? "fern" : "grass";
    if (random(i, 4) > 0.3 + shade * 0.7) continue;
    instances.push({
      id: `forest-understory-${i}`,
      definition: `forest-${species}`,
      position: [x, 0, z],
      rotation: [0, random(i, 5) * Math.PI * 2, 0],
      scale: 0.6 + random(i, 6) * 0.9,
    });
  }
  // A few anchored stones break the floor plane without filling the clearing.
  const stones: Vec3[] = [
    [-3.1, 0, 10],
    [3.3, 0, 4],
    [-3.7, 0, -2],
    [6, 0, -9],
    [-7, 0, 1],
    [2.9, 0, -12],
  ];
  stones.forEach((position, i) => {
    instances.push({
      id: `forest-stone-${i}`,
      definition: "forest-stone",
      position,
      rotation: [0, random(i, 7) * 6.28, 0],
      scale: 0.16 + random(i, 8) * 0.25,
    });
  });
  const soil = createSurfaceAppearance();
  soil.layers = [
    surfaceLayerSchema.parse({
      id: "litter",
      name: "Dry needle litter",
      color: [0.085, 0.056, 0.028],
      coverage: 0.65,
      relief: 0.002,
      mask: { kind: "noise", scale: 12, threshold: 0.42, softness: 0.2 },
    }),
    surfaceLayerSchema.parse({
      id: "moss",
      name: "Broken moss patches",
      color: [0.057, 0.071, 0.027],
      coverage: 0.6,
      relief: 0.001,
      mask: { kind: "noise", scale: 0.5, threshold: 0.6, softness: 0.18 },
    }),
  ];
  documents.push(
    {
      ...base("forest-soil", "Forest floor"),
      kind: "material",
      color: [0.064, 0.047, 0.03],
      secondary: [0.13, 0.105, 0.063],
      roughness: 0.98,
      metallic: 0,
      pattern: "noise",
      scale: 7,
      normalStrength: 0.15,
      appearance: soil,
      domain: "world",
    },
    {
      ...base("forest-ground", "Forest clearing"),
      kind: "terrain",
      seed,
      amplitude: 0,
      baseHeight: -0.01,
      frequency: 0.02,
      octaves: 1,
      material: "forest-soil",
      interventions: [],
    },
    {
      ...base("forest-sky", "Forest daylight"),
      kind: "environment",
      model: "analytic-sky",
      sunElevation: 0.65,
      sunAzimuth: -0.5,
      turbidity: 2.2,
      fogDensity: 0.003,
      skyColor: [0.3, 0.44, 0.62],
      horizonColor: [0.72, 0.77, 0.8],
      groundColor: [0.18, 0.16, 0.11],
      wind: [4, 0, 1],
    },
    {
      ...base("forest-light", "Forest daylight"),
      kind: "lighting",
      ambient: 1.0,
      lights: [
        { id: "sun", type: "directional", position: [-4, 8, 5], color: [1, 0.96, 0.9], intensity: 3.2 },
      ],
    },
    {
      ...base("forest-edge", "Pine forest edge"),
      kind: "world",
      generatorVersion: "wrela-world-1",
      terrain: "forest-ground",
      environment: "forest-sky",
      lighting: "forest-light",
      populations: [],
      instances,
    },
  );
  const stage = project.documents.find((d) => d.id === "neutral-stage");
  if (stage?.kind === "stage") {
    stage.environment = "forest-sky";
    stage.lighting = "forest-light";
    stage.camera = forestEdgeCamera();
    stage.exposure = 1.2;
  }
  const ids = new Set(documents.map((d) => d.id));
  return {
    ...project,
    name: "Pine forest edge",
    entry: "forest-edge",
    documents: [...project.documents.filter((d) => !ids.has(d.id)), ...documents],
  };
}
