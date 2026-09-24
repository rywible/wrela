import type { MeshData, ObjectDefinition, Quat, Vec3 } from "@wrela/model";

import type { PhysicsAdapter } from "./physics";

/** Collision is an explicit realization of the same source, independent of presentation. */
export function installObjectCollision(
  physics: PhysicsAdapter,
  id: string,
  definition: ObjectDefinition,
  mesh: MeshData,
  position: Vec3,
  rotation: Quat = [0, 0, 0, 1],
  scale = 1,
): void {
  switch (definition.collision) {
    case "none":
      return;
    case "mesh":
      physics.addStaticMesh(id, mesh, position, rotation, scale);
      return;
    case "compound":
      physics.addStaticCompound(id, definition.colliders ?? [], position, rotation, scale);
      return;
    default:
      physics.addStaticObject(id, definition.collision, mesh.bounds, position, rotation, scale);
  }
}
