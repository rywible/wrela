import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples";
import { type CharacterDefinition, contentKey } from "@wrela/model";
import {
  appendCreatureObservation,
  assessCreatureObservation,
  type CreatureNotebook,
  inspectCreatureNotebook,
} from "./creature-notebook";

const character = createCreatureFixture("ash-warden").project.documents.find(
  (d) => d.kind === "character",
) as CharacterDefinition;
const book: CreatureNotebook = { version: 1, target: character.id, observations: [], assessments: [] };
const note = {
  id: "orbital",
  target: character.id,
  sourceKey: contentKey(character),
  region: "head-region",
  summary: "Eye rim protrudes",
  category: "anatomy" as const,
  priority: "important" as const,
  evidence: {
    capture: "face.png",
    motion: "idle",
    tick: 0,
    camera: {
      position: [2, 2, 4] as [number, number, number],
      target: [0, 2, 1] as [number, number, number],
      fov: 30,
    },
    channel: "clay",
  },
  intent: "Recess the orbit without moving the eye",
  protect: ["eye-left", "eye-right"],
};
test("review evidence is immutable, serializable and reopens after source edits", () => {
  const observed = appendCreatureObservation(book, note);
  expect(book.observations).toHaveLength(0);
  expect(appendCreatureObservation(observed, note).observations).toHaveLength(1);
  expect(() => appendCreatureObservation(observed, { ...note, summary: "different" })).toThrow("immutable");
  const reviewed = assessCreatureObservation(observed, {
    observation: note.id,
    sourceKey: contentKey(character),
    status: "resolved",
    reason: "Checked front and grazing-light face views",
    evidence: ["after-front.png", "after-face.png"],
  });
  expect(
    inspectCreatureNotebook(character, JSON.parse(JSON.stringify(reviewed))).observations[0].status,
  ).toBe("resolved");
  const changed = structuredClone(character);
  changed.name = "Revised";
  expect(inspectCreatureNotebook(changed, reviewed).observations[0].status).toBe("open");
  expect(reviewed.assessments).toHaveLength(1);
});
