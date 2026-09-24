import type { Operation } from "@wrela/authoring";
import { buildGeologyObjects, geologyGeneratorId } from "@wrela/compiler";
import type { Project, TerrainDefinition } from "@wrela/model";

/** Explicit bake transaction: existing published instances are replaced only in worlds
 * using this terrain. Unused generated objects remain available for deliberate reuse. */
export function geologyPublication(project: Project, terrain: TerrainDefinition): Operation[] {
  const objects = buildGeologyObjects(terrain),
    generator = geologyGeneratorId(terrain);
  const existing = new Map(project.documents.map((document) => [document.id, document]));
  const owned = new Set(
    project.documents
      .filter((document) => document.generated?.generator === generator)
      .map((document) => document.id),
  );
  const worlds = project.documents.filter(
    (document) => document.kind === "world" && document.terrain === terrain.id,
  );
  if (!worlds.length) throw new Error("Create a world using this terrain before publishing formations.");
  if (project.documents.length + objects.filter((object) => !existing.has(object.id)).length > 256)
    throw new Error("Publishing exceeds the project's 256 document limit.");
  const operations: Operation[] = [];
  for (const object of objects) {
    const previous = existing.get(object.id);
    if (previous) {
      if (previous.kind !== "object" || previous.generated?.generator !== generator)
        throw new Error(`Formation identity conflicts with ${previous.name}.`);
      for (const key of ["field", "material", "collision"] as const)
        operations.push({ kind: "document.set", target: object.id, path: [key], value: object[key] });
    } else operations.push({ kind: "document.create", document: object });
  }
  for (const world of worlds) {
    if (world.kind !== "world") continue;
    const instances = world.instances.filter(
      (instance) => !(owned.has(instance.definition) && instance.id === `placed-${instance.definition}`),
    );
    for (let index = 0; index < objects.length; index++) {
      const object = objects[index],
        formation = (terrain.geology?.formations ?? [])[index];
      if (instances.some((instance) => instance.id === `placed-${object.id}`))
        throw new Error("A formation instance ID is already in use by an unrelated instance.");
      instances.push({
        id: `placed-${object.id}`,
        definition: object.id,
        position: [...formation.position],
        rotation: [0, 0, 0],
        scale: 1,
      });
    }
    if (instances.length > 256) throw new Error(`Publishing exceeds ${world.name}'s 256 instance limit.`);
    operations.push({ kind: "document.set", target: world.id, path: ["instances"], value: instances });
  }
  if (operations.length > 256)
    throw new Error("Publishing would exceed the transaction limit; reduce the number of linked worlds.");
  return operations;
}
