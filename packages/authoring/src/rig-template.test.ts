import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type Bounds, type CharacterDefinition, parseProject } from "@wrela/model";
import { type RigTemplateKind, rigTemplate } from "./rig-template";
import { AuthoringSession } from "./session";

const kinds: RigTemplateKind[] = ["biped", "quadruped", "chain"];
const unit: Bounds = { min: [0, 0, 0], max: [1, 1, 1] };
test("rig templates have stable rooted hierarchies, safe rest poses and validate as authored characters", () => {
  for (const kind of kinds) {
    const project = referenceProject(),
      character = project.documents.find(
        (document): document is CharacterDefinition => document.kind === "character",
      );
    if (!character) throw Error("Missing character");
    character.joints = rigTemplate(kind, character.field.bounds);
    character.motions = [];
    expect(() => parseProject(project)).not.toThrow();
    expect(character.joints.filter((joint) => joint.parent === null).map((joint) => joint.id)).toEqual([
      "root",
    ]);
    expect(new Set(character.joints.map((joint) => joint.id)).size).toBe(character.joints.length);
    for (const joint of character.joints) {
      const lineage = new Set([joint.id]);
      let parent = joint.parent;
      while (parent) {
        expect(lineage.has(parent)).toBe(false);
        lineage.add(parent);
        const ancestor = character.joints.find((candidate) => candidate.id === parent);
        if (!ancestor) throw Error(`Missing parent ${parent}`);
        parent = ancestor.parent;
      }
      expect(joint.minimum).toBeLessThanOrEqual(0);
      expect(joint.maximum).toBeGreaterThanOrEqual(0);
      expect(joint.minimum).toBeGreaterThanOrEqual(-Math.PI);
      expect(joint.maximum).toBeLessThanOrEqual(Math.PI);
      expect(joint.rotation).toEqual([0, 0, 0]);
    }
    expect(character.joints.map((joint) => joint.id)).toEqual(
      rigTemplate(kind, unit).map((joint) => joint.id),
    );
  }
});
test("all anchors follow translated nonuniform bounds and radii scale uniformly within schema limits", () => {
  const transformed: Bounds = { min: [-8, 5, 20], max: [-4, 8, 27] };
  for (const kind of kinds) {
    const baseline = rigTemplate(kind, unit),
      changed = rigTemplate(kind, transformed),
      doubled = rigTemplate(kind, { min: [0, 0, 0], max: [2, 2, 2] });
    changed.forEach((joint, index) => {
      for (let axis = 0; axis < 3; axis++) {
        expect(joint.position[axis]).toBeCloseTo(
          transformed.min[axis] +
            baseline[index].position[axis] * (transformed.max[axis] - transformed.min[axis]),
          8,
        );
        expect(joint.position[axis]).toBeGreaterThan(transformed.min[axis]);
        expect(joint.position[axis]).toBeLessThan(transformed.max[axis]);
      }
      expect(doubled[index].radius).toBeCloseTo(baseline[index].radius * 2, 8);
    });
    for (const bounds of [
      { min: [0, 0, 0], max: [0.001, 0.001, 0.001] },
      { min: [0, 0, 0], max: [200, 200, 200] },
    ] as Bounds[])
      for (const joint of rigTemplate(kind, bounds)) {
        expect(joint.radius).toBeGreaterThanOrEqual(0.01);
        expect(joint.radius).toBeLessThanOrEqual(10);
      }
  }
  for (const bounds of [
    { min: [0, 0, 0], max: [0, 1, 1] },
    { min: [0, 0, 0], max: [201, 1, 1] },
    { min: [0, 0, 0], max: [NaN, 1, 1] },
  ] as Bounds[])
    expect(() => rigTemplate("biped", bounds)).toThrow("bounds");
});
test("replacing a rig and clearing incompatible motions is one reversible validated transaction", () => {
  const session = new AuthoringSession(referenceProject()),
    original = session.inspect("polar-bunny") as CharacterDefinition;
  const joints = rigTemplate("quadruped", original.field.bounds);
  session.apply({
    expectedRevision: 0,
    label: "Replace rig",
    operations: [
      { kind: "document.set", target: original.id, path: ["joints"], value: joints },
      { kind: "document.set", target: original.id, path: ["motions"], value: [] },
    ],
  });
  expect((session.inspect(original.id) as CharacterDefinition).joints).toEqual(joints);
  expect((session.inspect(original.id) as CharacterDefinition).motions).toEqual([]);
  session.undo();
  expect(session.inspect(original.id)).toEqual(original);
  session.redo();
  expect((session.inspect(original.id) as CharacterDefinition).joints).toEqual(joints);
});
