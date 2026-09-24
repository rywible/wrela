import { type CharacterDefinition, contentKey, idSchema, vec3Schema } from "@wrela/model";

import { z } from "zod";

export const creatureObservationSchema = z.strictObject({
  id: idSchema,
  target: idSchema,
  sourceKey: z.string().min(1),
  region: idSchema,
  summary: z.string().min(1).max(1000),
  category: z.enum(["silhouette", "anatomy", "deformation", "contact", "surface", "groom", "performance"]),
  priority: z.enum(["blocking", "important", "polish"]),
  evidence: z.strictObject({
    capture: z.string().min(1).max(1000),
    motion: idSchema,
    tick: z.number().int().min(0).max(7200),
    camera: z.strictObject({
      position: vec3Schema,
      target: vec3Schema,
      fov: z.number().finite().min(1).max(179),
    }),
    channel: z.string().min(1).max(50),
    restPoint: vec3Schema.optional(),
  }),
  intent: z.string().min(1).max(1000),
  protect: z.array(idSchema).max(64).default([]),
});
export const creatureAssessmentSchema = z.strictObject({
  observation: idSchema,
  sourceKey: z.string().min(1),
  status: z.enum(["open", "resolved", "rejected"]),
  reason: z.string().min(1).max(2000),
  evidence: z.array(z.string().min(1).max(1000)).min(1).max(32),
});
export const creatureNotebookSchema = z.strictObject({
  version: z.literal(1),
  target: idSchema,
  observations: z.array(creatureObservationSchema).max(256),
  assessments: z.array(creatureAssessmentSchema).max(1024),
});
export type CreatureNotebook = z.infer<typeof creatureNotebookSchema>;
export function appendCreatureObservation(
  book: CreatureNotebook,
  input: z.input<typeof creatureObservationSchema>,
) {
  const result = creatureNotebookSchema.parse(book),
    observation = creatureObservationSchema.parse(input);
  if (observation.target !== result.target) throw Error("Observation belongs to another creature");
  const old = result.observations.find((o) => o.id === observation.id);
  if (old) {
    if (contentKey(old) !== contentKey(observation))
      throw Error("Observation IDs are immutable; append a new observation");
    return result;
  }
  result.observations.push(observation);
  return creatureNotebookSchema.parse(result);
}
export function assessCreatureObservation(
  book: CreatureNotebook,
  input: z.input<typeof creatureAssessmentSchema>,
) {
  const result = creatureNotebookSchema.parse(book),
    assessment = creatureAssessmentSchema.parse(input);
  if (!result.observations.some((o) => o.id === assessment.observation)) throw Error("Unknown observation");
  result.assessments.push(assessment);
  return creatureNotebookSchema.parse(result);
}
export function inspectCreatureNotebook(character: CharacterDefinition, book: CreatureNotebook) {
  const parsed = creatureNotebookSchema.parse(book),
    sourceKey = contentKey(character);
  if (parsed.target !== character.id) throw Error("Notebook belongs to another creature");
  return {
    sourceKey,
    observations: parsed.observations.map((observation) => {
      const assessment = parsed.assessments
        .slice()
        .reverse()
        .find((a) => a.observation === observation.id);
      return {
        observation,
        assessment,
        status: assessment?.sourceKey === sourceKey ? assessment.status : "open",
        needsReview: !assessment || assessment.sourceKey !== sourceKey,
        originalEvidenceStale: observation.sourceKey !== sourceKey,
      };
    }),
    policy:
      "An assessment approves only its exact source. A later edit reopens review without erasing earlier evidence.",
  };
}
