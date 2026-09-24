import {
  type AssemblyDefinition,
  type AssemblyPart,
  assemblyPartPoint,
  assemblySchema,
  assemblySocketFrame,
  dot,
  sub,
  type Vec3,
} from "@wrela/model";

export function createAssemblyPart(id = "part-1"): AssemblyPart {
  return {
    id,
    name: "Beam",
    profile: { kind: "rectangle", width: 0.2, height: 0.2 },
    path: [
      [0, 0, 0],
      [0, 0, 2],
    ],
    bevel: 0.015,
    position: [0, 0, 0],
    rotation: [0, 0, 0],
    repeat: { count: 1, offset: [1, 0, 0] },
    sockets: [
      { id: "start", position: [0, 0, 0], anchor: { point: "start", profileOffset: [0, 0] } },
      { id: "end", position: [0, 0, 0], anchor: { point: "end", profileOffset: [0, 0] } },
    ],
    wear: { amount: 0, scale: 1, seed: 1 },
  };
}
export function createDoorwayAssembly(width = 1, height = 2.1, thickness = 0.15): AssemblyDefinition {
  const beam = (id: string, position: Vec3, end: Vec3): AssemblyPart => ({
    ...createAssemblyPart(id),
    name: id,
    profile: { kind: "rectangle", width: thickness, height: thickness },
    bevel: 0,
    position,
    path: [[0, 0, 0], end],
  });
  return assemblySchema.parse({
    grid: 0.05,
    parts: [
      beam("left-post", [-width / 2 - thickness / 2, 0, 0], [0, height, 0]),
      beam("right-post", [width / 2 + thickness / 2, 0, 0], [0, height, 0]),
      beam("lintel", [-width / 2 - thickness, height + thickness / 2, 0], [width + thickness * 2, 0, 0]),
    ],
    clearances: [
      {
        id: "door-clearance",
        name: `${width}m × ${height}m doorway`,
        min: [-width / 2, 0, -thickness / 2],
        max: [width / 2, height, thickness / 2],
      },
    ],
  });
}
export function snapAssemblyPartToGrid(source: AssemblyDefinition, id: string): AssemblyDefinition {
  const assembly = structuredClone(source),
    part = assembly.parts.find((p) => p.id === id);
  if (!part) throw new Error(`Unknown assembly part ${id}`);
  if (part.mate) throw new Error("Detach the socket mate before grid snapping");
  part.position = part.position.map((v) => Math.round(v / assembly.grid) * assembly.grid) as Vec3;
  return assemblySchema.parse(assembly);
}
/** Socket snapping preserves authored orientation; sockets specify connection points. */
export function snapAssemblySockets(
  source: AssemblyDefinition,
  movingId: string,
  movingSocket: string,
  targetId: string,
  targetSocket: string,
): AssemblyDefinition {
  const assembly = assemblySchema.parse(structuredClone(source)),
    moving = assembly.parts.find((p) => p.id === movingId),
    target = assembly.parts.find((p) => p.id === targetId);
  if (!moving || !target || moving === target) throw new Error("Socket snapping needs two distinct parts");
  // Moving an ancestor of the target would move both endpoints and cannot solve alignment.
  let ancestor = target.mate?.part ?? target.parent;
  while (ancestor) {
    if (ancestor === moving.id) throw new Error("Cannot snap an ancestor to its descendant");
    const dependency = assembly.parts.find((p) => p.id === ancestor);
    ancestor = dependency?.mate?.part ?? dependency?.parent;
  }
  if (moving.mate) throw new Error("Detach the socket mate before one-time snapping");
  const a = moving.sockets.find((s) => s.id === movingSocket),
    b = target.sockets.find((s) => s.id === targetSocket);
  if (!a || !b) throw new Error("Unknown socket");
  let delta = sub(
    assemblyPartPoint(assembly, target, assemblySocketFrame(target, b).origin),
    assemblyPartPoint(assembly, moving, assemblySocketFrame(moving, a).origin),
  );
  if (moving.parent) {
    const parent = assembly.parts.find((p) => p.id === moving.parent);
    if (!parent) throw new Error("Missing parent");
    const origin = assemblyPartPoint(assembly, parent, [0, 0, 0]);
    const axes = [0, 1, 2].map((axis) => {
      const p: Vec3 = [0, 0, 0];
      p[axis] = 1;
      return sub(assemblyPartPoint(assembly, parent, p), origin);
    });
    delta = axes.map((axis) => dot(axis, delta)) as Vec3;
  }
  moving.position = moving.position.map((v, axis) => v + delta[axis]) as Vec3;
  return assemblySchema.parse(assembly);
}

/** A fixed mate stays attached to the chosen socket frame through subsequent edits. */
export function mateAssemblySockets(
  source: AssemblyDefinition,
  movingId: string,
  ownSocket: string,
  targetId: string,
  socket: string,
  module = 0,
): AssemblyDefinition {
  const assembly = structuredClone(source),
    moving = assembly.parts.find((p) => p.id === movingId);
  if (!moving) throw new Error(`Unknown part ${movingId}`);
  if (moving.joint)
    throw new Error(
      "Attach a fixed connector and articulate its child, or remove this part's joint before mating",
    );
  moving.parent = undefined;
  moving.mate = { part: targetId, socket, ownSocket, module };
  return assemblySchema.parse(assembly);
}
/** Bake the current rest pose when detaching, preserving the visible assembly. */
export function detachAssemblyMate(source: AssemblyDefinition, id: string): AssemblyDefinition {
  const assembly = structuredClone(source),
    part = assembly.parts.find((p) => p.id === id);
  if (!part) throw new Error(`Unknown part ${id}`);
  if (!part.mate) return assembly;
  const origin = assemblyPartPoint(assembly, part, [0, 0, 0]);
  const axes = [0, 1, 2].map((axis) => {
    const p: Vec3 = [0, 0, 0];
    p[axis] = 1;
    return sub(assemblyPartPoint(assembly, part, p), origin);
  });
  const y = Math.asin(Math.max(-1, Math.min(1, -axes[0][2])));
  part.rotation =
    Math.abs(Math.cos(y)) > 1e-6
      ? [Math.atan2(axes[1][2], axes[2][2]), y, Math.atan2(axes[0][1], axes[0][0])]
      : [0, y, Math.atan2(-axes[1][0], axes[1][1])];
  part.position = origin;
  part.mate = undefined;
  return assemblySchema.parse(assembly);
}
