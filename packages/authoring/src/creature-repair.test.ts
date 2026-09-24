import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples";
import { type CharacterDefinition, contentKey } from "@wrela/model";
import { type CreatureRepairRequest, proposeCreatureRepair } from "./creature-repair";
import { AuthoringSession } from "./session";

const project = createCreatureFixture("ash-warden").project;
const character = project.documents.find((d) => d.id === "ash-warden") as CharacterDefinition;
const request: CreatureRepairRequest = {
  id: "cheek-repair",
  target: character.id,
  sourceKey: contentKey(character),
  expectedRevision: 0,
  region: "head-region",
  nodeId: "skull",
  handles: [{ id: "cheek", position: [0.3, 2, 1.3], target: [0.32, 2, 1.3], tolerance: 0.0001 }],
  protectedPoints: [{ id: "opposite-eye", position: [-0.3, 2.1, 1.4], tolerance: 0.0001 }],
  support: { radii: [0.2, 0.15, 0.2], rotation: [0, 0, 0] },
  intent: "Restore cheek while preserving opposite eye",
};
test("repair fits rest handles, measures sensitivity, preserves points and creates an undoable candidate", () => {
  const session = new AuthoringSession(project);
  const result = session.repairCreature(request);
  expect(result.residuals.every((r) => r.passed)).toBe(true);
  expect(result.sensitivities[0].controls[0].weight).toBeCloseTo(1);
  expect(contentKey(session.getSnapshot().project)).toBe(contentKey(project));
  session.adoptCandidate(result.id);
  expect(contentKey(session.getSnapshot().project)).not.toBe(contentKey(project));
  session.undo();
  expect(contentKey(session.getSnapshot().project)).toBe(contentKey(project));
});
test("conflicting protected points and stale evidence reject without adoption", () => {
  expect(() => proposeCreatureRepair(character, { ...request, sourceKey: "stale" })).toThrow("stale");
  expect(() =>
    proposeCreatureRepair(character, {
      ...request,
      protectedPoints: [{ id: "pin", position: request.handles[0].position, tolerance: 0.0001 }],
    }),
  ).toThrow("conflict");
  expect(() => proposeCreatureRepair(character, { ...request, maxDisplacement: 0.005 })).toThrow("bound");
});
test("repair resolves multiple coupled controls instead of adding independent overshooting strokes", () => {
  const result = proposeCreatureRepair(character, {
    ...request,
    handles: [
      request.handles[0],
      { id: "brow", position: [0.3, 2.08, 1.3], target: [0.29, 2.08, 1.3], tolerance: 0.0001 },
    ],
  });
  expect(result.residuals.every((r) => r.passed)).toBe(true);
  expect(result.sensitivities[0].controls[1].weight).toBeGreaterThan(0);
});
