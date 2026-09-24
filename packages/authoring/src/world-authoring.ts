import { emptyWorldComposition, type WorldComposition, type WorldDefinition } from "@wrela/model";

export type WorldLayoutKind = "assembly" | "path" | "room" | "biome" | "landmark" | "encounter" | "streaming";
/** Build an immutable edit for the normal document transaction/undo path. */
export function addWorldLayout(
  world: WorldDefinition,
  kind: WorldLayoutKind,
  definition?: string,
): WorldComposition {
  const composition = structuredClone(world.composition ?? emptyWorldComposition());
  const ids = new Set(
    Object.values(composition).flatMap((items) => (Array.isArray(items) ? items.map((item) => item.id) : [])),
  );
  let suffix = 1;
  while (ids.has(`${kind}-${suffix}`)) suffix++;
  const id = `${kind}-${suffix}`;
  const member = definition
    ? [{ id: "part", definition, position: [0, 0, 0] as [number, number, number], yaw: 0, scale: 1 }]
    : [];
  if (kind === "assembly") {
    composition.assemblies.push({ id, name: "Assembly", members: member });
    composition.placements.push({ id, assembly: id, position: [0, 0, 0], yaw: 0, scale: 1 });
  } else if (kind === "path")
    composition.paths.push({
      id,
      kind: "path",
      points: [
        [0, 0, 0],
        [10, 0, 0],
      ],
      cornerRadius: 0,
      width: 3,
      shoulder: 1,
      flatten: true,
      spacing: 3,
      maxGrade: 0.25,
    });
  else if (kind === "room") {
    if (!definition) throw new Error("Choose a wall definition before creating a room");
    composition.rooms.push({
      id,
      center: [0, 0, 0],
      size: [8, 8],
      yaw: 0,
      wallDefinition: definition,
      moduleWidth: 2,
      doorSide: "south",
      doorWidth: 2,
    });
  } else if (kind === "biome")
    composition.biomes.push({
      id,
      center: [0, 0, 0],
      radius: 40,
      transition: 10,
      populations: world.populations.map((rule) => rule.id),
      density: 1,
    });
  else if (kind === "streaming")
    composition.streaming.push({
      id,
      center: [0, 0, 0],
      radius: 32,
      preloadDistance: 64,
      priority: 2,
      collision: true,
    });
  else
    composition.spaces.push({
      id,
      kind,
      center: [0, 0, 0],
      radius: 8,
      clearPopulation: true,
      members: member,
    });
  return composition;
}
export function duplicateAssemblyPlacement(composition: WorldComposition, id: string): WorldComposition {
  const result = structuredClone(composition),
    source = result.placements.find((placement) => placement.id === id);
  if (!source) throw new Error(`Unknown assembly placement: ${id}`);
  let suffix = 1;
  while (result.placements.some((placement) => placement.id === `${id}-${suffix}`)) suffix++;
  result.placements.push({
    ...source,
    id: `${id}-${suffix}`,
    position: [source.position[0] + 5, source.position[1], source.position[2]],
  });
  return result;
}

/** Append a usable continuation instead of a duplicate origin that validation rejects. */
export function appendWorldPathPoint(composition: WorldComposition, id: string): WorldComposition {
  const result = structuredClone(composition);
  const path = result.paths.find((path) => path.id === id);
  if (!path) throw new Error(`Unknown path: ${id}`);
  if (path.points.length >= 64) throw new RangeError("A path supports at most 64 control points");
  const end = path.points[path.points.length - 1],
    previous = path.points[path.points.length - 2];
  const dx = end[0] - previous[0],
    dz = end[2] - previous[2];
  const length = Math.hypot(dx, dz);
  path.points.push([
    end[0] + (length ? dx / length : 1) * 5,
    end[1],
    end[2] + (length ? dz / length : 0) * 5,
  ]);
  return result;
}

export type WorldLayoutCollection = Exclude<keyof WorldComposition, "review">;
/** Remove relationship owners and their dependents in one undoable transaction. */
export function removeWorldLayoutElement(
  composition: WorldComposition,
  collection: WorldLayoutCollection,
  id: string,
): WorldComposition {
  const result = structuredClone(composition);
  Object.assign(result, { [collection]: result[collection].filter((item) => item.id !== id) });
  if (collection === "assemblies")
    result.placements = result.placements.filter((placement) => placement.assembly !== id);
  if (collection === "paths" && result.review?.entryPath === id) delete result.review.entryPath;
  return result;
}
