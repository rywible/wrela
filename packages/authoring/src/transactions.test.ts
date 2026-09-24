import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import type { EditBatch } from "./commands";
import { AuthoringSession, RevisionConflict } from "./session";

const rename = (target: string, name: string): EditBatch["operations"] => [
  { kind: "document.rename", target, name },
];
const proposal = (id: string, target: string, name: string): EditBatch => ({
  transactionId: id,
  actor: "agent",
  intent: `Name ${target}`,
  preconditions: { reads: {}, writes: { [target]: 0 } },
  operations: rename(target, name),
});

test("independent proposals commit while the legacy global revision remains strict", () => {
  const session = new AuthoringSession(referenceProject());
  const before = session.getSnapshot().project;
  const a = proposal("sky-name", "winter-sky", "A sky");
  const b = proposal("fur-name", "snow-fur", "White fur");
  expect(session.apply(a).revision).toBe(1);
  expect(session.apply(b).revision).toBe(2);
  expect(session.getSnapshot().project.documents.find((d) => d.id === "polar-bunny")).toBe(
    before.documents.find((d) => d.id === "polar-bunny"),
  );
  expect(() => session.apply({ expectedRevision: 0, operations: rename("stone", "Old proposal") })).toThrow(
    RevisionConflict,
  );
  const retry = session.apply(a);
  expect(retry.revision).toBe(1);
  expect(session.getSnapshot().revision).toBe(2);
  expect(retry.key).not.toBe(contentKey(session.getSnapshot().project));
  expect(() => session.apply({ ...a, operations: rename("winter-sky", "Different payload") })).toThrow(
    "transaction ID",
  );
});

test("dependent proposals report document conflicts and implicit material reads", () => {
  const session = new AuthoringSession(referenceProject());
  session.apply(proposal("edit-fur", "snow-fur", "New material"));
  const local: EditBatch = {
    transactionId: "local-material",
    preconditions: { reads: { "snow-fur": 0 }, writes: { "polar-bunny": 0, "local-fur": null } },
    operations: [{ kind: "material.makeLocal", target: "polar-bunny", newId: "local-fur" }],
  };
  try {
    session.apply(local);
    throw Error("Expected conflict");
  } catch (error) {
    expect(error).toBeInstanceOf(RevisionConflict);
    expect((error as RevisionConflict).conflicts).toContainEqual({
      document: "snow-fur",
      reason: "revision",
      expected: 0,
      actual: 1,
    });
  }
  expect(session.getSnapshot().revision).toBe(1);
  expect(session.inspect("local-fur")).toBeUndefined();
  const omitted = { ...local, preconditions: { reads: {}, writes: { "polar-bunny": 0, "local-fur": null } } };
  try {
    session.apply(omitted);
    throw Error("Expected missing read");
  } catch (error) {
    expect((error as RevisionConflict).conflicts).toContainEqual({
      document: "snow-fur",
      reason: "missing-read",
      actual: 1,
    });
  }
});

test("creation preconditions must declare implicit writes and absence", () => {
  const session = new AuthoringSession(referenceProject());
  try {
    session.apply({
      preconditions: { reads: {}, writes: { "polar-bunny": 0 } },
      operations: [{ kind: "material.makeLocal", target: "polar-bunny", newId: "local-fur" }],
    });
    throw Error("Expected missing write");
  } catch (error) {
    expect((error as RevisionConflict).conflicts).toContainEqual({
      document: "local-fur",
      reason: "missing-write",
      actual: null,
    });
  }
  expect(session.getSnapshot().revision).toBe(0);
});

test("previews are isolated and retained transactions have semantic diffs and metadata", () => {
  const session = new AuthoringSession(referenceProject());
  const batch = proposal("review-me", "winter-sky", "Evening");
  const preview = session.preview(batch);
  expect(preview.diff).toEqual([{ id: "winter-sky", action: "update", properties: ["name"] }]);
  if (!preview.changes[0].after) throw Error("Missing preview");
  preview.changes[0].after.name = "Mutated preview";
  expect(session.getSnapshot().revision).toBe(0);
  const result = session.apply(batch);
  expect(result.key).toBe(contentKey(session.getSnapshot().project));
  expect(session.inspectTransaction("review-me")).toMatchObject({
    actor: "agent",
    intent: "Name winter-sky",
    revision: 1,
    diff: [{ id: "winter-sky", action: "update", properties: ["name"] }],
  });
  const record = session.inspectTransaction("review-me");
  if (!record?.changes[0].after) throw Error("Missing record");
  record.changes[0].after.name = "Mutated record";
  expect(session.inspect("winter-sky")).toMatchObject({ name: "Evening" });
});

test("targeted revert preserves another agent's work, supports retry and remains undoable", () => {
  const session = new AuthoringSession(referenceProject());
  session.apply(proposal("a", "winter-sky", "A sky"));
  session.apply(proposal("b", "snow-fur", "B fur"));
  const result = session.revert("a", {
    transactionId: "revert-a",
    actor: "human",
    intent: "Keep prior lighting",
  });
  expect(session.inspect("winter-sky")).toMatchObject({ name: "Winter afternoon" });
  expect(session.inspect("snow-fur")).toMatchObject({ name: "B fur" });
  expect(session.inspectTransaction(result.transactionId)?.reverts).toBe("a");
  expect(
    session.revert("a", { transactionId: "revert-a", actor: "human", intent: "Keep prior lighting" }),
  ).toEqual(result);
  session.undo();
  expect(session.inspect("winter-sky")).toMatchObject({ name: "A sky" });
  expect(session.inspect("snow-fur")).toMatchObject({ name: "B fur" });
  session.redo();
  expect(session.inspect("winter-sky")).toMatchObject({ name: "Winter afternoon" });
});

test("targeted revert refuses overlapping work and invalidating later references", () => {
  const session = new AuthoringSession(referenceProject());
  session.apply(proposal("a", "winter-sky", "A sky"));
  session.apply({ expectedRevision: 1, operations: rename("winter-sky", "Later name") });
  expect(() => session.revert("a")).toThrow("Later changes overlap");
  const material = session.getSnapshot().project.documents.find((d) => d.id === "snow-fur");
  if (!material) throw Error("Missing material");
  session.apply({
    expectedRevision: 2,
    transactionId: "new-material",
    operations: [{ kind: "document.create", document: { ...material, id: "shared-fur" } }],
  });
  session.apply({
    expectedRevision: 3,
    operations: [{ kind: "material.assign", target: "polar-bunny", material: "shared-fur" }],
  });
  expect(() => session.revert("new-material")).toThrow("invalidate source");
  expect(session.getSnapshot().revision).toBe(4);
});

test("changed-document history preserves ordering and is bounded independently of project size", () => {
  const session = new AuthoringSession(referenceProject(), {
    historyLimit: 2,
    historyBytes: 100_000,
    receiptLimit: 1,
  });
  const initial = session.getSnapshot().project;
  session.apply(proposal("a", "winter-sky", "A"));
  session.apply(proposal("b", "snow-fur", "B"));
  session.apply(proposal("c", "stone", "C"));
  expect(session.historyUsage().undoEntries).toBe(2);
  expect(session.historyUsage().transactionEntries).toBe(2);
  expect(session.historyUsage().retainedChangeBytes).toBeLessThan(100_000);
  expect(() => session.apply(proposal("a", "winter-sky", "A"))).toThrow("receipt expired");
  session.undo();
  session.undo();
  expect(session.getSnapshot().canUndo).toBe(false);
  expect(session.getSnapshot().project.documents.map((d) => d.id)).toEqual(
    initial.documents.map((d) => d.id),
  );
  expect(session.getSnapshot().project.documents.find((d) => d.id === "polar-bunny")).toBe(
    initial.documents.find((d) => d.id === "polar-bunny"),
  );
  expect(session.inspect("winter-sky")).toMatchObject({ name: "A" });
});

test("incremental validation still rejects deletion dependencies, newly introduced cycles and wrong-kind references", () => {
  const session = new AuthoringSession(referenceProject());
  expect(() =>
    session.apply({ expectedRevision: 0, operations: [{ kind: "document.delete", target: "snow-fur" }] }),
  ).toThrow("Missing definition");
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [{ kind: "material.assign", target: "polar-bunny", material: "winter-sky" }],
    }),
  ).toThrow("must be material");
  expect(session.getSnapshot().revision).toBe(0);
});

test("proposal read sets include materials copied from intermediate batch state", () => {
  const session = new AuthoringSession(referenceProject());
  session.apply(proposal("stone-update", "stone", "Changed stone"));
  const batch: EditBatch = {
    preconditions: {
      reads: { "snow-fur": 0, "onyx-eyes": 0, "rose-nose": 0, "rose-ears": 0 },
      writes: { "polar-bunny": 0, "local-stone": null },
    },
    operations: [
      { kind: "material.assign", target: "polar-bunny", material: "stone" },
      { kind: "material.makeLocal", target: "polar-bunny", newId: "local-stone" },
    ],
  };
  try {
    session.apply(batch);
    throw Error("Expected missing intermediate read");
  } catch (error) {
    expect((error as RevisionConflict).conflicts).toContainEqual({
      document: "stone",
      reason: "missing-read",
      actual: 1,
    });
  }
  expect(session.inspect("local-stone")).toBeUndefined();
  if (!batch.preconditions) throw Error("Missing preconditions");
  batch.preconditions.reads.stone = 1;
  expect(session.apply(batch).changed.sort()).toEqual(["local-stone", "polar-bunny"]);
});

test("transaction receipts and own result are finalized before observers can edit", () => {
  const session = new AuthoringSession(referenceProject());
  const outer = proposal("outer", "winter-sky", "Outer edit");
  let invoked = false;
  session.subscribe(() => {
    if (invoked) return;
    invoked = true;
    expect(session.apply(outer).revision).toBe(1);
    expect(() => session.apply({ ...proposal("outer", "stone", "Different work") })).toThrow(
      "transaction ID",
    );
    session.apply(proposal("inner", "snow-fur", "Observer edit"));
  });
  const result = session.apply(outer);
  expect(result.revision).toBe(1);
  expect(session.getSnapshot().revision).toBe(2);
  expect(result.key).not.toBe(contentKey(session.getSnapshot().project));
  expect(session.transactions().map((transaction) => transaction.id)).toEqual(["outer", "inner"]);
});

function orderedProject(count = 4) {
  const material = referenceProject().documents.find((document) => document.kind === "material");
  if (!material) throw Error("Missing material");
  return {
    schemaVersion: 1 as const,
    id: "ordering",
    name: "Ordering",
    entry: `doc-${count - 1}`,
    documents: Array.from({ length: count }, (_, index) => ({ ...material, id: `doc-${index}` })),
  };
}

test("targeted revert preserves unrelated deletion order and restores missing documents by surviving neighbors", () => {
  const session = new AuthoringSession(orderedProject());
  session.apply({ expectedRevision: 0, transactionId: "rename-c", operations: rename("doc-2", "C") });
  session.apply({ expectedRevision: 1, operations: [{ kind: "document.delete", target: "doc-0" }] });
  session.revert("rename-c");
  expect(session.getSnapshot().project.documents.map((d) => d.id)).toEqual(["doc-1", "doc-2", "doc-3"]);
  const deleted = new AuthoringSession(orderedProject());
  deleted.apply({
    expectedRevision: 0,
    transactionId: "delete-c",
    operations: [{ kind: "document.delete", target: "doc-2" }],
  });
  deleted.apply({ expectedRevision: 1, operations: [{ kind: "document.delete", target: "doc-0" }] });
  deleted.revert("delete-c");
  expect(deleted.getSnapshot().project.documents.map((d) => d.id)).toEqual(["doc-1", "doc-2", "doc-3"]);
});

test("coalesced deletions undo to the gesture's original order and redo exactly", () => {
  const project = orderedProject(),
    session = new AuthoringSession(project);
  for (let i = 0; i < 2; i++)
    session.apply({
      expectedRevision: i,
      gesture: "delete-pair",
      operations: [{ kind: "document.delete", target: `doc-${i}` }],
    });
  session.undo();
  expect(session.getSnapshot().project).toEqual(project);
  session.redo();
  expect(session.getSnapshot().project.documents.map((d) => d.id)).toEqual(["doc-2", "doc-3"]);
});

test("revert respects project capacity, valid metadata, and aggregate retained history budget", () => {
  const project = orderedProject(256),
    session = new AuthoringSession(project);
  session.apply({
    expectedRevision: 0,
    transactionId: "remove-one",
    operations: [{ kind: "document.delete", target: "doc-0" }],
  });
  session.apply({
    expectedRevision: 1,
    operations: [{ kind: "document.create", document: { ...project.documents[0], id: "replacement" } }],
  });
  expect(() => session.revert("remove-one")).toThrow("definition project limit");
  expect(session.getSnapshot().project.documents).toHaveLength(256);
  expect(() => session.revert("remove-one", { transactionId: "" })).toThrow();
  const bounded = new AuthoringSession(orderedProject(), { historyBytes: 2500 });
  for (let i = 0; i < 12; i++)
    bounded.apply({ expectedRevision: i, gesture: "rename", operations: rename("doc-0", `Version ${i}`) });
  const usage = bounded.historyUsage();
  expect(usage.retainedChangeBytes).toBeLessThanOrEqual(usage.historyBytes);
  expect(usage.retainedChangeBytes).toBe(usage.undoRedoChangeBytes + usage.transactionChangeBytes);
});

test("an explicit document reordering is retained and undoable even when its data is unchanged", () => {
  const project = orderedProject(),
    session = new AuthoringSession(project);
  session.apply({
    expectedRevision: 0,
    operations: [
      { kind: "document.delete", target: "doc-0" },
      { kind: "document.create", document: project.documents[0] },
    ],
  });
  expect(session.getSnapshot().project.documents.map((d) => d.id)).toEqual([
    "doc-1",
    "doc-2",
    "doc-3",
    "doc-0",
  ]);
  session.undo();
  expect(session.getSnapshot().project).toEqual(project);
  session.redo();
  expect(session.getSnapshot().project.documents.map((d) => d.id)).toEqual([
    "doc-1",
    "doc-2",
    "doc-3",
    "doc-0",
  ]);
});

test("optional material domains and layers remain editable after import and fully validated", () => {
  let session = new AuthoringSession(referenceProject());
  const layer = {
    color: [0.2, 0.4, 0.6],
    roughness: 0.6,
    metallic: 0,
    coverage: 0.5,
    slopeBias: 0.2,
    noiseScale: 2,
    normalStrength: 0.1,
  };
  const edit = (operations: EditBatch["operations"]) =>
    session.apply({
      expectedRevision: session.getSnapshot().revision,
      operations,
    });
  edit([
    { kind: "document.set", target: "snow-fur", path: ["domain"], value: "world" },
    { kind: "document.set", target: "snow-fur", path: ["layers"], value: [layer] },
  ]);
  expect(session.inspect("snow-fur")).toMatchObject({ domain: "world", layers: [layer] });
  session.undo();
  expect(session.inspect("snow-fur")).not.toHaveProperty("domain");
  session.redo();
  edit([{ kind: "document.set", target: "snow-fur", path: ["layers"], value: undefined }]);
  session = new AuthoringSession(JSON.parse(session.export()));
  edit([{ kind: "document.set", target: "snow-fur", path: ["layers"], value: [layer] }]);
  const revision = session.getSnapshot().revision;
  expect(() =>
    edit([{ kind: "document.set", target: "snow-fur", path: ["layers"], value: [layer, layer, layer] }]),
  ).toThrow();
  expect(() =>
    edit([{ kind: "document.set", target: "snow-fur", path: ["domain"], value: "screen" }]),
  ).toThrow();
  expect(() => edit([{ kind: "document.set", target: "snow-fur", path: ["unknown"], value: true }])).toThrow(
    "Unknown property",
  );
  expect(session.getSnapshot().revision).toBe(revision);
});

test("optional compound collision and fidelity policies author atomically without allowing provenance bypass", () => {
  const session = new AuthoringSession(referenceProject());
  const collider = { id: "body", shape: "sphere", position: [0, 0, 0], rotation: [0, 0, 0], radius: 1 };
  session.apply({
    expectedRevision: 0,
    operations: [
      { kind: "document.set", target: "river-stone", path: ["collision"], value: "compound" },
      { kind: "document.set", target: "river-stone", path: ["colliders"], value: [collider] },
      { kind: "document.set", target: "polar-bunny", path: ["physics", "colliders"], value: [collider] },
      { kind: "document.set", target: "river-stone", path: ["field", "fidelity"], value: { strict: true } },
      { kind: "document.set", target: "river-stone", path: ["field", "fidelity", "maxError"], value: 0.05 },
    ],
  });
  expect(session.inspect("river-stone")).toMatchObject({
    collision: "compound",
    colliders: [collider],
    field: { fidelity: { strict: true, maxError: 0.05 } },
  });
  expect(session.inspect("polar-bunny")).toMatchObject({ physics: { colliders: [collider] } });
  expect(() =>
    session.apply({
      expectedRevision: 1,
      operations: [
        {
          kind: "document.set",
          target: "river-stone",
          path: ["field", "fidelity", "unbounded"],
          value: true,
        },
      ],
    }),
  ).toThrow("Unknown property");
  expect(() =>
    session.apply({
      expectedRevision: 1,
      operations: [
        {
          kind: "document.set",
          target: "river-stone",
          path: ["generated"],
          value: { generator: "recipe", policy: "detached" },
        },
      ],
    }),
  ).toThrow("source policy");
  session.undo();
  expect(session.inspect("river-stone")).not.toHaveProperty("colliders");
  expect(session.inspect("river-stone")).not.toHaveProperty("field.fidelity");
});
