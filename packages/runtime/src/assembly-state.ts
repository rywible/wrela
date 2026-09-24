import { type AssemblyDefinition, contentKey } from "@wrela/model";

import type { DormantEntity } from "@wrela/world";

/** Indexed values keep the largest 128-part assembly within world-save string limits. */
export function encodeAssemblyJoints(
  source: AssemblyDefinition,
  commands: Readonly<Record<string, number>>,
): string {
  return JSON.stringify(
    source.parts.map((part) => (Object.hasOwn(commands, part.id) ? commands[part.id] : null)),
  );
}
export function decodeAssemblyState(
  entity: DormantEntity,
  source: AssemblyDefinition,
  definition: string,
): { tick: number; commands: Record<string, number> } {
  if (entity.definition !== definition || entity.state.sourceKey !== contentKey(source))
    throw new Error(`Saved assembly ${entity.id} is incompatible with the prepared world`);
  const tick = entity.state.tick;
  if (typeof tick !== "number" || !Number.isSafeInteger(tick) || tick < 0)
    throw new Error("Invalid saved assembly tick");
  if (typeof entity.state.joints !== "string" || entity.state.joints.length > 4096)
    throw new Error("Invalid saved assembly joint values");
  const values: unknown = JSON.parse(entity.state.joints);
  if (!Array.isArray(values) || values.length !== source.parts.length)
    throw new Error("Saved assembly joint count is incompatible");
  const commands: Record<string, number> = Object.create(null);
  values.forEach((value: unknown, index) => {
    if (value === null) return;
    const part = source.parts[index],
      joint = part.joint;
    if (
      !joint ||
      typeof value !== "number" ||
      !Number.isFinite(value) ||
      value < joint.minimum ||
      value > joint.maximum
    )
      throw new Error(`Invalid saved assembly joint ${part.id}`);
    commands[part.id] = value;
  });
  return { tick, commands };
}
