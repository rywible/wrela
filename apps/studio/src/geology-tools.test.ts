import { expect, test } from "bun:test";
import { AuthoringSession } from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import {
  defaultTerrainGeology,
  type TerrainDefinition,
  validateProjectRelationships,
  type WorldDefinition,
} from "@wrela/model";
import { geologyPublication } from "./geology-tools";

function setup() {
  const project = referenceProject();
  const terrain = project.documents.find((document) => document.kind === "terrain") as TerrainDefinition;
  const world = project.documents.find(
    (document) => document.kind === "world" && document.terrain === terrain.id,
  ) as WorldDefinition;
  terrain.geology = defaultTerrainGeology();
  terrain.geology.formations = [
    { id: "test-cave", kind: "cave", position: [0, 0, 0], size: [12, 8, 10], opening: 0.65, resolution: 32 },
  ];
  return { project, terrain, world };
}
test("formation publication creates renderable collidable objects and linked instances atomically", () => {
  const { project, terrain, world } = setup();
  const session = new AuthoringSession(project);
  const count = project.documents.length;
  session.apply({ expectedRevision: 0, operations: geologyPublication(project, terrain) });
  const next = session.getSnapshot().project;
  expect(next.documents.length).toBe(count + 1);
  const object = next.documents.find((document) => document.generated?.generator.startsWith("geology-"));
  expect(object?.kind === "object" && object.collision).toBe("mesh");
  const placed = next.documents.find((document) => document.id === world.id) as WorldDefinition;
  expect(placed.instances.some((instance) => instance.definition === object?.id)).toBe(true);
  session.undo();
  expect(session.getSnapshot().project).toEqual(project);
  session.redo();
  expect(session.getSnapshot().project).toEqual(next);
});

test("republish updates geometry and placement without duplicate objects or instances", () => {
  const { project, terrain, world } = setup();
  const session = new AuthoringSession(project);
  session.apply({ expectedRevision: 0, operations: geologyPublication(project, terrain) });
  session.apply({
    expectedRevision: 1,
    operations: [
      {
        kind: "document.set",
        target: terrain.id,
        path: ["geology", "formations", 0, "position"],
        value: [10, 1, 2],
      },
    ],
  });
  const before = session.getSnapshot().project;
  const changedTerrain = before.documents.find((document) => document.id === terrain.id) as TerrainDefinition;
  session.apply({ expectedRevision: 2, operations: geologyPublication(before, changedTerrain) });
  const after = session.getSnapshot().project;
  expect(after.documents.length).toBe(before.documents.length);
  const placed = after.documents.find((document) => document.id === world.id) as WorldDefinition;
  const cave = placed.instances.filter((instance) => instance.id.startsWith("placed-geology-"));
  expect(cave.length).toBe(1);
  expect(cave[0].position).toEqual([10, 1, 2]);
  session.apply({
    expectedRevision: 3,
    operations: [{ kind: "document.set", target: terrain.id, path: ["geology", "formations"], value: [] }],
  });
  const removed = session.getSnapshot().project;
  session.apply({
    expectedRevision: 4,
    operations: geologyPublication(
      removed,
      removed.documents.find((document) => document.id === terrain.id) as TerrainDefinition,
    ),
  });
  const cleaned = session
    .getSnapshot()
    .project.documents.find((document) => document.id === world.id) as WorldDefinition;
  expect(cleaned.instances.some((instance) => instance.id.startsWith("placed-geology-"))).toBe(false);
});

test("publication requires a linked world and protects identity collisions", () => {
  const { project, terrain } = setup();
  expect(() =>
    geologyPublication(
      { ...project, documents: project.documents.filter((document) => document.kind !== "world") },
      terrain,
    ),
  ).toThrow("world");
  const operation = geologyPublication(project, terrain).find(
    (operation) => operation.kind === "document.create",
  );
  if (operation?.kind !== "document.create") throw new Error("Expected generated document");
  project.documents.push({ ...operation.document, generated: undefined });
  expect(() => geologyPublication(project, terrain)).toThrow("identity conflicts");
});

test("formation material overrides validate references and republish without stale dependencies", () => {
  const { project, terrain } = setup();
  if (!terrain.geology) throw new Error("Missing geology");
  const material = project.documents.find((item) => item.kind === "material" && item.id !== terrain.material);
  if (!material) throw new Error("Expected alternate reference material");
  terrain.geology.formations[0].material = "missing-rock";
  expect(validateProjectRelationships(project).some((issue) => issue.message.includes("missing-rock"))).toBe(
    true,
  );
  terrain.geology.formations[0].material = terrain.id;
  expect(
    validateProjectRelationships(project).some((issue) => issue.message.includes("must be material")),
  ).toBe(true);
  delete terrain.geology.formations[0].material;
  const session = new AuthoringSession(project);
  session.apply({ expectedRevision: 0, operations: geologyPublication(project, terrain) });
  session.apply({
    expectedRevision: 1,
    operations: [
      {
        kind: "document.set",
        target: terrain.id,
        path: ["geology", "formations", 0],
        value: { ...terrain.geology.formations[0], material: material.id },
      },
    ],
  });
  const before = session.getSnapshot().project;
  const nextTerrain = before.documents.find((item) => item.id === terrain.id) as TerrainDefinition;
  session.apply({ expectedRevision: 2, operations: geologyPublication(before, nextTerrain) });
  const object = session
    .getSnapshot()
    .project.documents.find((item) => item.generated?.generator.startsWith("geology-"));
  if (object?.kind !== "object") throw new Error("Published object missing");
  expect(object.material).toBe(material.id);
  expect(object.dependencies).not.toContain(terrain.material);
  expect(validateProjectRelationships(session.getSnapshot().project)).toEqual([]);
  session.undo();
  expect(session.getSnapshot().project).toEqual(before);
});
