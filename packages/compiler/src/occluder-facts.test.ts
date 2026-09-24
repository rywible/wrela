import { expect, test } from "bun:test";
import type { FieldDefinition, FieldNode, Vec3 } from "@wrela/model";

import { compileField } from "./field";
import { opaqueFieldFacts } from "./occluder-facts";

const node = (id: string, changes: Partial<FieldNode> = {}): FieldNode => ({
  id,
  name: id,
  kind: "sphere",
  radius: 2,
  size: [2, 2, 2],
  position: [0, 0, 0],
  rotation: [0, 0, 0],
  blend: 0,
  children: [],
  ...changes,
});
const field = (nodes: FieldNode[], root = "root"): FieldDefinition => ({
  root,
  nodes,
  bounds: { min: [-5, -5, -5], max: [5, 5, 5] },
  resolution: 24,
});

test("opaque field facts distinguish real bounds from numeric permission and respect subtraction holes", () => {
  const whole = opaqueFieldFacts(field([node("root")]), "whole");
  expect(whole.interior.length).toBe(1);
  expect(whole.evidence).toBe("real-bound");
  expect(whole.numericError).toBe("unknown");
  expect(whole.interior[0].radius).toBeCloseTo(2 / Math.sqrt(3), 6);
  const hollow = field([
    node("outer"),
    node("hole", { radius: 1 }),
    node("root", { kind: "subtract", children: ["outer", "hole"] }),
  ]);
  expect(opaqueFieldFacts(hollow, "hollow").interior).toEqual([]);
});

test("ordered CSG sphere certificates survive independent original-field samples", () => {
  const definition = field([
    node("left", { position: [-1, 0, 0] }),
    node("right", { position: [2, 0, 0] }),
    node("union", { kind: "smoothUnion", blend: 0.4, children: ["left", "right"] }),
    node("hole", { position: [0.5, 0, 0], radius: 0.4 }),
    node("root", { kind: "subtract", children: ["union", "hole"] }),
  ]);
  const facts = opaqueFieldFacts(definition, "cut");
  const reference = compileField(definition, { prune: false });
  expect(facts.interior.length).toBeGreaterThan(0);
  for (const sphere of facts.interior)
    for (let i = 0; i < 1024; i++) {
      const y = 1 - (2 * (i + 0.5)) / 1024,
        angle = i * 2.399963229728653,
        radial = Math.sqrt(1 - y * y);
      const direction = [radial * Math.cos(angle), y, radial * Math.sin(angle)];
      const point = sphere.center.map((v, axis) => v + direction[axis] * sphere.radius) as Vec3;
      expect(reference.distance(point)).toBeLessThan(0);
    }
});

test("unsupported rotations and subtraction of a smooth unknown exterior yield no proof", () => {
  expect(opaqueFieldFacts(field([node("root", { rotation: [0, 0.1, 0] })]), "rotated").interior).toEqual([]);
  const definition = field([
    node("body"),
    node("a", { position: [3, 0, 0], radius: 0.2 }),
    node("b", { position: [3.2, 0, 0], radius: 0.2 }),
    node("cut", { kind: "smoothUnion", blend: 0.2, children: ["a", "b"] }),
    node("root", { kind: "subtract", children: ["body", "cut"] }),
  ]);
  expect(opaqueFieldFacts(definition, "unknown-cut").interior).toEqual([]);
});
