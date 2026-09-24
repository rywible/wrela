import { buildGeologyObjects, geologyGeneratorId } from "@wrela/compiler";
import {
  contentKey,
  type Document,
  type Project,
  type TerrainDefinition,
  type WorldDefinition,
} from "@wrela/model";

/** A terrain editor sees unpublished rock changes immediately. These documents
 * live only in the candidate scene's map; the project and world bake stay intact. */
export function createGeologyPreview(
  project: Project,
  terrain: TerrainDefinition,
  baseWorld: WorldDefinition,
) {
  const documents = new Map<string, Document>(project.documents.map((document) => [document.id, document]));
  const generator = geologyGeneratorId(terrain);
  const owned = new Set(
    project.documents
      .filter((document) => document.generated?.generator === generator)
      .map((document) => document.id),
  );
  const instances = baseWorld.instances.filter(
    (instance) => !(owned.has(instance.definition) && instance.id === `placed-${instance.definition}`),
  );
  const objects = buildGeologyObjects(terrain);
  for (let index = 0; index < objects.length; index++) {
    const object = objects[index],
      previous = documents.get(object.id);
    if (previous && (previous.kind !== "object" || previous.generated?.generator !== generator))
      throw new Error(`Formation preview identity conflicts with ${previous.name}.`);
    const id = `placed-${object.id}`;
    if (instances.some((instance) => instance.id === id))
      throw new Error("A formation preview instance ID is already in use by an unrelated instance.");
    const formation = terrain.geology?.formations[index];
    if (!formation) throw new Error("Formation source is unavailable");
    documents.set(object.id, object);
    instances.push({
      id,
      definition: object.id,
      position: [...formation.position],
      rotation: [0, 0, 0],
      scale: 1,
    });
  }
  const world: WorldDefinition = {
    ...baseWorld,
    id: `geology-preview-${contentKey(terrain.id)}`,
    name: `${terrain.name} preview`,
    instances,
  };
  return { world, documents };
}
