import { expect, test } from "bun:test";
import { shapedPineLookdevDefinition } from "@wrela/examples";
import { contentKey, type Vec3, vegetationSchema } from "@wrela/model";
import { coniferMeshes } from "./conifer-mesh";
import { deserializeArtifact, serializeArtifact } from "./cooked";
import { MAX_PINE_ARCHITECTURE_BRANCHES, pineArchitecture } from "./pine-architecture";
import { compileVegetation } from "./vegetation";

function specimen() {
  const doc = shapedPineLookdevDefinition();
  if (!doc.botanical?.conifer?.architecture) throw Error("Missing shaped pine");
  return {
    doc,
    source: doc.botanical,
    conifer: doc.botanical.conifer,
    shape: doc.botanical.conifer.architecture,
  };
}
const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));
function lineDistance(p: Vec3, a: Vec3, b: Vec3) {
  const d = b.map((v, i) => v - a[i]);
  const t = Math.max(
    0,
    Math.min(
      1,
      d.reduce((n, v, i) => n + v * (p[i] - a[i]), 0) /
        Math.max(
          1e-12,
          d.reduce((n, v) => n + v * v, 0),
        ),
    ),
  );
  return distance(p, a.map((v, i) => v + d[i] * t) as Vec3);
}

test("shaped pine preserves authored controls and replay through document decoding", () => {
  const { doc, shape } = specimen();
  expect(vegetationSchema.parse(doc)).toEqual(doc);
  const tree = pineArchitecture(doc);
  expect(pineArchitecture(structuredClone(doc))).toEqual(tree);
  expect(new Set(tree.branches.map((b) => b.id)).size).toBe(tree.branches.length);
  expect(new Set(tree.branches.map((b) => b.level))).toEqual(new Set([0, 1, 2, 3]));
  expect(tree.branches.filter((b) => b.cohort).every((b) => b.level === 3)).toBe(true);
  shape.twigPairs = 0;
  expect(vegetationSchema.safeParse(doc).success).toBe(false);
});

test("branch editing keeps every descendant attached and pruning leaves siblings unchanged", () => {
  const { doc, source } = specimen();
  const original = pineArchitecture(doc);
  source.branchEdits = [{ branch: "b12", lengthScale: 1.4, bend: [0, 0.8, 0], bare: false }];
  const edited = pineArchitecture(doc);
  expect(edited.branches.find((b) => b.id === "b12/s0-0")?.start).not.toEqual(
    original.branches.find((b) => b.id === "b12/s0-0")?.start,
  );
  const parents = new Map(edited.branches.map((b) => [b.id, b]));
  for (const b of edited.branches) {
    if (!b.parent) continue;
    const parent = parents.get(b.parent);
    if (!parent?.points) throw Error("Missing branch support");
    const points = parent.points;
    expect(Math.min(...points.slice(1).map((end, i) => lineDistance(b.start, points[i], end)))).toBeLessThan(
      0.00001,
    );
  }
  source.pruning.removedBranches = ["b12"];
  const pruned = pineArchitecture(doc);
  expect(pruned.branches.some((b) => b.id === "b12" || b.id.startsWith("b12/"))).toBe(false);
  expect(pruned.branches.filter((b) => b.id === "b16" || b.id.startsWith("b16/"))).toEqual(
    original.branches.filter((b) => b.id === "b16" || b.id.startsWith("b16/")),
  );
});

test("maturity adds branches and thickens wood without reshuffling existing attachments", () => {
  const { doc, source } = specimen();
  source.age = 0.4;
  const young = pineArchitecture(doc);
  source.age = 0.85;
  const old = pineArchitecture(doc),
    byId = new Map(old.branches.map((b) => [b.id, b]));
  expect(old.height).toBeGreaterThan(young.height);
  expect(old.branches.length).toBeGreaterThan(young.branches.length);
  for (const limb of young.branches.filter((b) => b.level === 1)) {
    const grown = byId.get(limb.id);
    if (!grown) throw Error("Existing branch disappeared");
    expect(distance(limb.start, grown.start)).toBeLessThan(0.001);
    expect(grown.radius).toBeGreaterThanOrEqual(limb.radius);
  }
});

test("held-out seeds and stages produce distinct complete cookable trees", () => {
  const shapes = new Set<string>();
  for (const seed of [73, 1009, 16847])
    for (const age of [0.25, 0.55, 0.85]) {
      const { doc, source } = specimen();
      doc.seed = seed;
      source.age = age;
      const mesh = coniferMeshes(doc, "review");
      expect(mesh.truncated).toBe(false);
      expect(mesh.leafCount).toBeGreaterThan(0);
      expect(mesh.foliage.thinCoverage?.layer?.length).toBe(mesh.foliage.positions.length / 3);
      shapes.add(contentKey(mesh.structure.branches));
      const artifact = compileVegetation(doc, "review");
      expect(artifact.diagnostics).toEqual([]);
      expect(deserializeArtifact(serializeArtifact(artifact))).toEqual(artifact);
      for (const surface of artifact.surfaces) {
        expect(surface.mesh.positions.every(Number.isFinite)).toBe(true);
        if (surface.mesh.shoots)
          expect(surface.mesh.shoots.anchors.length).toBe(surface.mesh.shoots.sourceIds.length * 4);
        else expect(surface.mesh.wind?.length).toBe((surface.mesh.positions.length / 3) * 4);
      }
    }
  expect(shapes.size).toBe(9);
}, 30000);

test("extreme authored amplification is bounded and reports incomplete structure", () => {
  const { doc, source, conifer, shape } = specimen();
  doc.branches = 96;
  source.age = 1;
  source.damage.brokenBranches = 0;
  conifer.shootsPerLimb = 20;
  shape.twigPairs = 4;
  const tree = pineArchitecture(doc);
  expect(tree.branches.length).toBeLessThanOrEqual(MAX_PINE_ARCHITECTURE_BRANCHES);
  expect(tree.truncated).toBe(true);
});

test("optional crown structure preserves old trees at zero and keeps edited twigs through clustered thinning", () => {
  const { doc, source, shape } = specimen();
  const original = pineArchitecture(doc);
  shape.crownAsymmetry = 0;
  shape.clusterVariation = 0;
  expect(pineArchitecture(doc)).toEqual(original);
  shape.crownAsymmetry = 0.85;
  shape.clusterVariation = 1;
  const structured = pineArchitecture(doc);
  expect(pineArchitecture(structuredClone(doc))).toEqual(structured);
  expect(structured.truncated).toBe(false);
  expect(structured.branches.find((b) => b.id === "trunk")).toEqual(
    original.branches.find((b) => b.id === "trunk"),
  );
  const ids = new Set(structured.branches.map((b) => b.id));
  const omitted = original.branches.find((b) => b.cohort && !ids.has(b.id));
  if (!omitted) throw Error("Expected a clustered gap");
  source.branchEdits = [{ branch: omitted.id, lengthScale: 1.1, bend: [0, 0.1, 0], bare: false }];
  const edited = pineArchitecture(doc);
  expect(edited.branches.some((b) => b.id === omitted.id)).toBe(true);
  const parents = new Map(edited.branches.map((b) => [b.id, b]));
  for (const branch of edited.branches) {
    expect(branch.points?.flat().every(Number.isFinite)).toBe(true);
    if (!branch.parent) continue;
    const parent = parents.get(branch.parent);
    if (!parent?.points) throw Error("Detached branch");
    expect(
      Math.min(...parent.points.slice(1).map((end, i) => lineDistance(branch.start, parent.points![i], end))),
    ).toBeLessThan(0.00001);
  }
});
