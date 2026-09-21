import type { DormantEntity } from "@wrela/world";

/** Release-authored, exact compatibility decisions. World generator compatibility remains strict. */
export type RuntimeSaveMigration = {
  id: string;
  characters: {
    definition: string;
    fromArtifactKey: string;
    toArtifactKey: string;
    motionIds?: Record<string, string>;
  }[];
};
export type RuntimeSaveMigrationReport = { id: string; changedEntities: string[] };

/** Pure copy: original saves remain available for rollback or a different release. */
export function remapRuntimeEntities(
  entities: DormantEntity[],
  migration: RuntimeSaveMigration,
): { entities: DormantEntity[]; report: RuntimeSaveMigrationReport } {
  if (!migration.id || migration.id.length > 120 || migration.characters.length > 256)
    throw new Error("Invalid runtime migration plan");
  const rules = new Map<string, RuntimeSaveMigration["characters"][number]>();
  for (const rule of migration.characters) {
    if (!rule.definition || !rule.fromArtifactKey || !rule.toArtifactKey)
      throw new Error("Migration rules require exact definition and artifact keys");
    const key = JSON.stringify([rule.definition, rule.fromArtifactKey]);
    if (rules.has(key)) throw new Error("Ambiguous runtime migration rule");
    if (Object.keys(rule.motionIds ?? {}).length > 32) throw new Error("Too many motion remaps");
    rules.set(key, rule);
  }
  const copy = structuredClone(entities),
    changedEntities: string[] = [];
  for (const entity of copy) {
    if (entity.state.runtime !== "wrela-character-1") continue;
    const rule = rules.get(JSON.stringify([entity.definition, entity.state.artifactKey]));
    if (!rule) continue;
    entity.state.artifactKey = rule.toArtifactKey;
    for (const field of ["motion", "previousMotion"]) {
      const motion = entity.state[field];
      if (typeof motion === "string" && Object.hasOwn(rule.motionIds ?? {}, motion))
        entity.state[field] = rule.motionIds?.[motion] ?? motion;
    }
    changedEntities.push(entity.id);
  }
  return { entities: copy, report: { id: migration.id, changedEntities } };
}
