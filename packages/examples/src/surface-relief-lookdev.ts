import { referenceProject } from "@wrela/examples/fixtures";
import type {
  AssemblyPart,
  Camera,
  FieldNode,
  MaterialDefinition,
  ObjectDefinition,
  Project,
  StageDefinition,
} from "@wrela/model";
import { createSurfaceAppearance, surfaceReliefSchema } from "@wrela/model";

export const SURFACE_RELIEF_CAMERAS: Record<"near" | "bark" | "stone" | "far", Camera> = {
  near: { position: [2.1, 1.65, 4.8], target: [0, 0.83, 0], fov: 36 },
  bark: { position: [-0.6, 0.92, 1.72], target: [-0.73, 0.86, 0], fov: 36 },
  stone: { position: [1.47, 0.89, 1.82], target: [0.72, 0.64, 0], fov: 36 },
  far: { position: [46.5, 18.44, 147], target: [0, 0.83, 0], fov: 36 },
};
const envelope = (id: string, name: string) => ({
  id,
  name,
  schemaVersion: 1 as const,
  dependencies: [] as string[],
});

/** Metre-scale source specimens: a swept bark column and an angular clipped stone, never shader balls. */
export function createSurfaceReliefLookdevProject(displaced = true, grazing = false): Project {
  const base = referenceProject();
  const environment = structuredClone(base.documents.find((entry) => entry.kind === "environment"));
  const lighting = structuredClone(base.documents.find((entry) => entry.kind === "lighting"));
  if (!environment || environment.kind !== "environment" || !lighting || lighting.kind !== "lighting")
    throw new Error("Missing source review environment");
  environment.id = "surface-relief-environment";
  environment.fogDensity = 0;
  environment.sunElevation = grazing ? 0.16 : 0.65;
  environment.sunAzimuth = grazing ? -1.12 : -0.65;
  environment.wind = [0, 0, 0];
  lighting.id = "surface-relief-lighting";
  lighting.ambient = grazing ? 0.22 : 0.45;
  lighting.lights = [
    { id: "review-sun", type: "directional", color: [1, 0.94, 0.84], intensity: 3.5, position: [2, 5, 4] },
  ];
  const materials: MaterialDefinition[] = (["bark", "stone"] as const).map((kind) => {
    const appearance = createSurfaceAppearance();
    // Identical flat color recipes isolate physical geometry in both A/B variants.
    appearance.detail = { kind: "none", scale: 1, strength: 0 };
    if (displaced)
      appearance.relief = surfaceReliefSchema.parse({
        kind,
        amplitude: kind === "bark" ? 0.022 : 0.014,
        scale: kind === "bark" ? 0.07 : 0.1,
        targetEdgeLength: 0.022,
        seed: kind === "bark" ? 37 : 19,
      });
    const color: [number, number, number] = kind === "bark" ? [0.15, 0.071, 0.027] : [0.24, 0.225, 0.195];
    return {
      ...envelope(
        `surface-relief-${kind}`,
        kind === "bark" ? "Physical bark grooves" : "Physical stone chips",
      ),
      kind: "material",
      color,
      secondary: [...color],
      pattern: "solid",
      roughness: kind === "bark" ? 0.78 : 0.74,
      metallic: 0,
      normalStrength: 0,
      scale: 1,
      domain: "local",
      appearance,
    };
  });
  const common = {
    bevel: 0,
    position: [0, 0, 0] as [number, number, number],
    rotation: [0, 0, 0] as [number, number, number],
    repeat: { count: 1, offset: [0, 0, 0] as [number, number, number] },
    sockets: [],
    wear: { amount: 0, scale: 1, seed: 0 },
  };
  const parts: AssemblyPart[] = [
    {
      ...common,
      id: "bark-column",
      name: "Bark column",
      material: materials[0].id,
      profile: { kind: "circle", radius: 0.34, segments: 48 },
      path: [
        [0, 0, 0],
        [0, 1.65, 0],
      ],
      position: [-0.73, 0, 0],
    },
    {
      ...common,
      id: "angular-stone",
      name: "Angular stone",
      material: materials[1].id,
      profile: {
        kind: "polygon",
        points: [
          [-0.44, -0.36],
          [-0.31, -0.52],
          [0.21, -0.49],
          [0.46, -0.29],
          [0.42, 0.32],
          [0.17, 0.56],
          [-0.27, 0.51],
          [-0.49, 0.23],
        ],
      },
      path: [
        [0, 0, -0.3],
        [0, 0, 0.3],
      ],
      position: [0.72, 0.52, 0],
      rotation: [0.03, -0.18, 0.07],
      bevel: 0.005,
      endBevel: 0.006,
    },
  ];
  const placeholder: FieldNode = {
    id: "source-envelope",
    name: "Assembly envelope",
    kind: "box",
    position: [0, 0.82, 0],
    rotation: [0, 0, 0],
    size: [1.4, 0.85, 0.5],
    radius: 0.1,
    blend: 0,
    children: [],
  };
  const object: ObjectDefinition = {
    ...envelope("surface-relief-specimens", "Bark and chipped stone"),
    kind: "object",
    dependencies: materials.map((material) => material.id),
    material: materials[0].id,
    collision: "none",
    assembly: { parts, grid: 0.01, clearances: [] },
    field: {
      root: placeholder.id,
      nodes: [placeholder],
      bounds: { min: [-1.4, -0.1, -0.6], max: [1.4, 1.8, 0.6] },
      resolution: 24,
    },
  };
  const stage: StageDefinition = {
    ...envelope("surface-relief-stage", "Physical surface relief review"),
    kind: "stage",
    environment: environment.id,
    lighting: lighting.id,
    subjects: [object.id],
    exposure: 1,
    ground: false,
    camera: SURFACE_RELIEF_CAMERAS.near,
  };
  const isolated = parts.map(
    (part, index): ObjectDefinition => ({
      ...structuredClone(object),
      id: `surface-relief-${index === 0 ? "bark" : "stone"}-specimen`,
      name: part.name,
      material: materials[index].id,
      dependencies: [materials[index].id],
      assembly: { parts: [structuredClone(part)], grid: 0.01, clearances: [] },
    }),
  );
  return {
    schemaVersion: 1,
    id: "surface-relief-lookdev",
    name: "Physical bark and stone study",
    entry: object.id,
    documents: [environment, lighting, ...materials, object, ...isolated, stage],
  };
}
