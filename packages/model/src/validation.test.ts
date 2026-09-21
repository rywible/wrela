import { describe, expect, test } from "bun:test";
import type {
  CharacterDefinition,
  ObjectDefinition,
  Project,
  TerrainDefinition,
  WorldDefinition,
} from "./documents";
import { referenceProject } from "./fixtures";
import { parseProject, validateProject } from "./validation";

const bunny = (p: Project) => p.documents.find((d) => d.kind === "character") as CharacterDefinition;
const stone = (p: Project) => p.documents.find((d) => d.kind === "object") as ObjectDefinition;
const invalid = (p: Project, pattern: RegExp) => {
  const result = validateProject(p);
  expect(result.project).toBeUndefined();
  expect(result.diagnostics.some((d) => pattern.test(d.message))).toBe(true);
};
describe("import validation", () => {
  test("reference project round trips through the complete validator", () => {
    const project = referenceProject();
    expect(parseProject(JSON.parse(JSON.stringify(project)))).toEqual(project);
  });
  test("rejects duplicate documents, absent references, and kind mismatch", () => {
    let p = referenceProject();
    p.documents.push(structuredClone(p.documents[0]));
    invalid(p, /Duplicate document/);
    p = referenceProject();
    stone(p).material = "missing";
    invalid(p, /Missing definition/);
    p = referenceProject();
    stone(p).material = "daylight";
    invalid(p, /must be material/);
  });
  test("rejects cyclic and excessive field graphs with source diagnostic", () => {
    const p = referenceProject(),
      d = stone(p),
      n = d.field.nodes[0];
    n.kind = "union";
    n.children = [n.id, n.id];
    const result = validateProject(p);
    expect(result.project).toBeUndefined();
    expect(
      result.diagnostics.some((x) => x.document === d.id && x.node === n.id && /cycle/.test(x.message)),
    ).toBe(true);
    const deep = referenceProject(),
      f = stone(deep).field,
      leaf = f.nodes[0];
    for (let i = 0; i < 35; i++) {
      const id = `group-${i}`;
      f.nodes.push({ ...structuredClone(leaf), id, kind: "union", children: [f.root, f.root] });
      f.root = id;
    }
    invalid(deep, /32 levels/);
  });
  test("rejects malformed extraction bounds and negative shape extents", () => {
    let p = referenceProject();
    stone(p).field.bounds.max[0] = stone(p).field.bounds.min[0];
    invalid(p, /positive extent/);
    p = referenceProject();
    stone(p).field.nodes[0].size[0] = -1;
    invalid(p, /positive/);
  });
  test("rejects skeletal cycles, reversed joint limits, orphaned and duplicate motion keys", () => {
    let p = referenceProject();
    bunny(p).joints[0].parent = bunny(p).joints[1].id;
    invalid(p, /cycle/);
    p = referenceProject();
    bunny(p).joints[0].minimum = 0.5;
    bunny(p).joints[0].maximum = -0.5;
    invalid(p, /minimum/);
    p = referenceProject();
    bunny(p).motions[0].keys[0].joint = "absent";
    invalid(p, /missing joint/);
    p = referenceProject();
    bunny(p).motions[0].keys.push(structuredClone(bunny(p).motions[0].keys[0]));
    invalid(p, /Duplicate motion key/);
    p = referenceProject();
    bunny(p).motions[0].keys[0].time = 100;
    invalid(p, /outside its duration/);
  });
  test("rejects incompatible world targets and document dependency cycles", () => {
    let p = referenceProject();
    (p.documents.find((d) => d.kind === "world") as WorldDefinition).terrain = "polar-bunny";
    invalid(p, /must be terrain/);
    p = referenceProject();
    p.documents[0].dependencies = [p.documents[1].id];
    p.documents[1].dependencies = [p.documents[0].id];
    invalid(p, /dependencies contain a cycle/);
  });
  test("rejects non-finite numbers, path identities, unsupported versions and excessive work", () => {
    let p = referenceProject();
    (p.documents.find((d) => d.kind === "terrain") as TerrainDefinition).frequency = Infinity;
    invalid(p, /finite|number/);
    p = referenceProject();
    p.documents[0].id = "../escape";
    invalid(p, /pattern/);
    p = referenceProject();
    (p as unknown as { schemaVersion: number }).schemaVersion = 2;
    invalid(p, /1/);
    p = referenceProject();
    stone(p).field.resolution = 1000;
    invalid(p, /80/);
  });
});

describe("stable source identities and complete graph validation", () => {
  test("field node material overrides must refer to material definitions", () => {
    const project = referenceProject();
    stone(project).field.nodes[0].material = "daylight";
    invalid(project, /material/);
  });
  test("unreachable field nodes are still validated before entering authored storage", () => {
    const project = referenceProject();
    const source = stone(project).field;
    source.nodes.push({
      ...structuredClone(source.nodes[0]),
      id: "orphan",
      kind: "union",
      children: ["orphan", source.root],
    });
    invalid(project, /cycle/);
  });
  test("motion, intervention, instance and population identities are unique within their owner", () => {
    let project = referenceProject();
    bunny(project).motions.push(structuredClone(bunny(project).motions[0]));
    invalid(project, /unique|Duplicate/i);
    project = referenceProject();
    const terrain = project.documents.find((d) => d.kind === "terrain") as TerrainDefinition;
    terrain.interventions.push(structuredClone(terrain.interventions[0]));
    invalid(project, /unique|Duplicate/i);
    project = referenceProject();
    const world = project.documents.find((d) => d.kind === "world") as WorldDefinition;
    world.instances.push(structuredClone(world.instances[0]));
    invalid(project, /unique|Duplicate/i);
    project = referenceProject();
    const populations = (project.documents.find((d) => d.kind === "world") as WorldDefinition).populations;
    populations.push(structuredClone(populations[0]));
    invalid(project, /unique|Duplicate/i);
  });
  test("shared field subgraphs cannot expand evaluation exponentially", () => {
    const project = referenceProject(),
      source = stone(project).field,
      leaf = source.nodes[0];
    for (let index = 0; index < 10; index++) {
      const id = `repeat-${index}`;
      source.nodes.push({
        ...structuredClone(leaf),
        id,
        kind: "union",
        children: [source.root, source.root, source.root, source.root],
      });
      source.root = id;
    }
    invalid(project, /512|budget|operations|cost/i);
  });
});
