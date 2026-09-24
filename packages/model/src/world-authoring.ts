import { z } from "zod";
import { worldRoomModules } from "./world-room";

export * from "./world-room";

import { worldPathSampleCount } from "./world-path";

export * from "./world-path";

const id = z
  .string()
  .min(1)
  .max(40)
  .regex(/^[a-zA-Z0-9_-]+$/);
const reference = z
  .string()
  .min(1)
  .max(100)
  .regex(/^[a-zA-Z0-9_-]+$/);
const scalar = z.number().finite();
const point = z.tuple([scalar, scalar, scalar]);
const zone = { center: point, radius: scalar.min(1).max(10000) };
const grounding = z.object({ offset: scalar.min(-100).max(100) }).optional();
const member = z.object({
  id,
  definition: reference,
  position: point,
  yaw: scalar,
  scale: scalar.min(0.01).max(100),
});
/** Semantic layout source. Generated identities depend on authored IDs, never array order. */
export const worldCompositionSchema = z.object({
  review: z
    .object({
      actorRadius: scalar.min(0.05).max(5),
      actorHeight: scalar.min(0.2).max(5),
      maxStepHeight: scalar.min(0.01).max(2),
      entryPath: id.optional(),
      sightline: z.object({ from: point, to: point }),
    })
    .optional(),
  assemblies: z
    .array(z.object({ id, name: z.string().min(1).max(120), members: z.array(member).max(128) }))
    .max(32),
  placements: z
    .array(
      z.object({
        id,
        assembly: id,
        position: point,
        yaw: scalar,
        scale: scalar.min(0.01).max(100),
        grounding,
      }),
    )
    .max(128),
  paths: z
    .array(
      z.object({
        id,
        kind: z.enum(["road", "path"]),
        points: z.array(point).min(2).max(64),
        cornerRadius: scalar.min(0).max(100).optional(),
        width: scalar.min(1).max(100),
        shoulder: scalar.min(0).max(50),
        flatten: z.boolean(),
        definition: reference.optional(),
        spacing: scalar.min(1).max(100),
        maxGrade: scalar.min(0).max(2),
      }),
    )
    .max(32),
  rooms: z
    .array(
      z.object({
        id,
        center: point,
        size: z.tuple([scalar.min(2).max(1000), scalar.min(2).max(1000)]),
        yaw: scalar,
        wallDefinition: reference,
        moduleWidth: scalar.min(0.5).max(100),
        doorSide: z.enum(["north", "east", "south", "west"]),
        doorWidth: scalar.min(0.5).max(20),
      }),
    )
    .max(32),
  biomes: z
    .array(
      z.object({
        id,
        ...zone,
        transition: scalar.min(0).max(1000),
        populations: z.array(reference).max(16),
        density: scalar.min(0).max(1),
      }),
    )
    .max(32),
  spaces: z
    .array(
      z.object({
        id,
        kind: z.enum(["landmark", "encounter"]),
        ...zone,
        grounding,
        clearPopulation: z.boolean(),
        members: z.array(member).max(64),
      }),
    )
    .max(64),
  streaming: z
    .array(
      z.object({
        id,
        ...zone,
        preloadDistance: scalar.min(0).max(10000),
        priority: scalar.min(0.01).max(10),
        collision: z.boolean(),
      }),
    )
    .max(64),
  overrides: z
    .array(
      z.object({
        id: z.string().min(1).max(512),
        removed: z.boolean(),
        position: point.optional(),
        yaw: scalar.optional(),
        scale: scalar.min(0.01).max(100).optional(),
      }),
    )
    .max(256),
});
export type WorldComposition = z.infer<typeof worldCompositionSchema>;
export type CompositionIssue = {
  severity: "error" | "warning";
  code: string;
  message: string;
  node?: string;
};
export function emptyWorldComposition(): WorldComposition {
  return {
    assemblies: [],
    placements: [],
    paths: [],
    rooms: [],
    biomes: [],
    spaces: [],
    streaming: [],
    overrides: [],
  };
}
export function worldCompositionReferences(composition?: WorldComposition): string[] {
  if (!composition) return [];
  return [
    ...new Set([
      ...composition.assemblies.flatMap((assembly) => assembly.members.map((part) => part.definition)),
      ...composition.paths.flatMap((path) => (path.definition ? [path.definition] : [])),
      ...composition.rooms.map((room) => room.wallDefinition),
      ...composition.spaces.flatMap((space) => space.members.map((part) => part.definition)),
    ]),
  ];
}
export function validateWorldComposition(
  composition: WorldComposition,
  populationIds: readonly string[] = [],
  budget: { instanceIds?: readonly string[]; interventionCount?: number } = {},
): CompositionIssue[] {
  const issues: CompositionIssue[] = [];
  const add = (node: string, message: string) =>
    issues.push({ severity: "error", code: "world.composition", node, message });
  for (const [kind, items] of Object.entries(composition)) {
    if (!Array.isArray(items)) continue;
    const seen = new Set<string>();
    for (const item of items) {
      if (seen.has(item.id)) add(item.id, `Duplicate ${kind} identity: ${item.id}`);
      seen.add(item.id);
    }
  }
  for (const group of [...composition.assemblies, ...composition.spaces]) {
    if (new Set(group.members.map((part) => part.id)).size !== group.members.length)
      add(group.id, "Member identities must be unique");
  }
  const assemblies = new Set(composition.assemblies.map((assembly) => assembly.id));
  for (const placement of composition.placements)
    if (!assemblies.has(placement.assembly)) add(placement.id, `Missing assembly: ${placement.assembly}`);
  for (const biome of composition.biomes) {
    if (biome.transition > biome.radius) add(biome.id, "Biome transition must fit inside its radius");
    for (const population of biome.populations)
      if (!populationIds.includes(population)) add(biome.id, `Missing population: ${population}`);
  }
  for (const room of composition.rooms) {
    const width = room.doorSide === "north" || room.doorSide === "south" ? room.size[0] : room.size[1];
    if (room.doorWidth >= width) add(room.id, "Door must be narrower than its wall");
  }
  for (const path of composition.paths) {
    if (path.flatten && path.points.some((point) => Math.abs(point[1]) > 100))
      add(path.id, "Road grading elevations must remain between -100 and 100 meters");
  }
  for (const placement of composition.placements) {
    const assembly = composition.assemblies.find((item) => item.id === placement.assembly);
    if (
      assembly?.members.some(
        (member) => member.scale * placement.scale > 100 || member.scale * placement.scale < 0.01,
      )
    )
      add(placement.id, "Combined assembly and member scale exceeds instance limits");
  }
  for (const path of composition.paths)
    for (let i = 1; i < path.points.length; i++) {
      const a = path.points[i - 1],
        b = path.points[i];
      const length = Math.hypot(b[0] - a[0], b[2] - a[2]);
      if (length < 0.001) add(path.id, "Path segments require distinct horizontal points");
      else if (Math.abs(b[1] - a[1]) / length > path.maxGrade)
        issues.push({
          severity: "warning",
          code: "world.path.grade",
          node: path.id,
          message: `Segment ${i} exceeds the traversal grade limit`,
        });
    }
  if (
    composition.review?.entryPath &&
    !composition.paths.some((path) => path.id === composition.review?.entryPath)
  )
    add("review", `Missing entry path: ${composition.review.entryPath}`);
  const removed = new Set(composition.overrides.filter((item) => item.removed).map((item) => item.id));
  const realizedIds = new Set((budget.instanceIds ?? []).filter((id) => !removed.has(id)));
  let instanceCount = realizedIds.size,
    interventionCount = budget.interventionCount ?? 0;
  const admit = (node: string, id: string) => {
    if (realizedIds.has(id)) add(node, `Generated instance identity collides: ${id}`);
    realizedIds.add(id);
    if (!removed.has(id)) instanceCount++;
  };
  for (const placement of composition.placements) {
    const assembly = composition.assemblies.find((item) => item.id === placement.assembly);
    for (const member of assembly?.members ?? [])
      admit(placement.id, `layout_assembly_${placement.id}_${member.id}`);
  }
  for (const space of composition.spaces)
    for (const member of space.members) admit(space.id, `layout_space_${space.id}_${member.id}`);
  for (const room of composition.rooms) {
    try {
      for (const module of worldRoomModules(room))
        admit(room.id, `layout_room_${room.id}_${module.side}_${module.index}`);
    } catch (error) {
      add(room.id, error instanceof Error ? error.message : String(error));
    }
  }
  for (const path of composition.paths) {
    if (path.definition) {
      const count = worldPathSampleCount(path, path.spacing);
      if (count > 256) add(path.id, "Path exceeds the 256 module sampling budget");
      else for (let index = 0; index < count; index++) admit(path.id, `layout_path_${path.id}_${index}`);
    }
    if (path.flatten) {
      const count = worldPathSampleCount(path, path.width / 2);
      if (count > 256) add(path.id, "Path exceeds the 256 grading sample budget");
      interventionCount += count;
    }
  }
  if (instanceCount > 256)
    add("composition", `World composition requires ${instanceCount} instances; limit is 256`);
  if (interventionCount > 256)
    add("composition", `World composition requires ${interventionCount} terrain interventions; limit is 256`);
  return issues;
}
