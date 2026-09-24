import {
  add,
  type CharacterDefinition,
  contentKey,
  type Diagnostic,
  dot,
  type FieldNode,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

import { creatureRegionPoint, creatureRotate, resolveCreatureAnchor } from "./creature";
import { cachedCreatureProduct } from "./creature-cache";

function inverseRotate(p: Vec3, rotation: Vec3): Vec3 {
  return [
    dot(p, creatureRotate([1, 0, 0], rotation)),
    dot(p, creatureRotate([0, 1, 0], rotation)),
    dot(p, creatureRotate([0, 0, 1], rotation)),
  ];
}
function primitiveSupport(node: FieldNode, normal: Vec3): number {
  switch (node.kind) {
    case "sphere":
      return node.radius;
    case "ellipsoid":
      return Math.hypot(...node.size.map((size, i) => size * normal[i]));
    case "box":
      return node.size.reduce((sum, size, i) => sum + size * Math.abs(normal[i]), 0);
    case "capsule":
      return node.radius + node.size[1] * Math.abs(normal[1]);
    case "torus":
      return node.radius + node.size[0] * Math.hypot(normal[0], normal[2]);
    default:
      throw new Error(
        `Attachment component ${node.id} must identify primitive leaves, not Boolean operations`,
      );
  }
}
export function prepareCreatureAttachments(source: CharacterDefinition): {
  document: CharacterDefinition;
  diagnostics: Diagnostic[];
} {
  const creature = source.creature;
  if (!creature?.attachments.length) return { document: source, diagnostics: [] };
  const key = contentKey({
    version: 2,
    field: {
      ...source.field,
      nodes: source.field.nodes.map(({ name: _name, material: _material, ...node }) => node),
    },
    regions: creature.regions.map((region) => ({ id: region.id, frame: region.frame })),
    charts: creature.charts.map(({ material: _material, ...chart }) => chart),
    anchors: creature.anchors,
    attachments: creature.attachments,
    sculpts: creature.sculpts,
  });
  const cached = cachedCreatureProduct("attachments", key, () => {
    const result = buildCreatureAttachments(source);
    return { field: result.document.field, diagnostics: result.diagnostics };
  });
  const current = new Map(source.field.nodes.map((node) => [node.id, node]));
  cached.field.nodes = cached.field.nodes.map((node) => ({
    ...node,
    name: current.get(node.id)?.name ?? node.name,
    material: current.get(node.id)?.material,
  }));
  return { document: { ...source, field: cached.field }, diagnostics: cached.diagnostics };
}

/** Mount primitive groups from source anchors before legacy field extraction.
 * This is a derived source view: authored primitive transforms are never mutated.
 * Parent transforms are respected, and DAG instances with ambiguous ownership fail. */
function buildCreatureAttachments(source: CharacterDefinition): {
  document: CharacterDefinition;
  diagnostics: Diagnostic[];
} {
  const creature = source.creature;
  if (!creature?.attachments.length) return { document: source, diagnostics: [] };
  const document = structuredClone(source),
    diagnostics: Diagnostic[] = [];
  const original = new Map(source.field.nodes.map((node) => [node.id, node]));
  const nodes = new Map(document.field.nodes.map((node) => [node.id, node]));
  const paths = new Map<string, FieldNode[][]>();
  let work = 0;
  const walk = (id: string, parents: FieldNode[]) => {
    if (++work > 8192 || parents.length > 128)
      throw new Error("Attachment ancestry exceeds bounded field traversal");
    const node = original.get(id);
    if (!node || parents.some((p) => p.id === id))
      throw new Error(`Invalid attachment field ancestry at ${id}`);
    const entries = paths.get(id) ?? [];
    entries.push(parents);
    paths.set(id, entries);
    for (const child of node.children) walk(child, [...parents, node]);
  };
  walk(source.field.root, []);
  const owned = new Set<string>();
  for (const attachment of creature.attachments) {
    const anchor = creature.anchors.find((a) => a.id === attachment.anchor);
    if (!anchor)
      throw new Error(`Attachment ${attachment.id} references missing anchor ${attachment.anchor}`);
    const resolved = resolveCreatureAnchor(creature, anchor);
    if (resolved.status !== "resolved" || !resolved.position || !resolved.normal)
      throw new Error(
        `Attachment ${attachment.id} cannot mount: ${resolved.diagnostics.map((d) => d.message).join(" ")}`,
      );
    const surfaceNormal = resolved.normal;
    const mounts = attachment.nodeIds.map((id) => {
      if (owned.has(id)) throw new Error(`Attachment node ${id} has multiple owners`);
      owned.add(id);
      const node = original.get(id),
        candidates = paths.get(id);
      if (!node || candidates?.length !== 1)
        throw new Error(`Attachment node ${id} has missing or ambiguous field ancestry`);
      primitiveSupport(node, [0, 1, 0]);
      const parents = candidates[0];
      let world: Vec3 = [...node.position];
      for (const parent of [...parents].reverse())
        world = add(parent.position, creatureRotate(world, parent.rotation));
      return { node, parents, world };
    });
    const center = scale(
      mounts.reduce((sum, mount) => add(sum, mount.world), [0, 0, 0] as Vec3),
      1 / mounts.length,
    );
    const regionOrigin = creatureRegionPoint(creature, anchor.region, [0, 0, 0]);
    const offset = sub(creatureRegionPoint(creature, anchor.region, attachment.offset), regionOrigin);
    // Surface seating uses actual primitive support in the resolved normal,
    // including all rotated field ancestors, while preserving component size.
    // The legacy center placement remains available for existing authored work.
    const support = (mount: (typeof mounts)[number]) => {
      let normal: Vec3 = [...surfaceNormal];
      for (const parent of mount.parents) normal = inverseRotate(normal, parent.rotation);
      return primitiveSupport(mount.node, inverseRotate(normal, mount.node.rotation));
    };
    const seating =
      attachment.placement === "surface"
        ? Math.max(...mounts.map((mount) => support(mount) - dot(sub(mount.world, center), surfaceNormal))) +
          attachment.minimumClearance
        : 0;
    const target = add(add(resolved.position, offset), scale(resolved.normal, seating)),
      delta = sub(target, center);
    let clearance = Infinity;
    for (const mount of mounts) {
      const world = add(mount.world, delta);
      let local: Vec3 = world,
        normal: Vec3 = [...resolved.normal];
      for (const parent of mount.parents) {
        local = inverseRotate(sub(local, parent.position), parent.rotation);
        normal = inverseRotate(normal, parent.rotation);
      }
      normal = inverseRotate(normal, mount.node.rotation);
      const support = primitiveSupport(mount.node, normal);
      clearance = Math.min(
        clearance,
        dot(sub(world, sub(resolved.position, scale(resolved.normal, anchor.offset))), resolved.normal) -
          support,
      );
      const destination = nodes.get(mount.node.id);
      if (!destination) throw new Error("Lost attachment node during preparation");
      destination.position = local;
      // Expand extraction bounds conservatively around mounted atomic support.
      const radius =
        Math.max(
          primitiveSupport(mount.node, [1, 0, 0]),
          primitiveSupport(mount.node, [0, 1, 0]),
          primitiveSupport(mount.node, [0, 0, 1]),
        ) * Math.sqrt(3);
      for (let axis = 0; axis < 3; axis++) {
        document.field.bounds.min[axis] = Math.min(
          document.field.bounds.min[axis],
          world[axis] - radius - 0.001,
        );
        document.field.bounds.max[axis] = Math.max(
          document.field.bounds.max[axis],
          world[axis] + radius + 0.001,
        );
      }
    }
    diagnostics.push({
      severity: clearance + 1e-8 < attachment.minimumClearance ? "error" : "info",
      code:
        clearance + 1e-8 < attachment.minimumClearance
          ? "creature.attachment.clearance"
          : "creature.attachment.mounted",
      node: attachment.id,
      message: `Mounted ${attachment.nodeIds.length} component(s) at anchor ${anchor.id}; tangent-plane clearance ${clearance.toFixed(5)} m (required ${attachment.minimumClearance.toFixed(5)} m). This local support test does not certify clearance against the entire body.`,
    });
  }
  return { document, diagnostics };
}
