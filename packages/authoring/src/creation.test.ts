import { expect, test } from "bun:test";
import { type Document, type Project, parseProject, referenceProject, references } from "@wrela/model";
import { planDefinitionCreation } from "./creation";
import { AuthoringSession } from "./session";

function minimal(): Project {
  const material = referenceProject().documents.find((document) => document.kind === "material");
  if (!material) throw Error("Missing material fixture");
  return {
    schemaVersion: 1,
    id: "minimal",
    name: "Minimal",
    entry: "custom",
    documents: [{ ...material, id: "custom" }],
  };
}
test("every kind can be created atomically in a minimal imported project and undone completely", () => {
  const kinds = new Set(referenceProject().documents.map((document) => document.kind));
  for (const kind of kinds) {
    const project = minimal(),
      before = structuredClone(project),
      session = new AuthoringSession(project);
    const plan = planDefinitionCreation(session.getSnapshot().project, kind);
    expect(project).toEqual(before);
    expect(plan.document.kind).toBe(kind);
    const result = session.apply({
      expectedRevision: 0,
      operations: [...plan.dependencies, plan.document].map((document) => ({
        kind: "document.create",
        document,
      })),
    });
    expect(result.changed.length).toBe(plan.dependencies.length + 1);
    const created = session.getSnapshot().project;
    expect(() => parseProject(created)).not.toThrow();
    for (const document of [...plan.dependencies, plan.document]) {
      expect(session.getSnapshot().documentRevisions[document.id]).toBe(1);
      for (const ref of references(document))
        expect(created.documents.some((target) => target.id === ref)).toBe(true);
    }
    session.undo();
    expect(session.getSnapshot().project).toEqual(before);
  }
});
test("new worlds contain only required dependencies, while character feature materials stay distinct", () => {
  const project = minimal(),
    world = planDefinitionCreation(project, "world"),
    character = planDefinitionCreation(project, "character");
  expect(world.dependencies.map((document) => document.kind).sort()).toEqual([
    "environment",
    "lighting",
    "terrain",
  ]);
  expect(world.document).toMatchObject({ populations: [], instances: [] });
  expect(character.dependencies.map((document) => document.kind)).toEqual([
    "material",
    "material",
    "material",
  ]);
  if (character.document.kind !== "character") throw Error("Wrong kind");
  expect(character.document.material).toBe("custom");
  const accents = character.document.field.nodes.flatMap((node) => (node.material ? [node.material] : []));
  expect(new Set(accents).size).toBe(3);
  expect(accents).not.toContain("custom");
});
test("same-ID wrong-kind documents are never reused as dependencies", () => {
  const project = minimal();
  project.documents[0].id = "winter-sky";
  project.entry = "winter-sky";
  const plan = planDefinitionCreation(project, "world");
  if (plan.document.kind !== "world") throw Error("Wrong kind");
  expect(plan.document.environment).not.toBe("winter-sky");
  const environmentId = plan.document.environment;
  expect(plan.dependencies.find((document) => document.id === environmentId)?.kind).toBe("environment");
  expect(() =>
    parseProject({ ...project, documents: [...project.documents, ...plan.dependencies, plan.document] }),
  ).not.toThrow();
});
test("cloning a generated definition detaches its source and preserves existing dependency meaning", () => {
  const project = referenceProject(),
    original = project.documents.find((document) => document.kind === "character");
  if (!original) throw Error("Missing character");
  original.generated = { generator: "creatures-v2", policy: "locked" };
  original.dependencies = ["winter-sky"];
  const plan = planDefinitionCreation(project, "character");
  expect(plan.dependencies).toEqual([]);
  expect(plan.document.generated).toEqual({ generator: "creatures-v2", policy: "detached" });
  expect(plan.document.dependencies).toEqual(["winter-sky"]);
  expect(plan.document.id).not.toBe(original.id);
  expect(original.generated.policy).toBe("locked");
  const session = new AuthoringSession(project);
  session.apply({
    expectedRevision: 0,
    operations: [
      { kind: "document.create", document: plan.document },
      { kind: "document.rename", target: plan.document.id, name: "My creature" },
    ],
  });
  expect((session.inspect(plan.document.id) as Document).name).toBe("My creature");
});
