import {
  botanicalGeometrySource,
  contentKey,
  type GrowthEvent,
  type PersistentVegetationGrowth,
  persistentVegetationGrowthSchema,
  type VegetationDefinition,
} from "@wrela/model";

import { analyzeBotanicalGrowth } from "./botanical-analysis";
import { botanicalGrowthKey, growBotanical } from "./botanical-growth";

export function vegetationGrowthDefinitionKey(doc: VegetationDefinition) {
  return contentKey({
    seed: doc.seed,
    height: doc.height,
    radius: doc.radius,
    botanical: botanicalGeometrySource(doc.botanical),
  });
}
/** Prepare a complete replacement off the render loop. The caller commits it only after compilation. */
export function prepareVegetationGrowth(
  doc: VegetationDefinition,
  steps: number,
  previous?: PersistentVegetationGrowth,
  events: GrowthEvent[] = [],
): PersistentVegetationGrowth {
  const initial = doc.botanical?.development;
  if (!initial) throw Error("Tree has no developmental source");
  if (!Number.isInteger(steps) || steps < 0 || steps > 64) throw Error("Invalid developmental step");
  if (previous) validateVegetationGrowth(doc, previous);
  const development = {
    ...(previous?.development ?? structuredClone(initial)),
    steps,
    events: [...(previous?.development.events ?? initial.events), ...events],
  };
  if (previous && events.some((event) => event.step <= previous.checkpoint.step))
    throw Error("New runtime events must follow the saved checkpoint");
  const checkpoint =
    previous && steps >= previous.checkpoint.step
      ? { ...previous.checkpoint, sourceKey: botanicalGrowthKey(previous.seed, development) }
      : undefined;
  const seed = previous?.seed ?? doc.seed;
  const result: PersistentVegetationGrowth = {
    version: 1,
    definition: doc.id,
    definitionKey: vegetationGrowthDefinitionKey(doc),
    seed,
    development,
    checkpoint: growBotanical(seed, development, checkpoint),
  };
  return validateVegetationGrowth(doc, result);
}
export function validateVegetationGrowth(
  doc: VegetationDefinition,
  input: PersistentVegetationGrowth,
): PersistentVegetationGrowth {
  const value = persistentVegetationGrowthSchema.parse(input);
  if (
    value.definition !== doc.id ||
    value.definitionKey !== vegetationGrowthDefinitionKey(doc) ||
    value.checkpoint.seed !== value.seed ||
    value.checkpoint.step !== value.development.steps ||
    value.checkpoint.sourceKey !== botanicalGrowthKey(value.seed, value.development)
  )
    throw Error("Vegetation checkpoint source mismatch");
  const analysis = analyzeBotanicalGrowth(value.checkpoint);
  if (analysis.violations.length) throw Error(`Invalid vegetation checkpoint: ${analysis.violations[0]}`);
  const byId = new Map(value.checkpoint.shoots.map((shoot) => [shoot.id, shoot]));
  if (
    value.checkpoint.ledger.length !== value.checkpoint.step ||
    new Set(value.checkpoint.buds.map((b) => b.id)).size !== value.checkpoint.buds.length ||
    value.checkpoint.buds.some((bud) => bud.parent && !byId.has(bud.parent))
  )
    throw Error("Invalid vegetation checkpoint history");
  return value;
}
export function vegetationGrowthDocument(
  doc: VegetationDefinition,
  value: PersistentVegetationGrowth,
  instanceId: string,
): VegetationDefinition {
  const record = validateVegetationGrowth(doc, value);
  const botanical = doc.botanical;
  if (!botanical) throw Error("Botanical source required");
  return {
    ...structuredClone(doc),
    id: `growth-${contentKey(instanceId)}`,
    seed: record.seed,
    botanical: { ...botanical, development: record.development },
  };
}

export type GrowthCompileResult = {
  growth: PersistentVegetationGrowth;
  document: VegetationDefinition;
  artifact: import("@wrela/model").CompiledVegetation;
};
export type GrowthCompileProvider = (
  document: VegetationDefinition,
  instanceId: string,
  steps: number,
  previous: PersistentVegetationGrowth | undefined,
  events: GrowthEvent[],
  quality: import("@wrela/model").Quality,
) => Promise<GrowthCompileResult>;
