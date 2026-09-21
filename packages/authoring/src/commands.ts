import {
  documentSchema,
  fieldNodeSchema,
  idSchema,
  interventionSchema,
  jointSchema,
  numberSchema,
  vec3Schema,
} from "@wrela/model";
import { z } from "zod";
export const operationSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("document.create"), document: documentSchema }),
  z.object({ kind: z.literal("document.delete"), target: idSchema }),
  z.object({ kind: z.literal("document.rename"), target: idSchema, name: z.string().min(1).max(120) }),
  z.object({
    kind: z.literal("document.set"),
    target: idSchema,
    path: z
      .array(z.union([z.string().max(120), z.number().int().min(0).max(4096)]))
      .min(1)
      .max(8),
    value: z.unknown(),
  }),
  z.object({ kind: z.literal("document.detach"), target: idSchema }),
  z.object({ kind: z.literal("field.add"), target: idSchema, node: fieldNodeSchema }),
  z.object({ kind: z.literal("field.remove"), target: idSchema, node: idSchema }),
  z.object({
    kind: z.literal("field.update"),
    target: idSchema,
    node: idSchema,
    changes: fieldNodeSchema.partial().omit({ id: true }),
  }),
  z.object({ kind: z.literal("material.assign"), target: idSchema, material: idSchema }),
  z.object({ kind: z.literal("material.makeLocal"), target: idSchema, newId: idSchema }),
  z.object({ kind: z.literal("terrain.intervene"), target: idSchema, intervention: interventionSchema }),
  z.object({
    kind: z.literal("terrain.widenValley"),
    target: idSchema,
    intervention: idSchema,
    width: z.number().min(2).max(2000),
  }),
  z.object({
    kind: z.literal("world.placeForest"),
    target: idSchema,
    rule: idSchema,
    definition: idSchema,
    spacing: z.number().min(2).max(100),
    density: z.number().min(0).max(1),
    seed: z.number().int(),
  }),
  z.object({ kind: z.literal("character.addJoint"), target: idSchema, joint: jointSchema }),
  z.object({
    kind: z.literal("character.setJointLimit"),
    target: idSchema,
    joint: idSchema,
    minimum: numberSchema,
    maximum: numberSchema,
  }),
  z.object({ kind: z.literal("character.pose"), target: idSchema, joint: idSchema, rotation: vec3Schema }),
  z.object({
    kind: z.literal("character.addKey"),
    target: idSchema,
    motion: idSchema,
    joint: idSchema,
    time: z.number().min(0).max(120),
    rotation: vec3Schema,
    translation: vec3Schema,
  }),
]);
export type Operation = z.infer<typeof operationSchema>;
const revisionPrecondition = z.number().int().nonnegative().nullable();
export const documentPreconditionsSchema = z.object({
  /** null asserts that the document does not exist. */
  reads: z.record(idSchema, revisionPrecondition),
  writes: z.record(idSchema, revisionPrecondition),
});
export const batchSchema = z
  .object({
    expectedRevision: z.number().int().nonnegative().optional(),
    expectedDocuments: z.record(idSchema, z.number().int().nonnegative()).optional(),
    preconditions: documentPreconditionsSchema.optional(),
    transactionId: idSchema.optional(),
    actor: z.string().min(1).max(120).optional(),
    intent: z.string().min(1).max(1000).optional(),
    operations: z.array(operationSchema).min(1).max(256),
    label: z.string().max(120).optional(),
    gesture: z.string().max(100).optional(),
  })
  .refine((batch) => batch.expectedRevision !== undefined || batch.preconditions !== undefined, {
    message: "Provide an expected project revision or explicit document read/write preconditions",
  });
export type EditBatch = z.infer<typeof batchSchema>;
export const commandDescriptions: Record<Operation["kind"], string> = {
  "document.create": "Create a validated definition",
  "document.delete": "Delete an unreferenced definition",
  "document.rename": "Rename a definition without changing its identity",
  "document.set": "Set a validated domain property",
  "document.detach": "Detach a generated definition for direct editing",
  "field.add": "Add a shape to a field composition",
  "field.remove": "Remove a shape and repair its composition",
  "field.update": "Edit shape parameters",
  "material.assign": "Assign a shared material",
  "material.makeLocal": "Create and assign an independent material variant",
  "terrain.intervene": "Add or update an authored terrain intervention",
  "terrain.widenValley": "Change a valley intervention width",
  "world.placeForest": "Add or update a deterministic population rule",
  "character.addJoint": "Add a joint with an explicit parent",
  "character.setJointLimit": "Set an anatomical joint limit",
  "character.pose": "Set a joint rest pose",
  "character.addKey": "Insert or replace a motion key",
};
