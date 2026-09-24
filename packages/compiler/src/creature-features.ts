import {
  add,
  type CharacterDefinition,
  contentKey,
  type Diagnostic,
  type FieldNode,
  type MeshData,
  type Quality,
  type Vec3,
} from "@wrela/model";

import { creatureRotate } from "./creature";
import { cachedCreatureProduct } from "./creature-cache";
import { parametricPrimitive } from "./primitive";

export type CreatureFeatureCompilation = {
  document: CharacterDefinition;
  empty: boolean;
  meshes: MeshData[];
  diagnostics: Diagnostic[];
  /** Overlapping closed opaque components realize exterior hard-union visibility.
   * They are not a manifold volume, and cannot be used as signed-distance truth. */
  domain: "opaque-exterior-hard-union";
};
export function emptyCreatureMesh(): MeshData {
  return {
    positions: new Float32Array(),
    normals: new Float32Array(),
    indices: new Uint32Array(),
    sourceIds: [],
    materialGroups: [],
    bounds: { min: [0, 0, 0], max: [0, 0, 0] },
  };
}

export function prepareCreatureFeatures(
  source: CharacterDefinition,
  quality: Quality = "review",
  maxVertices = 80_000,
): CreatureFeatureCompilation {
  if (!source.creature) return buildCreatureFeatures(source, quality, maxVertices);
  const key = contentKey({
    version: 1,
    quality,
    maxVertices,
    field: {
      ...source.field,
      nodes: source.field.nodes.map(({ name: _name, material: _material, ...node }) => node),
    },
  });
  const cached = cachedCreatureProduct("features", key, () => {
    const product = buildCreatureFeatures(source, quality, maxVertices);
    return {
      field: product.document.field,
      empty: product.empty,
      meshes: product.meshes,
      diagnostics: product.diagnostics,
      domain: product.domain,
    };
  });
  const materials = new Map(source.field.nodes.map((node) => [node.id, node]));
  // Material edits rebind source assignments without tessellating any primitive.
  cached.field.nodes = cached.field.nodes.map((node) => ({
    ...node,
    name: materials.get(node.id)?.name ?? node.name,
    material: materials.get(node.id)?.material,
  }));
  for (const mesh of cached.meshes)
    for (const group of mesh.materialGroups ?? [])
      group.material = materials.get(mesh.sourceIds?.[0] ?? "")?.material ?? source.material;
  return {
    document: cached.meshes.length ? { ...source, field: cached.field } : source,
    empty: cached.empty,
    meshes: cached.meshes,
    diagnostics: cached.diagnostics,
    domain: cached.domain,
  };
}

/** Preserve authored small features without raising the body's global grid.
 * Only primitive leaves reachable exclusively through exact hard unions qualify.
 * Smooth blends, intersections, subtraction, clipping and shared DAG leaves retain
 * their established extraction route and cannot silently become separate objects. */
function buildCreatureFeatures(
  source: CharacterDefinition,
  quality: Quality = "review",
  maxVertices = 80_000,
): CreatureFeatureCompilation {
  if (!Number.isInteger(maxVertices) || maxVertices < 0 || maxVertices > 250_000)
    throw new Error("Feature vertex budget must be an integer in [0,250000]");
  const result: CreatureFeatureCompilation = {
    document: source,
    empty: false,
    meshes: [],
    diagnostics: [],
    domain: "opaque-exterior-hard-union",
  };
  if (!source.creature) return result;
  const nodes = new Map(source.field.nodes.map((node) => [node.id, node]));
  const instances = new Map<string, number>();
  const candidates: { node: FieldNode; parents: FieldNode[] }[] = [];
  let work = 0;
  const walk = (id: string, parents: FieldNode[], eligible: boolean) => {
    if (++work > 8192 || parents.length > 128)
      throw new Error("Creature feature analysis exceeds bounded field traversal");
    const node = nodes.get(id);
    if (!node || parents.some((parent) => parent.id === id))
      throw new Error(`Invalid creature field ancestry at ${id}`);
    if (node.size.some((value) => !Number.isFinite(value) || value <= 0))
      throw new Error(`Shape size must be positive at ${id}`);
    instances.set(id, (instances.get(id) ?? 0) + 1);
    if (
      eligible &&
      parents.length &&
      node.children.length === 0 &&
      (node.kind === "sphere" || node.kind === "ellipsoid")
    )
      candidates.push({ node, parents });
    for (const child of node.children) walk(child, [...parents, node], eligible && node.kind === "union");
  };
  walk(source.field.root, [], true);
  const selected = new Set<string>();
  let used = 0;
  const rings = quality === "interactive" ? 8 : quality === "review" ? 12 : 20;
  for (const { node, parents } of candidates) {
    if (instances.get(node.id) !== 1) continue;
    const radii: Vec3 = node.kind === "sphere" ? [node.radius, node.radius, node.radius] : [...node.size];
    const { mesh, surfaceError } = parametricPrimitive(
      {
        center: [...node.position],
        rotation: [...node.rotation],
        radii,
        nodeId: node.id,
        material: node.material,
      },
      rings,
    );
    // The rotated ellipsoid's exact AABB is derived from its three radius axes.
    let center: Vec3 = [...node.position];
    const axes: Vec3[] = [
      [radii[0], 0, 0],
      [0, radii[1], 0],
      [0, 0, radii[2]],
    ].map((axis) => creatureRotate(axis as Vec3, node.rotation));
    for (const parent of [...parents].reverse()) {
      center = add(parent.position, creatureRotate(center, parent.rotation));
      for (let i = 0; i < 3; i++) axes[i] = creatureRotate(axes[i], parent.rotation);
    }
    const min: Vec3 = [0, 0, 0],
      max: Vec3 = [0, 0, 0];
    let clipped = false;
    for (let axis = 0; axis < 3; axis++) {
      const extent = Math.hypot(...axes.map((v) => v[axis]));
      min[axis] = center[axis] - extent;
      max[axis] = center[axis] + extent;
      if (
        min[axis] < source.field.bounds.min[axis] - 1e-7 ||
        max[axis] > source.field.bounds.max[axis] + 1e-7
      )
        clipped = true;
    }
    if (clipped) {
      result.diagnostics.push({
        severity: "info",
        code: "creature.feature.clipped-fallback",
        node: node.id,
        message:
          "Feature intersects authored extraction bounds; retained the clipping-aware field realization.",
      });
      continue;
    }
    if (used + mesh.positions.length / 3 > maxVertices) {
      result.diagnostics.push({
        severity: "warning",
        code: "creature.feature.budget",
        node: node.id,
        message:
          "Local feature vertex budget exhausted; retained field extraction with its declared feature limits.",
      });
      continue;
    }
    for (let i = 0; i < mesh.positions.length; i += 3) {
      let p: Vec3 = [mesh.positions[i], mesh.positions[i + 1], mesh.positions[i + 2]],
        n: Vec3 = [mesh.normals[i], mesh.normals[i + 1], mesh.normals[i + 2]];
      for (const parent of [...parents].reverse()) {
        p = add(parent.position, creatureRotate(p, parent.rotation));
        n = creatureRotate(n, parent.rotation);
      }
      mesh.positions.set(p, i);
      mesh.normals.set(n, i);
    }
    mesh.bounds = { min, max };
    mesh.materialGroups = [
      { material: node.material ?? source.material, start: 0, count: mesh.indices.length },
    ];
    mesh.fidelity = {
      sampleSpacing: [0, 0, 0],
      maxError: surfaceError,
      unresolvedFeatures: [
        "Opaque exterior union component; not a fused volume or inside-surface guarantee.",
      ],
    };
    result.meshes.push(mesh);
    selected.add(node.id);
    used += mesh.positions.length / 3;
  }
  if (!selected.size) return result;
  const document = structuredClone(source),
    copy = new Map(document.field.nodes.map((node) => [node.id, node]));
  const prune = (id: string): string | undefined => {
    if (selected.has(id)) return undefined;
    const node = copy.get(id);
    if (!node) throw new Error(`Missing feature field node ${id}`);
    if (node.kind === "union") {
      node.children = node.children.map(prune).filter((child): child is string => child !== undefined);
      if (!node.children.length) return undefined;
    }
    return id;
  };
  const root = prune(document.field.root);
  result.empty = root === undefined;
  if (root) {
    document.field.root = root;
    document.field.nodes = document.field.nodes.filter((node) => !selected.has(node.id));
    result.document = document;
  }
  result.diagnostics.push({
    severity: "info",
    code: "creature.feature.local-realization",
    message: `Preserved ${selected.size} independent hard-union features with ${used} analytic-surface vertices. Valid for opaque exterior visibility; components remain separate closed meshes, not a fused manifold or inside-volume representation.`,
  });
  return result;
}

/** Stable concatenation followed by one range per material prevents repeated
 * surface identities and redundant draws from independently realized features. */
export function mergeCreatureFeatureMeshes(
  base: MeshData,
  features: MeshData[],
  defaultMaterial: string,
): MeshData {
  if (!features.length) return base;
  const meshes = [base, ...features],
    vertexCount = meshes.reduce((sum, mesh) => sum + mesh.positions.length / 3, 0);
  if (vertexCount > 250_000) throw new Error("Creature body and local features exceed 250000 vertices");
  const positions = new Float32Array(vertexCount * 3),
    normals = new Float32Array(vertexCount * 3),
    sourceIds: string[] = [],
    groups = new Map<string, number[]>();
  const colors = meshes.some((mesh) => mesh.colors) ? new Float32Array(vertexCount * 3).fill(1) : undefined;
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  let offset = 0;
  for (const mesh of meshes) {
    positions.set(mesh.positions, offset * 3);
    normals.set(mesh.normals, offset * 3);
    if (mesh.colors) colors?.set(mesh.colors, offset * 3);
    for (let i = 0; i < mesh.positions.length / 3; i++) sourceIds.push(mesh.sourceIds?.[i] ?? "legacy");
    for (const group of mesh.materialGroups ?? [
      { material: defaultMaterial, start: 0, count: mesh.indices.length },
    ]) {
      const bucket = groups.get(group.material) ?? [];
      for (let i = group.start; i < group.start + group.count; i++) bucket.push(mesh.indices[i] + offset);
      groups.set(group.material, bucket);
    }
    if (mesh.positions.length)
      for (let axis = 0; axis < 3; axis++) {
        min[axis] = Math.min(min[axis], mesh.bounds.min[axis]);
        max[axis] = Math.max(max[axis], mesh.bounds.max[axis]);
      }
    offset += mesh.positions.length / 3;
  }
  const count = [...groups.values()].reduce((sum, list) => sum + list.length, 0),
    indices = new Uint32Array(count),
    materialGroups: NonNullable<MeshData["materialGroups"]> = [];
  let start = 0;
  for (const [material, bucket] of groups) {
    indices.set(bucket, start);
    materialGroups.push({ material, start, count: bucket.length });
    start += bucket.length;
  }
  return { positions, normals, indices, sourceIds, materialGroups, colors, bounds: { min, max } };
}
