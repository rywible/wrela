import { expect, test } from "bun:test";
import type { ToolkitEntry } from "@wrela/authoring";
import {
  appendWorkEvent,
  attachWorkReview,
  buildHeroDesign,
  createHeroWorkspace,
  createWorkSession,
  decideWork,
  heroCreationOperations,
  heroReviewViews,
  heroRevisionOperations,
  instantiateCreationRecipe,
  iterateAuthoring,
  materializeWorkProposal,
  planAuthoringIteration,
  planVisualRepair,
  promoteCreationRecipe,
  promoteToolkitEntry,
  proposeWork,
  recordToolkitTrial,
} from "@wrela/authoring";
import { compileAssemblyMesh } from "@wrela/compiler";
import { contentKey, parseProject, revolvedShellSchema } from "@wrela/model";
import { reviewAuthoring } from "@wrela/review";
import { transferTree } from "./fixtures/authoring-transfer";
import { expeditionLantern, hangingBell } from "./fixtures/hero-authoring";

function meshFor(design = expeditionLantern()) {
  const result = buildHeroDesign(design),
    doc = result.documents.find((d) => d.kind === "object");
  if (doc?.kind !== "object" || !doc.assembly) throw Error("Missing hero assembly");
  return { result, doc, ...compileAssemblyMesh(doc.assembly, doc.material) };
}
test("from-scratch designs have no asset templates and compile finite bounded editable geometry", () => {
  const baseline = createHeroWorkspace();
  expect(baseline.documents.some((d) => d.kind === "object")).toBe(false);
  for (const design of [expeditionLantern(), hangingBell()]) {
    const { mesh, doc } = meshFor(design);
    expect(mesh.indices.length / 3).toBeLessThan(30000);
    expect(mesh.positions.every(Number.isFinite)).toBe(true);
    const operations = heroCreationOperations(baseline, design),
      work = proposeWork(createWorkSession(baseline, { id: "test", brief: "Create" }), "created", operations);
    expect(materializeWorkProposal(work, "created").documents.some((d) => d.id === doc.id)).toBe(true);
    expect(contentKey(meshFor(design).mesh)).toBe(contentKey(mesh));
  }
});
test("revolved meridians reject crossings and produce closed shells with real cavities", () => {
  expect(() =>
    revolvedShellSchema.parse({
      segments: 32,
      profile: [
        [0.1, 0],
        [0.2, 1],
        [0.1, 1],
        [0.2, 0],
      ],
    }),
  ).toThrow();
  const { doc } = meshFor(hangingBell()),
    body = doc.assembly!.parts[0],
    { mesh } = compileAssemblyMesh({ ...doc.assembly!, parts: [body] }, doc.material);
  const edges = new Map<string, number>(),
    point = (i: number) =>
      Array.from(mesh.positions.slice(i * 3, i * 3 + 3))
        .map((v) => v.toFixed(6))
        .join(",");
  for (let i = 0; i < mesh.indices.length; i += 3) {
    const vs = [0, 1, 2].map((j) => point(mesh.indices[i + j]));
    for (let j = 0; j < 3; j++) {
      const key = [vs[j], vs[(j + 1) % 3]].sort().join("/");
      edges.set(key, (edges.get(key) ?? 0) + 1);
    }
  }
  expect([...edges.values()].every((n) => n === 2)).toBe(true);
  expect(mesh.positions.every((v, i) => i % 3 !== 0 || Number.isFinite(v))).toBe(true);
  // The material contour never touches the axis; there are no end discs sealing the mouth.
  expect(
    Array.from({ length: mesh.positions.length / 3 }, (_, i) =>
      Math.hypot(mesh.positions[i * 3], mesh.positions[i * 3 + 2]),
    ).every((r) => r > 0.014),
  ).toBe(true);
});
test("shell articulation moves children without mutating source", () => {
  const { doc, mesh } = meshFor();
  const before = contentKey(doc),
    open = compileAssemblyMesh(doc.assembly!, doc.material, { "door-hinge": 1 });
  expect(contentKey(open.mesh.positions)).not.toBe(contentKey(mesh.positions));
  expect(contentKey(doc)).toBe(before);
});
test("grain origin repair changes material coordinates without changing geometry or connections", () => {
  const { doc, result } = meshFor(hangingBell()),
    project = parseProject({
      schemaVersion: 1,
      id: "p",
      name: "Bell",
      entry: doc.id,
      documents: result.documents,
    });
  const work = createWorkSession(project, { id: "grain", brief: "Vary timber origin" });
  const candidate = materializeWorkProposal(
    proposeWork(
      work,
      "grain",
      planVisualRepair(project, {
        kind: "timber.grain-origin",
        target: doc.id,
        parts: ["mount"],
        amount: 0.8,
      }),
    ),
    "grain",
  );
  const next = candidate.documents.find((d) => d.id === doc.id);
  if (next?.kind !== "object") throw Error();
  const oldMesh = compileAssemblyMesh(doc.assembly!, doc.material).mesh,
    newMesh = compileAssemblyMesh(next.assembly!, next.material).mesh;
  expect(contentKey(oldMesh.positions)).toBe(contentKey(newMesh.positions));
  expect(contentKey(oldMesh.materialCoordinates)).not.toBe(contentKey(newMesh.materialCoordinates));
  const origins = Array.from({ length: 24 }, (_, seed) => {
    const operation = planVisualRepair(project, {
      kind: "timber.grain-origin",
      target: doc.id,
      parts: ["mount"],
      amount: 1,
      seed,
    })[0];
    if (operation.kind !== "document.set") throw Error("Expected a source property edit");
    return (operation.value as number[])[2];
  });
  expect(Math.min(...origins)).toBeLessThan(-0.05);
  expect(Math.max(...origins)).toBeGreaterThan(0.05);
  expect(() =>
    planVisualRepair(project, { kind: "timber.grain-origin", target: doc.id, parts: ["body"], amount: 1 }),
  ).toThrow("wood material");
});
test("creation recipes rebind material dependencies, preserve part IDs and require recorded acceptance", async () => {
  const baseline = createHeroWorkspace();
  let work = proposeWork(
    createWorkSession(baseline, { id: "recipe", brief: "Lantern" }),
    "lantern",
    heroCreationOperations(baseline, expeditionLantern()),
  );
  const options = {
    id: "lantern-recipe",
    revision: "1",
    description: "Portable construction",
    roots: ["hero-lantern"],
    capabilities: [],
  };
  expect(() => promoteCreationRecipe(work, "lantern", options)).toThrow("visual acceptance");
  const candidate = materializeWorkProposal(work, "lantern");
  work = attachWorkReview(work, "lantern", await reviewAuthoring(baseline, candidate, []));
  work = decideWork(work, "lantern", {
    reviewer: "test-fixture",
    verdict: "accepted",
    reason: "Fixture approval exercises serialization, not an art qualification",
  });
  const recipe = promoteCreationRecipe(work, "lantern", options),
    plan = instantiateCreationRecipe(baseline, recipe, "copy");
  const copied = materializeWorkProposal(
    proposeWork(createWorkSession(baseline, { id: "copy", brief: "Transfer" }), "copy", plan.operations),
    "copy",
  );
  const object = copied.documents.find((d) => d.id === plan.roots[0]);
  expect(object?.kind).toBe("object");
  if (object?.kind !== "object") throw Error();
  expect(object.assembly?.parts[0].id).toBe("reservoir");
  expect(object.material).not.toBe("hero-lantern-brass");
  expect(() => instantiateCreationRecipe(copied, recipe, "copy")).toThrow("exists");
  expect(plan.acceptance).toBe("unreviewed");
});
test("pinned branch decisions fail before persistence or rendering", async () => {
  const baseline = createHeroWorkspace();
  let work = proposeWork(
    createWorkSession(baseline, { id: "pins", brief: "Create" }),
    "base",
    heroCreationOperations(baseline, expeditionLantern()),
  );
  let writes = 0,
    rendered = false;
  const store = {
    get: async () => work,
    put: async (next: typeof work) => {
      writes++;
      work = next;
      return contentKey(next);
    },
    saveArtifact: async () => "unused",
  };
  await expect(
    iterateAuthoring(
      store,
      work.id,
      {
        expectedKey: contentKey(work),
        proposal: "bad",
        target: "hero-lantern",
        baseProposal: "base",
        operations: [{ kind: "document.rename", target: "hero-lantern", name: "Changed" }],
        pins: [{ target: "hero-lantern", path: ["name"], reason: "Pinned identity" }],
      },
      async () => {
        rendered = true;
        throw Error("Unexpected render");
      },
    ),
  ).rejects.toThrow("Pinned decision");
  expect(writes).toBe(0);
  expect(rendered).toBe(false);
});
test("toolkit promotion requires distinct source-bound cross-family trials and records art separately", async () => {
  let entry: ToolkitEntry = {
    version: 1,
    id: "revolved-shell",
    revision: "1",
    title: "Hollow curved shell",
    description: "Closed thin shell",
    tags: ["shell", "hollow"],
    level: "compiler",
    status: "experimental",
    implementation: {
      kind: "builtin",
      capability: "assembly.revolved-shell",
      version: "1",
      sourceKey: "fixture-source",
    },
    controls: [],
    preserves: ["Hollow meridian"],
    limitations: ["No self-intersecting profile"],
    previews: [],
    engineeringMs: 100,
    trials: [],
  };
  for (const design of [expeditionLantern(), hangingBell()]) {
    const baseline = createHeroWorkspace();
    let work = proposeWork(
      createWorkSession(baseline, { id: design.id, brief: design.intent }),
      "created",
      heroCreationOperations(baseline, design),
    );
    work = attachWorkReview(
      work,
      "created",
      await reviewAuthoring(baseline, materializeWorkProposal(work, "created"), []),
    );
    expect(() =>
      recordToolkitTrial(entry, { work, proposal: "created", family: design.family, elapsedMs: 1 }),
    ).toThrow("usage");
    work = appendWorkEvent(work, {
      id: "usage",
      at: new Date().toISOString(),
      kind: "tool-development",
      actor: "test",
      summary: "revolved-shell@1",
      evidence: [work.proposals[0].candidateKey],
    });
    entry = recordToolkitTrial(entry, { work, proposal: "created", family: design.family, elapsedMs: 1 });
    expect(() =>
      recordToolkitTrial(entry, { work, proposal: "created", family: design.family, elapsedMs: 1 }),
    ).toThrow("Duplicate");
  }
  expect(promoteToolkitEntry(entry, "validated").status).toBe("validated");
  expect(() => promoteToolkitEntry(entry, "approved")).toThrow();
  expect(() =>
    promoteToolkitEntry(
      { ...entry, trials: entry.trials.map((t) => ({ ...t, family: "same" })) },
      "validated",
    ),
  ).toThrow("different asset families");
});

test("construction advances through production and returns to clay without changing source policy", () => {
  const design = expeditionLantern();
  design.stage = "blockout";
  const baseline = createHeroWorkspace();
  let work = proposeWork(
    createWorkSession(baseline, { id: "stages", brief: "Create" }),
    "blockout",
    heroCreationOperations(baseline, design),
  );
  const blockout = materializeWorkProposal(work, "blockout"),
    root = blockout.documents.find((d) => d.id === design.id)!;
  for (const stage of ["construction", "surface", "production", "blockout"] as const) {
    design.stage = stage;
    const operations = heroRevisionOperations(blockout, design);
    expect(
      operations.some(
        (op) => op.kind === "document.set" && ["generated", "dependencies"].includes(String(op.path[0])),
      ),
    ).toBe(false);
    work = proposeWork(work, stage + "-revision", [...work.proposals[0].batch.operations, ...operations]);
    const project = materializeWorkProposal(work, stage + "-revision"),
      object = project.documents.find((d) => d.id === design.id)!;
    expect(object.generated).toEqual(root.generated);
    expect(heroReviewViews(project, design.id, stage).length).toBe(stage === "production" ? 6 : 3);
    if (stage === "production")
      expect(project.documents.find((d) => d.id === design.id + "-light")).toMatchObject({
        emission: { intensity: 3 },
      });
  }
});

test("material combination preserves geometry and provenance", () => {
  const baseline = createHeroWorkspace();
  let work = proposeWork(
    createWorkSession(baseline, { id: "combine", brief: "Create" }),
    "base",
    heroCreationOperations(baseline, expeditionLantern()),
  );
  work = proposeWork(work, "donor", [
    ...work.proposals[0].batch.operations,
    { kind: "document.set", target: "hero-lantern-brass", path: ["roughness"], value: 0.7 },
  ]);
  const plan = planAuthoringIteration(work, {
    expectedKey: contentKey(work),
    proposal: "combined",
    target: "hero-lantern",
    baseProposal: "base",
    combine: { proposal: "donor", materials: ["hero-lantern-brass"] },
  });
  const merged = materializeWorkProposal(proposeWork(work, "combined", plan.operations), "combined");
  expect(merged.documents.find((d) => d.id === "hero-lantern-brass")).toMatchObject({ roughness: 0.7 });
  expect(merged.documents.find((d) => d.id === "hero-lantern")).toEqual(
    materializeWorkProposal(work, "base").documents.find((d) => d.id === "hero-lantern"),
  );
});

test("changed branch repairs must address a realized branch identity", async () => {
  const baseline = transferTree(),
    doc = baseline.documents.find((d) => d.id === baseline.entry)!;
  const work = proposeWork(
    createWorkSession(baseline, { id: "branches", brief: "Repair" }),
    "bad",
    planVisualRepair(baseline, {
      kind: "vegetation.branch-gap",
      target: doc.id,
      branches: ["not-real"],
      bare: true,
    }),
  );
  await expect(reviewAuthoring(baseline, materializeWorkProposal(work, "bad"), [])).rejects.toThrow(
    "not realized",
  );
});
