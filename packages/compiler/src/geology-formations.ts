import {
  buildGeologicalRockNodes,
  contentKey,
  type FieldNode,
  type GeologyFormation,
  type ObjectDefinition,
  type TerrainDefinition,
  type Vec3,
} from "@wrela/model";

/** Stable bounded IDs avoid name/ID length limits and regenerate in place. */
export function geologyObjectId(terrain: TerrainDefinition, formation: GeologyFormation): string {
  return `geology-${contentKey([terrain.id, formation.id])}`;
}
export function geologyGeneratorId(terrain: TerrainDefinition): string {
  return `geology-${contentKey(terrain.id)}`;
}
function node(id: string, kind: FieldNode["kind"], position: Vec3, size: Vec3): FieldNode {
  return { id, name: id, kind, position, size, rotation: [0, 0, 0], radius: 1, blend: 0, children: [] };
}

/** A broad, noise-free polygonal tunnel avoids the near-tangent oscillations
 * that produced thin lips when subtracting one fractured field from another.
 * Expanding each support plane creates a protected wall around its cavity. */
function openingNodes(formation: GeologyFormation, margin = 0): FieldNode[] {
  const [width, height, depth] = formation.size;
  const prefix = margin > 0 ? "opening-protection" : "opening";
  const center: Vec3 = [0, height * formation.opening * 0.2, 0];
  if (formation.kind === "overhang")
    return [
      node(
        prefix,
        "box",
        [0, (height * formation.opening) / 2 - 0.1, depth * 0.25],
        [width * 0.65 + margin, (height * formation.opening) / 2 + margin, depth * 0.65 + margin],
      ),
    ];
  if (!formation.rock)
    return [
      node(prefix, "ellipsoid", center, [
        (width * formation.opening) / 2 + margin,
        height * formation.opening * 0.5 + margin,
        depth * 1.5 + margin,
      ]),
    ];
  const halfWidth = (width * formation.opening) / 2,
    halfHeight = height * formation.opening * 0.5;
  const body = node(`${prefix}-center`, "box", center, [
    halfWidth + margin,
    halfHeight + margin,
    depth * 2 + margin,
  ]);
  const nodes = [body];
  for (const side of [-1, 1]) {
    const plane = node(`${prefix}-roof-${side === -1 ? "left" : "right"}`, "box", center, [
      halfWidth * 1.1 + margin,
      halfHeight * 1.1 + margin,
      depth * 2 + margin,
    ]);
    plane.rotation = [0, 0, (side * Math.PI) / 8];
    nodes.push(plane);
  }
  const opening = node(prefix, "intersect", [0, 0, 0], [1, 1, 1]);
  opening.children = nodes.map((piece) => piece.id);
  return [...nodes, opening];
}

/** A finite outcrop with actual CSG cavities and the same collision surface.
 * The intact heightfield underneath provides its floor. */
export function buildGeologyObject(
  terrain: TerrainDefinition,
  formation: GeologyFormation,
): ObjectDefinition {
  const [width, height, depth] = formation.size;
  const body = node("outcrop", "box", [0, height / 2, 0], [width / 2, height / 2, depth / 2]);
  const opening = openingNodes(formation);
  const spacing = Math.max(...formation.size) / formation.resolution;
  const protection = openingNodes(formation, spacing * 3);
  const exterior = formation.rock
    ? buildGeologicalRockNodes(formation.size, formation.rock, spacing * 1.5, {
        root: protection[protection.length - 1].id,
        nodes: protection,
      })
    : [body];
  const root = node("formation", "subtract", [0, 0, 0], [1, 1, 1]);
  root.children = [exterior[exterior.length - 1].id, opening[opening.length - 1].id];
  const heading = formation.heading ?? 0;
  root.rotation = [0, heading, 0];
  const padding = (Math.max(...formation.size) / formation.resolution) * 1.5;
  const extentX = (Math.abs(Math.cos(heading)) * width + Math.abs(Math.sin(heading)) * depth) / 2;
  const extentZ = (Math.abs(Math.sin(heading)) * width + Math.abs(Math.cos(heading)) * depth) / 2;
  return {
    id: geologyObjectId(terrain, formation),
    name: `${terrain.name} ${formation.kind} ${formation.id}`.slice(0, 120),
    schemaVersion: 1,
    kind: "object",
    dependencies: [formation.material ?? terrain.material],
    material: formation.material ?? terrain.material,
    generated: { generator: geologyGeneratorId(terrain), policy: "detached" },
    collision: "mesh",
    field: {
      root: root.id,
      nodes: [...exterior, ...opening, root],
      resolution: formation.resolution,
      bounds: {
        min: [-extentX - padding, -padding, -extentZ - padding],
        max: [extentX + padding, height + padding, extentZ + padding],
      },
    },
  };
}
export function buildGeologyObjects(terrain: TerrainDefinition): ObjectDefinition[] {
  return terrain.geology?.formations.map((formation) => buildGeologyObject(terrain, formation)) ?? [];
}
