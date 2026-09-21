import { z } from "zod";
export const idSchema = z
  .string()
  .min(1)
  .max(100)
  .regex(/^[a-zA-Z0-9_-]+$/);
export const numberSchema = z.number().finite();
export const vec3Schema = z.tuple([numberSchema, numberSchema, numberSchema]);
const color = z.tuple([z.number().min(0).max(1), z.number().min(0).max(1), z.number().min(0).max(1)]);
export const recipePathSchema = z
  .array(z.union([z.string().min(1).max(100), z.number().int().min(0)]))
  .min(1)
  .max(12);
export const recipeOverrideSchema = z.object({
  node: idSchema.optional(),
  path: recipePathSchema,
  value: z.union([z.number().finite(), z.string().max(120), z.boolean()]),
});
const envelope = {
  id: idSchema,
  name: z.string().min(1).max(120),
  schemaVersion: z.literal(1),
  dependencies: z.array(idSchema).max(128),
  generated: z
    .object({
      generator: z.string(),
      policy: z.enum(["locked", "detached"]),
      recipe: z
        .object({
          id: idSchema,
          version: z.string().min(1).max(64),
          key: z.string().max(128),
          parameters: z.record(idSchema, z.number().finite()),
          overrides: z.array(recipeOverrideSchema).max(128),
        })
        .optional(),
    })
    .optional(),
};
export const fieldNodeSchema = z.object({
  id: idSchema,
  name: z.string().max(120),
  kind: z.enum([
    "sphere",
    "ellipsoid",
    "box",
    "capsule",
    "torus",
    "union",
    "subtract",
    "intersect",
    "smoothUnion",
  ]),
  position: vec3Schema,
  rotation: vec3Schema,
  size: vec3Schema,
  radius: z.number().min(0.01).max(100),
  blend: z.number().min(0).max(10),
  children: z.array(idSchema).max(64),
  material: idSchema.optional(),
});
export type FieldNode = z.infer<typeof fieldNodeSchema>;
export const fieldSchema = z.object({
  root: idSchema,
  nodes: z.array(fieldNodeSchema).min(1).max(128),
  bounds: z.object({ min: vec3Schema, max: vec3Schema }),
  resolution: z.number().int().min(12).max(80),
  fidelity: z
    .object({
      maxError: z.number().finite().positive().max(100).optional(),
      minimumFeatureSize: z.number().finite().positive().max(200).optional(),
      strict: z.boolean().optional(),
    })
    .optional(),
});
export type FieldDefinition = z.infer<typeof fieldSchema>;
export const materialSchema = z.object({
  ...envelope,
  kind: z.literal("material"),
  color,
  secondary: color,
  roughness: z.number().min(0.04).max(1),
  metallic: z.number().min(0).max(1),
  pattern: z.enum(["solid", "noise", "stripes", "marble"]),
  scale: z.number().min(0.01).max(100),
  normalStrength: z.number().min(0).max(1),
  domain: z.enum(["local", "world"]).optional(),
  layers: z
    .array(
      z.object({
        color,
        roughness: z.number().min(0.04).max(1),
        metallic: z.number().min(0).max(1),
        coverage: z.number().min(0).max(1),
        slopeBias: z.number().min(-1).max(1),
        noiseScale: z.number().min(0.01).max(100),
        normalStrength: z.number().min(0).max(1),
      }),
    )
    .max(2)
    .optional(),
});
export const jointSchema = z.object({
  id: idSchema,
  name: z.string().max(120),
  parent: idSchema.nullable(),
  position: vec3Schema,
  rotation: vec3Schema,
  radius: z.number().min(0.01).max(10),
  minimum: z.number().min(-Math.PI).max(Math.PI),
  maximum: z.number().min(-Math.PI).max(Math.PI),
});
export const motionSchema = z.object({
  id: idSchema,
  name: z.string().max(120),
  duration: z.number().min(0.1).max(120),
  loop: z.boolean(),
  keys: z
    .array(
      z.object({
        joint: idSchema,
        time: z.number().min(0).max(120),
        rotation: vec3Schema,
        translation: vec3Schema,
      }),
    )
    .max(2048),
});
const colliderEnvelope = { id: idSchema, position: vec3Schema, rotation: vec3Schema.default([0, 0, 0]) };
const colliderExtent = z.number().finite().min(0.01).max(100);
export const characterColliderSchema = z.discriminatedUnion("shape", [
  z.object({
    ...colliderEnvelope,
    shape: z.literal("box"),
    size: z.tuple([colliderExtent, colliderExtent, colliderExtent]),
  }),
  z.object({ ...colliderEnvelope, shape: z.literal("sphere"), radius: colliderExtent }),
  z.object({
    ...colliderEnvelope,
    shape: z.literal("capsule"),
    radius: colliderExtent,
    halfHeight: z.number().finite().min(0).max(100),
  }),
]);
export type CharacterCollider = z.infer<typeof characterColliderSchema>;
export const objectSchema = z.object({
  ...envelope,
  kind: z.literal("object"),
  field: fieldSchema,
  material: idSchema,
  collision: z.enum(["none", "box", "sphere", "mesh", "compound"]),
  colliders: z.array(characterColliderSchema).min(1).max(32).optional(),
});
export const characterSchema = z.object({
  ...envelope,
  kind: z.literal("character"),
  field: fieldSchema,
  material: idSchema,
  joints: z.array(jointSchema).min(1).max(64),
  motions: z.array(motionSchema).max(32),
  physics: z.object({
    mode: z.enum(["kinematic", "dynamic", "motor"]),
    mass: z.number().min(0.01).max(1000),
    restitution: z.number().min(0).max(1),
    friction: z.number().min(0).max(3),
    colliders: z.array(characterColliderSchema).min(1).max(16).optional(),
  }),
});
export const vegetationSchema = z.object({
  ...envelope,
  kind: z.literal("vegetation"),
  seed: z.number().int(),
  height: z.number().min(0.5).max(30),
  radius: z.number().min(0.1).max(10),
  branches: z.number().int().min(3).max(32),
  material: idSchema,
  trunkMaterial: idSchema,
  windResponse: z.number().min(0).max(2),
  variation: z.number().min(0).max(1),
});
export const lightingSchema = z.object({
  ...envelope,
  kind: z.literal("lighting"),
  lights: z
    .array(
      z.object({
        id: idSchema,
        type: z.enum(["directional", "point"]),
        position: vec3Schema,
        color,
        intensity: z.number().min(0).max(30),
      }),
    )
    .max(8),
  ambient: z.number().min(0).max(3),
});
export const environmentSchema = z.object({
  ...envelope,
  kind: z.literal("environment"),
  model: z.literal("analytic-sky"),
  sunElevation: z.number().min(-0.2).max(1.57),
  sunAzimuth: z.number().min(-6.29).max(6.29),
  turbidity: z.number().min(1).max(10),
  fogDensity: z.number().min(0).max(0.1),
  skyColor: color,
  horizonColor: color,
  groundColor: color,
  wind: vec3Schema,
});
export const waterSchema = z.object({
  ...envelope,
  kind: z.literal("water"),
  level: z.number().min(-100).max(100),
  color,
  roughness: z.number().min(0.02).max(1),
  waves: z
    .array(
      z.object({
        amplitude: z.number().min(0).max(3),
        wavelength: z.number().min(0.5).max(100),
        speed: z.number().min(-10).max(10),
        direction: z.number().min(-6.29).max(6.29),
        phase: z.number().min(-100).max(100),
      }),
    )
    .max(8),
});
export const interventionSchema = z.object({
  id: idSchema,
  kind: z.enum(["raise", "lower", "flatten", "valley", "clearing", "river"]),
  center: z.tuple([numberSchema, numberSchema]),
  radius: z.number().min(1).max(1000),
  strength: z.number().min(-100).max(100),
  targetHeight: z.number().min(-100).max(100),
});
export const terrainSchema = z.object({
  ...envelope,
  kind: z.literal("terrain"),
  seed: z.number().int(),
  amplitude: z.number().min(0).max(100),
  frequency: z.number().min(0.0001).max(0.5),
  octaves: z.number().int().min(1).max(6),
  baseHeight: z.number().min(-100).max(100),
  material: idSchema,
  interventions: z.array(interventionSchema).max(256),
});
export const worldSchema = z.object({
  ...envelope,
  kind: z.literal("world"),
  generatorVersion: z.literal("wrela-world-1"),
  terrain: idSchema,
  environment: idSchema,
  lighting: idSchema,
  water: idSchema.optional(),
  populations: z
    .array(
      z.object({
        id: idSchema,
        definition: idSchema,
        spacing: z.number().min(2).max(100),
        density: z.number().min(0).max(1),
        minHeight: numberSchema,
        maxHeight: numberSchema,
        maxSlope: z.number().min(0).max(1),
        seed: z.number().int(),
      }),
    )
    .max(16),
  instances: z
    .array(
      z.object({
        id: idSchema,
        definition: idSchema,
        position: vec3Schema,
        rotation: vec3Schema,
        scale: z.number().min(0.01).max(100),
      }),
    )
    .max(256),
});
export const stageSchema = z.object({
  ...envelope,
  kind: z.literal("stage"),
  environment: idSchema,
  lighting: idSchema,
  ground: z.boolean(),
  exposure: z.number().min(0.1).max(8),
  camera: z.object({ position: vec3Schema, target: vec3Schema, fov: z.number().min(15).max(100) }),
  subjects: z.array(idSchema).max(32),
});
export const documentSchema = z.discriminatedUnion("kind", [
  materialSchema,
  objectSchema,
  characterSchema,
  vegetationSchema,
  lightingSchema,
  environmentSchema,
  waterSchema,
  terrainSchema,
  worldSchema,
  stageSchema,
]);
export type Document = z.infer<typeof documentSchema>;
export type MaterialDefinition = z.infer<typeof materialSchema>;
export type ObjectDefinition = z.infer<typeof objectSchema>;
export type CharacterDefinition = z.infer<typeof characterSchema>;
export type VegetationDefinition = z.infer<typeof vegetationSchema>;
export type LightingRigDefinition = z.infer<typeof lightingSchema>;
export type EnvironmentDefinition = z.infer<typeof environmentSchema>;
export type WaterDefinition = z.infer<typeof waterSchema>;
export type TerrainDefinition = z.infer<typeof terrainSchema>;
export type WorldDefinition = z.infer<typeof worldSchema>;
export type StageDefinition = z.infer<typeof stageSchema>;
export type Joint = z.infer<typeof jointSchema>;
export type Motion = z.infer<typeof motionSchema>;
export type Intervention = z.infer<typeof interventionSchema>;
export const recipeSchema = z.object({
  id: idSchema,
  version: z.string().min(1).max(64),
  template: documentSchema,
  parameters: z
    .record(
      idSchema,
      z.object({
        default: numberSchema,
        min: numberSchema,
        max: numberSchema,
        integer: z.boolean().optional(),
      }),
    )
    .refine((parameters) => Object.keys(parameters).length <= 32, "Recipe supports at most 32 parameters"),
  bindings: z
    .array(
      z.object({
        parameter: idSchema,
        node: idSchema.optional(),
        path: recipePathSchema,
        scale: numberSchema.optional(),
        offset: numberSchema.optional(),
      }),
    )
    .max(128),
});
export const recipeInstanceSchema = z.object({
  id: idSchema,
  name: z.string().min(1).max(120),
  recipe: idSchema,
  version: z.string().min(1).max(64),
  parameters: z.record(idSchema, numberSchema).optional(),
  overrides: z.array(recipeOverrideSchema).max(128).optional(),
});
export type RecipeDefinition = z.infer<typeof recipeSchema>;
export type RecipeInstance = z.infer<typeof recipeInstanceSchema>;
export const projectSchema = z.object({
  schemaVersion: z.literal(1),
  id: idSchema,
  name: z.string().min(1).max(120),
  documents: z.array(documentSchema).min(1).max(256),
  entry: idSchema,
  recipes: z.array(recipeSchema).max(64).optional(),
  recipeInstances: z.array(recipeInstanceSchema).max(256).optional(),
});
export type Project = z.infer<typeof projectSchema>;
