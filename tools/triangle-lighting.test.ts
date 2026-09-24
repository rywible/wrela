import { expect, test } from "bun:test";
import {
  compileIndirectGeometry,
  indirectProbePosition,
  indirectProbeVisible,
  indirectTriangleCacheSteps,
  triangleProbeVisibility,
} from "@wrela/compiler";
import type { EvaluatedScene, MeshData, Vec3 } from "@wrela/model";
import { withIndirectReceivers } from "@wrela/render-webgpu/indirect";
import { packVertices } from "@wrela/render-webgpu/packing";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { indirectAlpineFixture, indirectBoxFixture } from "./fixtures/indirect-scenes";

function sharedVertices(mesh: MeshData): MeshData {
  const positions: number[] = [],
    normals: number[] = [],
    ids = new Map<string, number>();
  const indices = Array.from(mesh.indices, (i) => {
    const p = Array.from(mesh.positions.subarray(i * 3, i * 3 + 3)),
      key = p.join(",");
    let index = ids.get(key);
    if (index === undefined) {
      index = positions.length / 3;
      ids.set(key, index);
      positions.push(...p);
      normals.push(0, 0, 0);
    }
    for (let a = 0; a < 3; a++) normals[index * 3 + a] += mesh.normals[i * 3 + a];
    return index;
  });
  for (let i = 0; i < normals.length; i += 3) {
    const length = Math.hypot(normals[i], normals[i + 1], normals[i + 2]);
    for (let a = 0; a < 3; a++) normals[i + a] /= length;
  }
  return {
    ...mesh,
    positions: Float32Array.from(positions),
    normals: Float32Array.from(normals),
    // Unused prefix/suffix exercise a material draw range on a shared mesh.
    indices: Uint32Array.from([0, 0, 0, ...indices, 0, 0, 0]),
  };
}

test.each([
  "faceted",
  "indexed",
])("%s curved receivers retain geometry and reject stale proofs", async (kind) => {
  const fixture = indirectAlpineFixture(),
    cache = new IndirectLightingCache();
  if (kind === "indexed") {
    const ground = fixture.surfaces[0],
      count = ground.mesh.indices.length;
    ground.mesh = sharedVertices(ground.mesh);
    ground.drawRange = { start: 3, count };
  }
  for (const s of fixture.surfaces) {
    s.material.normalStrength = 0.4;
    s.mesh.sourceIds = Array.from({ length: s.mesh.positions.length / 3 }, (_, i) => `vertex-${i}`);
    s.mesh.materialCoordinates = s.mesh.positions.slice();
  }
  const scene: EvaluatedScene = {
    ...fixture,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  try {
    cache.update(scene, { dimensions: [6, 4, 6], samples: 16, skySamples: 1 });
    const field = await cache.waitReady();
    expect(field.triangleCache?.report.admitted).toBeGreaterThan(20);
    const realized = withIndirectReceivers({ ...scene, indirectLighting: field });
    let changed = 0;
    for (let i = 0; i < scene.surfaces.length; i++) {
      const before = scene.surfaces[i].mesh,
        after = realized.surfaces[i].mesh;
      if (!after.indirectProofs) continue;
      changed++;
      expect(after.positions).toBe(before.positions);
      expect(after.indices).toBe(before.indices);
      expect(after.normals).toBe(before.normals);
      expect(after.materialCoordinates).toBe(before.materialCoordinates);
      expect(after.sourceIds).toBe(before.sourceIds);
      expect(realized.surfaces[i].drawRange).toBe(scene.surfaces[i].drawRange);
      expect(packVertices(after).length).toBe((after.positions.length / 3) * 23);
    }
    expect(changed).toBeGreaterThan(0);
    // Interpret the new short lists using the independently exercised CPU BVH
    // query. The full hierarchy is the control; no image interpolation is involved.
    const visibility = field.visibility;
    if (!visibility?.cells) throw Error("Missing visibility");
    let checked = 0,
      shared = 0;
    for (const source of field.triangleCache?.sources ?? []) {
      const mesh = source.mesh,
        proofs = mesh.indirectProofs;
      if (!proofs) continue;
      const seen = new Set<number>();
      for (let i = source.start; i < source.start + source.count && checked < 24; i += 3) {
        const vertex = mesh.indices[i],
          offset = vertex * 4;
        if (!proofs[offset]) continue;
        const first = proofs[offset] - 1,
          address = (proofs[offset + 1] - 1) * 4;
        const cells = visibility.cells.slice();
        const dims = field.dimensions.map((v) => v - 1);
        for (let c = 0; c < dims[0] * dims[1] * dims[2]; c++) cells.set([0, -1, address / 4, 0], c * 4);
        // The fragment's leaf pointer becomes the CPU tree's negative pointer.
        cells[address + 2] = -cells[address + 2] - 1;
        const x = proofs[offset + 2],
          y = proofs[offset + 3],
          z = 1 - Math.abs(x) - Math.abs(y);
        const raw =
          z >= 0
            ? [x, y, z]
            : [(1 - Math.abs(y)) * (x < 0 ? -1 : 1), (1 - Math.abs(x)) * (y < 0 ? -1 : 1), z];
        const n = raw.map((v) => v / Math.hypot(...raw)) as Vec3;
        const p = [0, 1, 2].map(
          (a) =>
            (mesh.positions[mesh.indices[i] * 3 + a] +
              mesh.positions[mesh.indices[i + 1] * 3 + a] +
              mesh.positions[mesh.indices[i + 2] * 3 + a]) /
            3,
        ) as Vec3;
        const c = p.map((v, a) =>
          Math.min(
            field.dimensions[a] - 2,
            Math.max(
              0,
              Math.floor((v - field.origin[a] + n[a] * Math.min(...field.spacing) * 0.02) / field.spacing[a]),
            ),
          ),
        );
        // The shader only consumes this proof in its certified interpolation
        // cell; another part of a shared triangle may use the ordinary query.
        if (c[0] + field.dimensions[0] * (c[1] + field.dimensions[1] * c[2]) !== first) continue;
        if (seen.has(vertex)) shared++;
        seen.add(vertex);
        for (let corner = 0; corner < 8; corner++) {
          const index =
            first +
            (corner & 1) +
            field.dimensions[0] * ((corner >> 1) & 1) +
            field.dimensions[0] * field.dimensions[1] * ((corner >> 2) & 1);
          if (field.data[index * 60 + 3] < 0.5) continue;
          const probe = indirectProbePosition(field, index);
          expect(indirectProbeVisible({ ...field, visibility: { ...visibility, cells } }, p, n, probe)).toBe(
            indirectProbeVisible({ ...field, visibility: { ...visibility, cells: undefined } }, p, n, probe),
          );
        }
        checked++;
      }
    }
    expect(checked).toBe(24);
    if (kind === "indexed") expect(shared).toBeGreaterThan(0);
    const restored = withIndirectReceivers({ ...realized, indirectLighting: undefined });
    expect(restored.surfaces.map((s) => s.mesh)).toEqual(scene.surfaces.map((s) => s.mesh));
    const moving = {
      ...scene,
      surfaces: scene.surfaces.map((s) => ({ ...s, matrix: s.matrix.slice() })),
      indirectLighting: field,
    };
    for (const s of moving.surfaces) s.matrix[12] += 1;
    expect(withIndirectReceivers(moving).surfaces.every((s) => !s.mesh.indirectProofs)).toBe(true);
    const cached = realized.surfaces.find((s) => s.mesh.indirectProofs);
    if (!cached) throw Error("Missing realized receiver");
    const range = { start: 3, count: 3 };
    const editedRange = withIndirectReceivers({ ...realized, surfaces: [{ ...cached, drawRange: range }] });
    expect(editedRange.surfaces[0].drawRange).toBe(range);
    expect(editedRange.surfaces[0].mesh.indirectProofs).toBeUndefined();
    const editedNormals = cached.mesh.normals.slice();
    const editedMesh = { ...cached.mesh, normals: editedNormals };
    const edited = withIndirectReceivers({ ...realized, surfaces: [{ ...cached, mesh: editedMesh }] });
    expect(edited.surfaces[0].mesh.normals).toBe(editedNormals);
    expect(edited.surfaces[0].mesh.indirectProofs).toBeUndefined();
    expect(
      withIndirectReceivers({ ...edited, indirectLighting: undefined }).surfaces[0].mesh.indirectProofs,
    ).toBeUndefined();
  } finally {
    cache.dispose();
  }
});

test("a whole-triangle certificate agrees with rays across its interior and a wide normal cone", async () => {
  const fixture = indirectBoxFixture(),
    geometry = compileIndirectGeometry(fixture.surfaces);
  const cache = new IndirectLightingCache(),
    scene: EvaluatedScene = {
      ...fixture,
      environment: evaluateEnvironment(),
      time: 0,
      mode: "beauty",
      grid: false,
    };
  try {
    cache.update(scene, {
      dimensions: [4, 4, 4],
      lighting: fixture.lighting,
      samples: 16,
      skySamples: 1,
      surfaceCache: false,
    });
    const field = await cache.waitReady();
    const points: Vec3[] = [
        [-0.9, 0, -0.9],
        [-0.9, 0, -0.5],
        [-0.5, 0, -0.9],
      ],
      normal: Vec3 = [0, 1, 0];
    let certified = 0;
    for (const probe of [
      [-0.7, 1, -0.7],
      [0.2, 1, 0.6],
      [0.1, -1, 0.1],
      [0, 1, -2],
    ] as Vec3[]) {
      const proof = triangleProbeVisibility(geometry, points, normal, probe);
      if (proof === undefined) continue;
      certified++;
      for (let x = 0; x <= 8; x++)
        for (let y = 0; y <= 8 - x; y++)
          for (let angle = 0; angle < 12; angle++) {
            const p = points[0].map(
              (v, a) => v + ((points[1][a] - v) * x) / 8 + ((points[2][a] - v) * y) / 8,
            ) as Vec3;
            const t = (angle * Math.PI) / 6,
              n: Vec3 = [Math.cos(t) * 0.85, Math.sqrt(1 - 0.85 ** 2), Math.sin(t) * 0.85];
            expect(
              indirectProbeVisible(
                {
                  ...field,
                  visibility: field.visibility ? { ...field.visibility, cells: undefined } : undefined,
                },
                p,
                n,
                probe,
              ),
            ).toBe(proof);
          }
    }
    expect(certified).toBeGreaterThan(1);
  } finally {
    cache.dispose();
  }
});

test("a small draw range cannot allocate an unbounded shared-vertex proof stream", async () => {
  const fixture = indirectAlpineFixture(),
    cache = new IndirectLightingCache();
  const surface = fixture.surfaces[0],
    positions = new Float32Array(262145 * 3),
    normals = new Float32Array(positions.length);
  positions.set(surface.mesh.positions);
  normals.set(surface.mesh.normals);
  surface.mesh = { ...surface.mesh, positions, normals };
  surface.drawRange = { start: 0, count: 3 };
  const scene: EvaluatedScene = {
    ...fixture,
    surfaces: [surface],
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  try {
    cache.update(scene, { surfaceCache: false, dimensions: [2, 2, 2], samples: 16, skySamples: 1 });
    const field = await cache.waitReady(),
      steps = indirectTriangleCacheSteps(compileIndirectGeometry(scene.surfaces), field, scene.surfaces);
    let step = steps.next();
    while (!step.done) step = steps.next();
    expect(step.value.report.excluded).toContainEqual({ id: surface.id, reason: "receiver vertex budget" });
    expect(step.value.report.bytes).toBe(0);
    expect(step.value.sources).toHaveLength(0);
  } finally {
    cache.dispose();
  }
});

test("a blocker inside the probe endpoint tolerance cannot certify a blocked receiver", () => {
  const floor = indirectBoxFixture().surfaces[0];
  floor.matrix[13] = 1;
  const geometry = compileIndirectGeometry([floor]);
  expect(
    triangleProbeVisibility(
      geometry,
      [
        [-0.1, 0, -0.1],
        [-0.1, 0, 0.1],
        [0.1, 0, -0.1],
      ],
      [0, 1, 0],
      [0, 1.00006, 0],
    ),
  ).toBeUndefined();
});
