import {
  type CharacterDefinition,
  type CompiledCharacter,
  type CompiledSurface,
  contentKey,
  cross,
  type Diagnostic,
  dot,
  type FieldDefinition,
  type MeshData,
  type ObjectDefinition,
  type Quality,
  sub,
  type Vec3,
} from "@wrela/model";
import { ProductCache } from "./cache";
import { compileField } from "./field";
import { bindingKey, geometryKey, materialBindingKey, productKeys } from "./products";
import { COMPILER_VERSION } from "./versions";

export { COMPILER_VERSION } from "./versions";

const geometryProducts = new ProductCache<{ mesh: MeshData; diagnostics: Diagnostic[] }>(16 * 1024 * 1024);
const materialProducts = new ProductCache<{
  indices: Uint32Array;
  materialGroups: MeshData["materialGroups"];
}>(4 * 1024 * 1024);
const bindingProducts = new ProductCache<{
  weights: Float32Array;
  jointIndices: Uint16Array;
  unbound: number;
}>(8 * 1024 * 1024);
export function compilerCacheMetrics() {
  return {
    geometry: geometryProducts.metrics,
    material: materialProducts.metrics,
    binding: bindingProducts.metrics,
  };
}
export function clearCompilerCaches() {
  geometryProducts.clear();
  materialProducts.clear();
  bindingProducts.clear();
}
export function surfaceResolution(field: FieldDefinition, quality: Quality): number {
  return Math.max(
    12,
    Math.min(field.resolution, quality === "interactive" ? 24 : quality === "review" ? 48 : 80),
  );
}
/** A conforming six-tetrahedron subdivision shares the same face diagonals in
 * every cell. Edge vertices are welded across cells and source provenance is
 * retained per vertex. No claim is made that an implicit value is a distance. */
export function extractSurface(
  field: FieldDefinition,
  quality: Quality = "review",
  defaultMaterial?: string,
): { mesh: MeshData; diagnostics: Diagnostic[] } {
  const program = compileField(field),
    n = surfaceResolution(field, quality),
    row = n + 1,
    plane = row * row;
  const { min, max } = field.bounds;
  if (min.some((v, i) => !Number.isFinite(v) || !Number.isFinite(max[i]) || max[i] <= v))
    throw new Error("Field extraction bounds must have positive finite extent");
  const step: Vec3 = [(max[0] - min[0]) / n, (max[1] - min[1]) / n, (max[2] - min[2]) / n];
  const sampleDiagonal = Math.hypot(...step);
  const values = new Float32Array(row ** 3),
    points = (id: number): Vec3 => [
      min[0] + (id % row) * step[0],
      min[1] + (Math.floor(id / row) % row) * step[1],
      min[2] + Math.floor(id / plane) * step[2],
    ];
  let clipped = false;
  for (let z = 0; z <= n; z++)
    for (let y = 0; y <= n; y++)
      for (let x = 0; x <= n; x++) {
        const id = z * plane + y * row + x,
          value = program.distance(points(id));
        if (!Number.isFinite(value)) throw new Error("Field evaluator produced a non-finite value");
        values[id] = value;
        if (value < 0 && (x === 0 || x === n || y === 0 || y === n || z === 0 || z === n)) clipped = true;
      }
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [],
    sourceIds: string[] = [],
    edges = new Map<number, number>();
  const epsilon = Math.max(1e-5, Math.min(...step) * 0.015),
    count = values.length;
  const vertex = (a: number, b: number): number => {
    const lo = Math.min(a, b),
      hi = Math.max(a, b),
      key = values[a] === 0 ? count * count + a : values[b] === 0 ? count * count + b : lo * count + hi,
      cached = edges.get(key);
    if (cached !== undefined) return cached;
    const va = values[a],
      vb = values[b],
      t = Math.abs(va - vb) > 1e-20 ? va / (va - vb) : 0.5,
      pa = points(a),
      pb = points(b);
    const p: Vec3 = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t, pa[2] + (pb[2] - pa[2]) * t];
    const id = positions.length / 3;
    if (id >= 250_000)
      throw new Error("Surface exceeds the 250,000 vertex budget; reduce resolution or simplify the field");
    positions.push(...p);
    normals.push(...program.normal(p, epsilon));
    sourceIds.push(program.sample(p).source);
    edges.set(key, id);
    return id;
  };
  const at = (a: number): Vec3 => [positions[a * 3], positions[a * 3 + 1], positions[a * 3 + 2]];
  const triangle = (a: number, b: number, c: number) => {
    const normal = cross(sub(at(b), at(a)), sub(at(c), at(a)));
    if (dot(normal, normal) < 1e-20) return;
    const expected: Vec3 = [
      normals[a * 3] + normals[b * 3] + normals[c * 3],
      normals[a * 3 + 1] + normals[b * 3 + 1] + normals[c * 3 + 1],
      normals[a * 3 + 2] + normals[b * 3 + 2] + normals[c * 3 + 2],
    ];
    if (indices.length >= 1_500_000)
      throw new Error("Surface exceeds the 500,000 triangle budget; reduce resolution or simplify the field");
    if (dot(normal, expected) < 0) indices.push(a, c, b);
    else indices.push(a, b, c);
  };
  const tetrahedra = [
    [0, 5, 1, 6],
    [0, 1, 2, 6],
    [0, 2, 3, 6],
    [0, 3, 7, 6],
    [0, 7, 4, 6],
    [0, 4, 5, 6],
  ];
  for (let z = 0; z < n; z++)
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++) {
        const a = z * plane + y * row + x,
          corners = [
            a,
            a + 1,
            a + row + 1,
            a + row,
            a + plane,
            a + plane + 1,
            a + plane + row + 1,
            a + plane + row,
          ];
        if (corners.every((id) => values[id] >= 0) || corners.every((id) => values[id] < 0)) continue;
        for (const tet of tetrahedra) {
          const ids = tet.map((i) => corners[i]),
            inside = ids.filter((i) => values[i] < 0),
            outside = ids.filter((i) => values[i] >= 0);
          if (inside.length === 0 || inside.length === 4) continue;
          if (inside.length === 1)
            triangle(
              vertex(inside[0], outside[0]),
              vertex(inside[0], outside[1]),
              vertex(inside[0], outside[2]),
            );
          else if (inside.length === 3)
            triangle(
              vertex(outside[0], inside[0]),
              vertex(outside[0], inside[1]),
              vertex(outside[0], inside[2]),
            );
          else {
            const a = vertex(inside[0], outside[0]),
              b = vertex(inside[0], outside[1]),
              c = vertex(inside[1], outside[0]),
              d = vertex(inside[1], outside[1]);
            triangle(a, b, c);
            triangle(b, d, c);
          }
        }
      }
  let materialGroups: MeshData["materialGroups"];
  if (defaultMaterial && field.nodes.some((node) => node.material)) {
    const groups = new Map<string, number[]>();
    for (let index = 0; index < indices.length; index += 3) {
      const a = at(indices[index]),
        b = at(indices[index + 1]),
        c = at(indices[index + 2]);
      const center: Vec3 = [(a[0] + b[0] + c[0]) / 3, (a[1] + b[1] + c[1]) / 3, (a[2] + b[2] + c[2]) / 3];
      const material = program.sample(center).material ?? defaultMaterial;
      let group = groups.get(material);
      if (!group) {
        group = [];
        groups.set(material, group);
      }
      group.push(indices[index], indices[index + 1], indices[index + 2]);
    }
    indices.length = 0;
    materialGroups = [];
    for (const [material, group] of groups) {
      materialGroups.push({ material, start: indices.length, count: group.length });
      for (const index of group) indices.push(index);
    }
  }
  const diagnostics: Diagnostic[] = [];
  const unresolvedFeatures = program.ir.instructions
    .filter(
      (instruction) =>
        instruction.minimumFeatureSize !== null && instruction.minimumFeatureSize < sampleDiagonal,
    )
    .map((instruction) => instruction.id);
  for (const id of unresolvedFeatures)
    diagnostics.push({
      severity: field.fidelity?.strict ? "error" : "warning",
      code: "field.feature-undersampled",
      message: `Feature ${id} is smaller than the ${sampleDiagonal.toPrecision(3)} metre sampling diagonal and may disappear or change shape. Tighten bounds, enlarge the feature, or increase resolution.`,
      node: id,
    });
  if (field.fidelity?.minimumFeatureSize !== undefined && sampleDiagonal > field.fidelity.minimumFeatureSize)
    diagnostics.push({
      severity: field.fidelity.strict ? "error" : "warning",
      code: "field.feature-contract",
      message: `Sampling diagonal ${sampleDiagonal.toPrecision(3)} exceeds the requested ${field.fidelity.minimumFeatureSize} metre minimum feature size.`,
      node: field.root,
    });
  if (field.fidelity?.maxError !== undefined)
    diagnostics.push({
      severity: field.fidelity.strict ? "error" : "warning",
      code: "field.error-unproven",
      message: `This implicit extraction cannot certify the requested ${field.fidelity.maxError} metre surface error. Its sampling diagonal is ${sampleDiagonal.toPrecision(3)} metres; this is not a geometric error bound.`,
      node: field.root,
    });
  if (field.fidelity?.strict && diagnostics.some((diagnostic) => diagnostic.severity === "error"))
    throw new Error(diagnostics.map((diagnostic) => diagnostic.message).join("\n"));
  if (clipped)
    diagnostics.push({
      severity: "warning",
      code: "field.clipped",
      message:
        "The field intersects its extraction boundary. Enlarge the authored bounds to close the surface.",
      node: field.root,
    });
  if (!indices.length)
    diagnostics.push({
      severity: "warning",
      code: "field.empty",
      message: "No surface was found within the extraction bounds.",
      node: field.root,
    });
  return {
    mesh: {
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      indices: new Uint32Array(indices),
      sourceIds,
      materialGroups,
      bounds: structuredClone(field.bounds),
      fidelity: {
        sampleSpacing: step,
        requestedMaxError: field.fidelity?.maxError,
        maxError: null,
        unresolvedFeatures,
      },
    },
    diagnostics,
  };
}
export function surfaceGeometrySource(
  doc: ObjectDefinition | CharacterDefinition,
  quality: Quality,
): unknown {
  return {
    compiler: COMPILER_VERSION,
    id: doc.id,
    kind: doc.kind,
    quality,
    ...productKeys(doc, quality),
    ...(doc.kind === "character" ? { jointLabels: doc.joints.map((joint) => [joint.id, joint.name]) } : {}),
  };
}
function surfaceProduct(doc: ObjectDefinition | CharacterDefinition, quality: Quality) {
  const key = geometryKey(doc, quality);
  let geometry = geometryProducts.get(key);
  if (!geometry) {
    geometry = extractSurface(doc.field, quality);
    const mesh = geometry.mesh;
    geometryProducts.set(
      key,
      geometry,
      mesh.positions.byteLength +
        mesh.normals.byteLength +
        mesh.indices.byteLength +
        (mesh.sourceIds?.reduce((sum, id) => sum + id.length * 2 + 8, 0) ?? 0),
    );
  }
  const diagnostics = geometry.diagnostics.map((diagnostic) => ({ ...diagnostic, document: doc.id }));
  if (!doc.field.nodes.some((node) => node.material)) return { mesh: geometry.mesh, diagnostics };
  const assignmentKey = `${key}:${materialBindingKey(doc)}`;
  let assignment = materialProducts.get(assignmentKey);
  if (!assignment) {
    const program = compileField(doc.field),
      mesh = geometry.mesh,
      groups = new Map<string, number[]>();
    for (let i = 0; i < mesh.indices.length; i += 3) {
      const center: Vec3 = [0, 0, 0];
      for (let corner = 0; corner < 3; corner++)
        for (let axis = 0; axis < 3; axis++)
          center[axis] += mesh.positions[mesh.indices[i + corner] * 3 + axis] / 3;
      const material = program.sample(center).material ?? doc.material;
      const group = groups.get(material) ?? [];
      if (!groups.has(material)) groups.set(material, group);
      group.push(mesh.indices[i], mesh.indices[i + 1], mesh.indices[i + 2]);
    }
    const indices = new Uint32Array(mesh.indices.length),
      materialGroups: NonNullable<MeshData["materialGroups"]> = [];
    let offset = 0;
    for (const [material, group] of groups) {
      indices.set(group, offset);
      materialGroups.push({ material, start: offset, count: group.length });
      offset += group.length;
    }
    assignment = { indices, materialGroups };
    materialProducts.set(assignmentKey, assignment, indices.byteLength);
  }
  return { mesh: { ...geometry.mesh, ...assignment }, diagnostics };
}
export function compileSurface(doc: ObjectDefinition, quality: Quality = "review"): CompiledSurface {
  const { mesh, diagnostics } = surfaceProduct(doc, quality);
  return {
    kind: "surface",
    id: doc.id,
    key: contentKey(surfaceGeometrySource(doc, quality)),
    mesh,
    material: doc.material,
    diagnostics: diagnostics.map((d) => ({ ...d, document: doc.id })),
  };
}
/** Rest joint positions are model-space, rotations are local Euler offsets.
 * Envelopes cover the segment from each joint to its parent; four strongest
 * smooth compact influences are normalized. Outside envelopes bind to nearest
 * bone with a diagnostic, preserving finite, usable deformation. */
export function compileCharacter(doc: CharacterDefinition, quality: Quality = "review"): CompiledCharacter {
  const { mesh, diagnostics } = surfaceProduct(doc, quality),
    count = mesh.positions.length / 3;
  const key = bindingKey(doc, quality);
  let binding = bindingProducts.get(key);
  if (!binding) {
    binding = bindCharacter(doc, mesh);
    bindingProducts.set(key, binding, binding.weights.byteLength + binding.jointIndices.byteLength);
  }
  const { weights, jointIndices, unbound } = binding;
  if (unbound)
    diagnostics.push({
      severity: "warning",
      code: "binding.outside-envelope",
      message: `${unbound} of ${count} vertices lie outside all influence envelopes and use the nearest bone. Enlarge the envelopes or refine the rig.`,
      document: doc.id,
    });
  return {
    kind: "character",
    id: doc.id,
    key: contentKey(surfaceGeometrySource(doc, quality)),
    mesh,
    material: doc.material,
    joints: structuredClone(doc.joints),
    motions: structuredClone(doc.motions),
    jointIndices,
    weights,
    diagnostics,
  };
}
function bindCharacter(doc: CharacterDefinition, mesh: MeshData) {
  const count = mesh.positions.length / 3,
    weights = new Float32Array(count * 4),
    jointIndices = new Uint16Array(count * 4),
    joints = new Map(doc.joints.map((j) => [j.id, j]));
  let unbound = 0;
  for (let v = 0; v < count; v++) {
    const p: Vec3 = [mesh.positions[v * 3], mesh.positions[v * 3 + 1], mesh.positions[v * 3 + 2]];
    const candidates = doc.joints
      .map((joint, index) => {
        const parent = joint.parent ? joints.get(joint.parent) : undefined,
          a = parent?.position ?? joint.position,
          b = joint.position,
          ab = sub(b, a),
          ap = sub(p, a),
          t = Math.max(0, Math.min(1, dot(ap, ab) / (dot(ab, ab) || 1))),
          distance = Math.hypot(p[0] - a[0] - ab[0] * t, p[1] - a[1] - ab[1] * t, p[2] - a[2] - ab[2] * t),
          radius = parent ? parent.radius * (1 - t) + joint.radius * t : joint.radius;
        const normalized = distance / radius;
        return { index, distance: normalized, weight: Math.max(0, 1 - normalized) ** 2 };
      })
      .sort((a, b) => b.weight - a.weight || a.distance - b.distance || a.index - b.index)
      .slice(0, 4);
    let sum = candidates.reduce((s, c) => s + c.weight, 0);
    if (sum <= 1e-10) {
      unbound++;
      candidates[0].weight = 1;
      sum = 1;
    }
    for (let i = 0; i < 4; i++) {
      const c = candidates[i];
      jointIndices[v * 4 + i] = c?.index ?? 0;
      weights[v * 4 + i] = c ? c.weight / sum : 0;
    }
  }
  return { jointIndices, weights, unbound };
}
