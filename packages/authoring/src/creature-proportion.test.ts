import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema, type FieldNode, type Vec3 } from "@wrela/model";
import { AuthoringSession } from "./session";

// Independent closed-form Rz Ry Rx reference, rather than the source editor's
// sequential axis rotation implementation.
function matrix([x, y, z]: Vec3): number[][] {
  const sx = Math.sin(x),
    cx = Math.cos(x),
    sy = Math.sin(y),
    cy = Math.cos(y),
    sz = Math.sin(z),
    cz = Math.cos(z);
  return [
    [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx],
    [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx],
    [-sy, cy * sx, cy * cx],
  ];
}
const times = (m: number[][], p: Vec3): Vec3 =>
  m.map((row) => row.reduce((sum, value, i) => sum + value * p[i], 0)) as Vec3;
const transpose = (m: number[][]) => [0, 1, 2].map((axis) => m.map((row) => row[axis]));
const add = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v + b[i]) as Vec3;
const sub = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v - b[i]) as Vec3;
const multiply = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v * b[i]) as Vec3;
function fixture(rotation: Vec3, frameRotation: Vec3 = [0, 0, 0], kind: "box" | "ellipsoid" = "ellipsoid") {
  const project = referenceProject(),
    character = project.documents.find((d) => d.kind === "character") as CharacterDefinition;
  const node = character.field.nodes.find((n) => n.id === "body");
  if (!node) throw Error("fixture");
  Object.assign(node, { position: [0.5, 1.2, -0.4], rotation, size: [0.3, 0.5, 0.7], kind });
  const origin: Vec3 = [0.2, 0.4, -0.3];
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "shape",
        name: "Shape",
        nodeIds: ["body"],
        frame: { position: origin, rotation: frameRotation },
        extent: [1, 1, 1],
      },
    ],
  });
  return { session: new AuthoringSession(project), original: structuredClone(node), origin, frameRotation };
}
function verifyAffine(
  rotation: Vec3,
  scale: Vec3,
  frameRotation: Vec3 = [0, 0, 0],
  kind: "box" | "ellipsoid" = "ellipsoid",
) {
  const fixtureData = fixture(rotation, frameRotation, kind),
    { session, original, origin } = fixtureData;
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "creature.proportion", target: "polar-bunny", region: "shape", scale }],
  });
  const edited = session.inspectCreature("polar-bunny").nodes[0];
  const world = (node: FieldNode, p: Vec3) =>
    add(node.position, times(matrix(node.rotation), multiply(node.size, p)));
  const affine = (p: Vec3) =>
    add(
      origin,
      times(matrix(frameRotation), multiply(times(transpose(matrix(frameRotation)), sub(p, origin)), scale)),
    );
  const samples: Vec3[] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 1, 1],
    [-1, 1, -1],
    [0.27, -0.43, 0.68],
  ];
  for (const point of samples) {
    const actual = world(edited, point),
      expected = affine(world(original, point));
    for (let axis = 0; axis < 3; axis++) expect(actual[axis]).toBeCloseTo(expected[axis], 11);
  }
  expect(edited.rotation).toEqual(original.rotation);
  return { original, edited, session };
}
test("X rotation commutes with X-only scaling when Y and Z factors agree", () => {
  const { original, edited } = verifyAffine([0.73, 0, 0], [1.7, 1, 1]);
  expect(edited.size).toEqual([original.size[0] * 1.7, original.size[1], original.size[2]]);
});
test("arbitrary rotations retain exact affine sampling under uniform scaling", () => {
  const { edited } = verifyAffine([0.47, -0.63, 0.91], [1.4, 1.4, 1.4], [0.31, 0.2, -0.8]);
  expect(edited.size).toEqual([0.3 * 1.4, 0.5 * 1.4, 0.7 * 1.4]);
});
test("a true axis permutation remaps scale into primitive local axes", () => {
  const { edited } = verifyAffine([0, 0, Math.PI / 2], [1.7, 1, 0.8]);
  expect(edited.size).toEqual([0.3, 0.5 * 1.7, 0.7 * 0.8]);
});
test("edit frames with the same rotated basis preserve exact nonuniform scaling", () => {
  const rotation: Vec3 = [0.2, -0.4, 0.6];
  const { edited } = verifyAffine(rotation, [1.1, 1.4, 0.8], rotation);
  expect(edited.size).toEqual([0.3 * 1.1, 0.5 * 1.4, 0.7 * 0.8]);
});
test("a genuinely mixed nonuniform rotation rejects atomically instead of changing shape", () => {
  const { session } = fixture([0, 0, 0.37]),
    before = session.export();
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [
        { kind: "creature.proportion", target: "polar-bunny", region: "shape", scale: [1.7, 1, 1] },
      ],
    }),
  ).toThrow("unrepresentable shear");
  expect(session.export()).toBe(before);
  expect(session.getSnapshot().revision).toBe(0);
});
test("rotated box support remains inside source extraction bounds after a large compatible edit", () => {
  const { edited, session } = verifyAffine([0.73, 0, 0], [4, 4, 4], [0, 0, 0], "box");
  const document = session.inspect("polar-bunny") as CharacterDefinition;
  for (let corner = 0; corner < 8; corner++) {
    const local = edited.size.map((value, axis) => (corner & (1 << axis) ? value : -value)) as Vec3;
    const point = add(edited.position, times(matrix(edited.rotation), local));
    for (let axis = 0; axis < 3; axis++) {
      expect(point[axis]).toBeGreaterThanOrEqual(document.field.bounds.min[axis]);
      expect(point[axis]).toBeLessThanOrEqual(document.field.bounds.max[axis]);
    }
  }
});
