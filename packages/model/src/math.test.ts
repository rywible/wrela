import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples/fixtures";
import { canonical, contentKey } from "./math";
import { parseProject } from "./validation";

test("canonical JSON omits undefined object fields and retains empty array entries as null", () => {
  expect(canonical({ z: undefined, b: 2, a: 1 })).toBe('{"a":1,"b":2}');
  const sparse = new Array<unknown>(4);
  sparse[1] = 1;
  expect(canonical(sparse)).toBe("[null,1,null,null]");
  expect(contentKey({ a: 1, optional: undefined })).toBe(contentKey({ a: 1 }));
});
test("cleared optional authored fields keep the same identity through project export and import", () => {
  const project = referenceProject(),
    character = project.documents.find((doc) => doc.kind === "character");
  if (!character || character.kind !== "character") throw Error("Missing character");
  character.field.nodes[0].material = undefined;
  character.physics.colliders = undefined;
  expect(contentKey(project)).toBe(contentKey(parseProject(JSON.parse(JSON.stringify(project)))));
});
