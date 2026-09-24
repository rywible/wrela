import { alpinePineLookdevDefinition } from "@wrela/examples/alpine-lookdev";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import type { Camera } from "@wrela/model";
import { alpineConiferMaterials, parseProject } from "@wrela/model";
export const waterValleyCameras: Record<string, Camera> = {
  bank: { position: [8, 4.5, 11], target: [-1, 0.2, -4], fov: 54 },
  pool: { position: [7, 1.2, 17], target: [-2, 0.3, 4], fov: 52 },
  contact: { position: [3, 1.3, 3], target: [0, 0, -3], fov: 49 },
};
/** One composed environment using the ordinary world, botanical and water sources. */
export function createWaterValley() {
  const project = createWaterLookdev();
  const creek = project.documents.find((d) => d.id === "water-study-creek");
  if (creek?.kind !== "water" || !creek.domain) throw Error("Missing creek");
  creek.domain.resolution = 64;
  creek.domain.renderResolution = 256;
  creek.domain.bedDetail = 0.035;
  creek.domain.obstacles.forEach((o, i) => {
    o.aspect = [1.3, 0.7, 1.4, 0.85][i];
    o.yaw = i * 1.7;
  });
  creek.optics = { ...creek.optics!, scattering: [0.008, 0.018, 0.022], anisotropy: 0.35, foamLifetime: 5 };
  const pine = alpinePineLookdevDefinition();
  pine.height = 6.5;
  pine.radius = 1.7;
  pine.branches = 32;
  const materials = alpineConiferMaterials();
  const sky = project.documents.find((d) => d.kind === "environment")!;
  const stage = project.documents.find((d) => d.kind === "stage");
  if (stage?.kind !== "stage") throw Error("Missing stage");
  stage.camera = waterValleyCameras.bank;
  const envelope = { schemaVersion: 1, dependencies: [] };
  const trees = [
    [-9, -17, 1.1],
    [-7, -9, 0.8],
    [-8, -1, 1.2],
    [-9, 7, 0.9],
    [-9, 16, 1.1],
    [10, 14, 1.2],
    [10, 6, 0.9],
    [7, -4, 1.1],
    [5, -15, 0.8],
    [7, -22, 1.1],
  ];
  return parseProject({
    ...project,
    entry: "water-study-valley",
    documents: [
      ...project.documents,
      pine,
      materials.needles,
      materials.bark,
      {
        ...envelope,
        kind: "material",
        id: "water-bank-earth",
        name: "Damp gravel and earth",
        color: [0.12, 0.14, 0.1],
        secondary: [0.24, 0.21, 0.15],
        roughness: 0.9,
        metallic: 0,
        pattern: "noise",
        scale: 1.5,
        normalStrength: 0.25,
      },
      {
        ...envelope,
        kind: "terrain",
        id: "water-valley-ground",
        name: "Valley surround",
        seed: 87,
        amplitude: 4,
        frequency: 0.012,
        octaves: 3,
        baseHeight: 1.1,
        material: "water-bank-earth",
        interventions: [
          {
            id: "water-bed-clearance",
            kind: "flatten",
            center: [0, 0],
            radius: 34,
            strength: 1,
            targetHeight: -4,
          },
        ],
      },
      {
        ...envelope,
        kind: "world",
        id: "water-study-valley",
        name: "Creek and quiet pool",
        generatorVersion: "wrela-world-1",
        terrain: "water-valley-ground",
        environment: sky.id,
        lighting: stage.lighting,
        water: creek.id,
        populations: [],
        instances: trees.map(([x, z, scale], i) => ({
          id: `bank-pine-${i}`,
          definition: pine.id,
          position: [x, 1.08, z],
          rotation: [0, i * 0.73, 0],
          scale,
        })),
      },
    ],
  });
}
