import { developmentalStructure } from "@wrela/compiler/botanical-growth-structure";
import { botanicalStructure } from "@wrela/compiler/botanical-structure";
import {
  alpinePineLookdevDefinition,
  createForestEdge,
  forestEdgeCamera,
  referenceProject,
  shapedPineLookdevDefinition,
} from "@wrela/examples";
import {
  alpineConiferMaterials,
  botanicalDevelopmentSchema,
  botanicalPreset,
  createVegetationStand,
  type Document,
  type GrowthSpecies,
  type Project,
  paperBirchMaterials,
  type StageDefinition,
  type VegetationDefinition,
  type WorldDefinition,
} from "@wrela/model";
import type { LookdevStudy } from "./lookdev";

/** The specimen and mixed-age stand share source definitions, authoring semantics,
 * lighting and the production renderer. Captures deliberately exercise LOD and wind. */
export function vegetationStudies(
  options: {
    forest?: boolean;
    heldout?: boolean;
    architecture?: boolean;
    seed?: number;
    age?: number;
    development?: GrowthSpecies;
    steps?: number;
  } = {},
): LookdevStudy[] {
  if (options.forest)
    return [
      {
        project: createForestEdge(options.seed),
        subject: "forest-edge",
        stage: "neutral-stage",
        antialiasing: "msaa",
        frames: [
          ...[0, 6, 12, 18, 24].map((time) => ({
            id: `forest-walk-${time}`,
            camera: forestEdgeCamera(time),
            time,
          })),
          { id: "forest-overview", camera: { position: [18, 8, 22], target: [0, 3, -2], fov: 50 } },
        ],
      },
    ];
  const plant = options.architecture ? shapedPineLookdevDefinition() : alpinePineLookdevDefinition();
  if (!plant.botanical) throw new Error("Missing botanical lookdev source");
  if (options.seed !== undefined) plant.seed = options.seed;
  if (options.age !== undefined) plant.botanical.age = options.age;
  if (options.development) {
    if (options.development === "paper-birch") plant.botanical = botanicalPreset("birch");
    plant.botanical.development = botanicalDevelopmentSchema.parse({
      species: options.development,
      steps: options.steps ?? 16,
    });
    plant.name = options.development === "paper-birch" ? "Grown paper birch" : "Grown lodgepole pine";
  }
  plant.botanical.review.spacing = 4.8;
  plant.botanical.review.ageVariation = 0.5;
  const materials = alpineConiferMaterials();
  if (options.development === "paper-birch") {
    const birch = paperBirchMaterials();
    materials.bark = { ...birch.bark, id: materials.bark.id };
    materials.needles = { ...birch.leaves, id: materials.needles.id };
  }
  const population = createVegetationStand(plant);
  const project = referenceProject();
  const stage: StageDefinition = {
    id: "vegetation-review-stage",
    name: "Botanical daylight review",
    kind: "stage",
    schemaVersion: 1,
    dependencies: [],
    environment: "vegetation-review-sky",
    lighting: "vegetation-review-light",
    ground: true,
    exposure: 1.2,
    subjects: [],
    camera: { position: [8, 5, 14], target: [0, 3.7, 0], fov: 42 },
  };
  if (options.development) {
    const cameraScale = Math.max(0.12, developmentalStructure(plant).height / 7.4);
    stage.camera = {
      ...stage.camera,
      position: stage.camera.position.map((v) => v * cameraScale) as [number, number, number],
      target: stage.camera.target.map((v) => v * cameraScale) as [number, number, number],
    };
  }
  const world: WorldDefinition = {
    id: "vegetation-review-stand",
    name: "Pine community across seed and maturity",
    kind: "world",
    schemaVersion: 1,
    dependencies: [],
    generatorVersion: "wrela-world-1",
    terrain: "vegetation-review-ground",
    environment: stage.environment,
    lighting: stage.lighting,
    populations: [],
    instances: population.instances,
  };
  const documents: Document[] = [
    stage,
    plant,
    materials.needles,
    materials.bark,
    ...population.documents,
    world,
    {
      id: stage.environment,
      name: "Daylight",
      kind: "environment",
      schemaVersion: 1,
      dependencies: [],
      model: "analytic-sky",
      sunElevation: 0.65,
      sunAzimuth: 0.55,
      turbidity: 2,
      fogDensity: 0,
      skyColor: [0.3, 0.44, 0.62],
      horizonColor: [0.72, 0.77, 0.8],
      groundColor: [0.2, 0.21, 0.2],
      wind: [5, 0, 1],
    },
    {
      id: stage.lighting,
      name: "Soft daylight",
      kind: "lighting",
      schemaVersion: 1,
      dependencies: [],
      ambient: 1.4,
      lights: [
        { id: "key", type: "directional", position: [4, 8, 5], color: [1, 0.96, 0.9], intensity: 3.2 },
      ],
    },
    {
      id: world.terrain,
      name: "Neutral review ground",
      kind: "terrain",
      schemaVersion: 1,
      dependencies: [],
      seed: 1,
      amplitude: 0,
      baseHeight: -0.02,
      frequency: 0.02,
      octaves: 1,
      material: "vegetation-review-floor",
      interventions: [],
    },
    {
      id: "vegetation-review-floor",
      name: "Neutral gray ground",
      kind: "material",
      schemaVersion: 1,
      dependencies: [],
      color: [0.12, 0.13, 0.12],
      secondary: [0.12, 0.13, 0.12],
      roughness: 1,
      metallic: 0,
      pattern: "solid",
      scale: 1,
      normalStrength: 0,
    },
  ];
  const ids = new Set(documents.map((document) => document.id));
  const authored: Project = {
    ...project,
    documents: [...project.documents.filter((document) => !ids.has(document.id)), ...documents],
  };
  if (options.heldout) {
    return (["grass", "fern", "oak"] as const).map((species, index) => {
      const botanical = botanicalPreset(species);
      botanical.age = 0.72;
      const specimen: VegetationDefinition = {
        ...plant,
        id: `heldout-${species}`,
        name: `Held-out ${species}`,
        seed: 119 + index * 37,
        height: species === "oak" ? 4 : 0.9,
        radius: species === "oak" ? 1.6 : 0.38,
        branches: species === "oak" ? 8 : 5,
        botanical,
      };
      const camera =
        species === "oak"
          ? {
              position: [5, 3, 7] as [number, number, number],
              target: [0, 1.8, 0] as [number, number, number],
              fov: 42,
            }
          : {
              position: [1.2, 0.9, 1.8] as [number, number, number],
              target: [0, 0.4, 0] as [number, number, number],
              fov: 42,
            };
      return {
        project: { ...authored, documents: [...authored.documents, specimen] },
        subject: specimen.id,
        stage: stage.id,
        antialiasing: "msaa",
        frames: [{ id: `heldout-${species}`, camera }],
      };
    });
  }
  const branch = botanicalStructure(plant).branches.find((b) => b.id === "b16");
  const branchFrame = branch
    ? [
        {
          id: "plant-branch",
          sourcePrefix: `${plant.id}/${branch.id}`,
          camera: {
            position: [
              (branch.start[0] + branch.end[0]) / 2 + 1.7,
              branch.start[1] + 1.5,
              (branch.start[2] + branch.end[2]) / 2 + 2.6,
            ] as [number, number, number],
            target: [
              (branch.start[0] + branch.end[0]) / 2,
              (branch.start[1] + branch.end[1]) / 2,
              (branch.start[2] + branch.end[2]) / 2,
            ] as [number, number, number],
            fov: 40,
          },
        },
      ]
    : [];
  return [
    {
      project: authored,
      subject: plant.id,
      antialiasing: "msaa",
      stage: stage.id,
      frames: [
        ...branchFrame,
        { id: "plant-beauty", camera: stage.camera },
        { id: "plant-silhouette", camera: stage.camera, mode: "silhouette" },
        { id: "plant-detail", camera: { position: [1.4, 3.4, 2.3], target: [0, 3.1, 0], fov: 40 } },
        {
          id: "plant-albedo",
          camera: { position: [1.4, 3.4, 2.3], target: [0, 3.1, 0], fov: 40 },
          mode: "albedo",
        },
        {
          id: "plant-shading-normals",
          camera: { position: [1.4, 3.4, 2.3], target: [0, 3.1, 0], fov: 40 },
          mode: "shading-normals",
        },
        { id: "plant-backlit", camera: stage.camera, sunDirection: [-0.5, 0.22, -0.84] },
        { id: "plant-wind-a", camera: stage.camera, time: 0.6 },
        { id: "plant-wind-b", camera: stage.camera, time: 1.5 },
      ],
    },
    {
      project: authored,
      subject: world.id,
      antialiasing: "msaa",
      stage: stage.id,
      frames: [
        { id: "stand-near", camera: { position: [9, 2.4, 12], target: [0, 3, 0], fov: 48 } },
        { id: "stand-gameplay", camera: { position: [18, 3, 26], target: [0, 3, 0], fov: 48 } },
        // This is deliberately beyond the projected-size transition, including
        // the wind-expanded bounds and the selector's 15% hysteresis band.
        { id: "stand-far", camera: { position: [96, 12, 168], target: [0, 3, 0], fov: 48 } },
      ],
    },
  ];
}
