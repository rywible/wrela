import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/model";
import { AuthoringSession, RevisionConflict } from "./session";

test("atomic batches reject invalid dependencies without touching source or history", () => {
  const s = new AuthoringSession(referenceProject());
  expect(() =>
    s.apply({
      expectedRevision: 0,
      operations: [
        { kind: "document.rename", target: "polar-bunny", name: "Changed" },
        { kind: "material.assign", target: "polar-bunny", material: "missing" },
      ],
    }),
  ).toThrow();
  expect(s.getSnapshot().revision).toBe(0);
  expect(s.getSnapshot().project.documents.find((d) => d.id === "polar-bunny")?.name).toBe("Polar bunny");
  expect(s.getSnapshot().canUndo).toBe(false);
});
test("undo is monotonic and gesture coalescing preserves original content", () => {
  const s = new AuthoringSession(referenceProject());
  for (let i = 0; i < 5; i++)
    s.apply({
      expectedRevision: i,
      gesture: "drag",
      operations: [
        { kind: "document.set", target: "winter-sky", path: ["sunElevation"], value: 0.1 + i * 0.1 },
      ],
    });
  s.undo();
  expect(s.getSnapshot().revision).toBe(6);
  expect(s.getSnapshot().canUndo).toBe(false);
  expect(s.getSnapshot().project.documents.find((d) => d.id === "winter-sky")).toMatchObject({
    sunElevation: 0.48,
  });
  s.redo();
  expect(s.getSnapshot().revision).toBe(7);
  expect(() =>
    s.apply({
      expectedRevision: 5,
      operations: [{ kind: "document.rename", target: "polar-bunny", name: "Oops" }],
    }),
  ).toThrow(RevisionConflict);
});
test("agent edits compare every touched document revision", () => {
  const s = new AuthoringSession(referenceProject());
  expect(() =>
    s.apply({
      expectedRevision: 0,
      expectedDocuments: { "winter-sky": 0 },
      operations: [{ kind: "document.rename", target: "polar-bunny", name: "Bunny" }],
    }),
  ).toThrow(RevisionConflict);
});
test("joint rename preserves references and field cycles fail safely", () => {
  const s = new AuthoringSession(referenceProject());
  s.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.set", target: "polar-bunny", path: ["joints", 1, "name"], value: "Neck" }],
  });
  expect(
    (s.getSnapshot().project.documents.find((d) => d.id === "polar-bunny") as any).joints[1],
  ).toMatchObject({ id: "head-joint", name: "Neck" });
  expect(() =>
    s.apply({
      expectedRevision: 1,
      operations: [
        {
          kind: "field.update",
          target: "polar-bunny",
          node: "body",
          changes: { kind: "union", children: ["anatomy", "head"] },
        },
      ],
    }),
  ).toThrow("cycle");
});
test("local material variant is independent and prototype paths are blocked", () => {
  const s = new AuthoringSession(referenceProject());
  s.apply({
    expectedRevision: 0,
    operations: [{ kind: "material.makeLocal", target: "polar-bunny", newId: "local-fur" }],
  });
  expect(s.getSnapshot().project.documents.find((d) => d.id === "polar-bunny")).toMatchObject({
    material: "local-fur",
  });
  expect(() =>
    s.apply({
      expectedRevision: 1,
      operations: [
        { kind: "document.set", target: "polar-bunny", path: ["__proto__", "polluted"], value: true },
      ],
    }),
  ).toThrow();
  expect(({} as any).polluted).toBeUndefined();
});

test("read snapshots cannot mutate authoritative source or revision state", () => {
  const session = new AuthoringSession(referenceProject()),
    snapshot = session.getSnapshot();
  expect(() => {
    snapshot.project.name = "Bypass";
  }).toThrow();
  expect(() => {
    snapshot.documentRevisions["polar-bunny"] = 100;
  }).toThrow();
  const inspected = session.inspect("polar-bunny") as any;
  inspected.name = "Independent copy";
  expect((session.inspect("polar-bunny") as any).name).toBe("Polar bunny");
  expect(session.getSnapshot()).toBe(snapshot);
});
test("implicit material creation participates in revisions, undo and editable generation detachment", () => {
  const project = referenceProject(),
    material = project.documents.find((doc) => doc.id === "snow-fur");
  if (!material) throw Error("Missing material");
  material.generated = { generator: "fixture", policy: "locked" };
  const session = new AuthoringSession(project),
    result = session.apply({
      expectedRevision: 0,
      expectedDocuments: { "polar-bunny": 0 },
      operations: [{ kind: "material.makeLocal", target: "polar-bunny", newId: "local-fur" }],
    });
  expect(result.changed.sort()).toEqual(["local-fur", "polar-bunny"]);
  expect(session.getSnapshot().documentRevisions["local-fur"]).toBe(1);
  session.apply({
    expectedRevision: 1,
    operations: [{ kind: "document.set", target: "local-fur", path: ["roughness"], value: 0.5 }],
  });
  expect((session.inspect("local-fur") as any).generated.policy).toBe("detached");
  expect((session.inspect("snow-fur") as any).roughness).toBe(0.83);
  session.undo();
  session.undo();
  expect(session.inspect("local-fur")).toBeUndefined();
  expect((session.inspect("polar-bunny") as any).material).toBe("snow-fur");
  session.redo();
  expect(session.getSnapshot().documentRevisions["local-fur"]).toBe(5);
});
test("shape removal retains a surviving group transform and cleans removed subtrees", () => {
  const project = referenceProject(),
    doc = project.documents.find((doc) => doc.id === "river-stone");
  if (!doc || doc.kind !== "object") throw Error("Missing stone");
  const base = doc.field.nodes[0];
  doc.field.nodes = [
    {
      ...base,
      id: "root",
      kind: "union",
      position: [0.5, 0.3, 0],
      rotation: [0, 0.7, 0],
      children: ["left", "right"],
      material: "stone",
    },
    { ...base, id: "left", position: [-0.5, 0, 0] },
    { ...base, id: "right", position: [0.5, 0, 0] },
  ];
  doc.field.root = "root";
  const session = new AuthoringSession(project);
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "field.remove", target: doc.id, node: "right" }],
  });
  const result = session.inspect(doc.id) as typeof doc;
  expect(result.field.root).toBe("root");
  expect(result.field.nodes[0]).toMatchObject({
    position: [0.5, 0.3, 0],
    rotation: [0, 0.7, 0],
    children: ["left"],
    material: "stone",
  });
  expect(() =>
    session.apply({
      expectedRevision: 1,
      operations: [{ kind: "field.remove", target: doc.id, node: "left" }],
    }),
  ).toThrow("retain");
  expect(session.getSnapshot().revision).toBe(1);
});
test("undo navigation ends gesture coalescing and edits preserve explicit dependencies", () => {
  const project = referenceProject(),
    doc = project.documents.find((doc) => doc.id === "river-stone");
  if (!doc) throw Error("Missing stone");
  doc.dependencies = ["winter-sky"];
  const session = new AuthoringSession(project);
  session.apply({
    expectedRevision: 0,
    gesture: "a",
    operations: [{ kind: "document.rename", target: doc.id, name: "A" }],
  });
  session.apply({
    expectedRevision: 1,
    gesture: "b",
    operations: [{ kind: "document.rename", target: doc.id, name: "B" }],
  });
  session.undo();
  session.apply({
    expectedRevision: 3,
    gesture: "a",
    operations: [{ kind: "document.rename", target: doc.id, name: "C" }],
  });
  session.undo();
  expect((session.inspect(doc.id) as any).name).toBe("A");
  expect((session.inspect(doc.id) as any).dependencies).toContain("winter-sky");
});
test("nested stable identities cannot be casually overwritten", () => {
  const session = new AuthoringSession(referenceProject());
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [
        { kind: "document.set", target: "polar-bunny", path: ["joints", 0, "id"], value: "changed" },
      ],
    }),
  ).toThrow("Stable identities");
  expect(session.getSnapshot().revision).toBe(0);
});

test("a faulty observer cannot make a committed transaction appear to fail", () => {
  const session = new AuthoringSession(referenceProject());
  let notified = false;
  session.subscribe(() => {
    throw Error("Broken observer");
  });
  session.subscribe(() => {
    notified = true;
  });
  const result = session.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.rename", target: "polar-bunny", name: "Committed bunny" }],
  });
  expect(result.revision).toBe(1);
  expect(notified).toBe(true);
  expect((session.inspect("polar-bunny") as any).name).toBe("Committed bunny");
  expect(session.getSnapshot().diagnostics[0].code).toBe("authoring.observer");
});

test("optional world water supports add, remove, JSON round trip and re-add while rejecting unknown properties", () => {
  const project = referenceProject(),
    world = project.documents.find((document) => document.kind === "world");
  if (!world) throw Error("Missing world");
  delete world.water;
  let session = new AuthoringSession(project);
  const setWater = (value: unknown) =>
    session.apply({
      expectedRevision: session.getSnapshot().revision,
      operations: [{ kind: "document.set", target: world.id, path: ["water"], value }],
    });
  setWater("coastal-waves");
  expect(session.inspect(world.id)).toMatchObject({ water: "coastal-waves" });
  setWater(undefined);
  session = new AuthoringSession(JSON.parse(JSON.stringify(session.getSnapshot().project)));
  setWater("coastal-waves");
  expect(session.inspect(world.id)).toMatchObject({ water: "coastal-waves" });
  expect(() => setWater("winter-sky")).toThrow("must be water");
  expect(() =>
    session.apply({
      expectedRevision: session.getSnapshot().revision,
      operations: [{ kind: "document.set", target: world.id, path: ["unknown"], value: true }],
    }),
  ).toThrow("Unknown property");
});
