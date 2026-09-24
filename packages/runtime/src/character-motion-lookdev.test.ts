import { expect, test } from "bun:test";
import { compileCharacter } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples";
import { applyCharacterMotionLookdev } from "@wrela/examples/character-motion-lookdev";
import { documentSchema, performanceIssues } from "@wrela/model";
import { auditCreatureMotion } from "./creature-motion-audit";
import { RuntimeSession } from "./session";

function character() {
  const source = createCreatureFixture("ash-warden").project.documents.find(
    (document) => document.kind === "character",
  );
  if (!source || source.kind !== "character") throw Error("Missing Warden");
  return applyCharacterMotionLookdev(source);
}
test("lookdev performance remains bounded editable source with four distinct footfall times", () => {
  const source = character();
  expect(documentSchema.safeParse(source).success).toBe(true);
  expect(performanceIssues(source.joints, source.motions, source.performance)).toEqual([]);
  expect(
    new Set(
      source.performance?.clips.find((clip) => clip.motion === "walk")?.events.map((event) => event.time),
    ).size,
  ).toBe(4);
  const walk = source.motions.find((clip) => clip.id === "walk");
  expect(walk?.keys.length).toBeLessThan(2048);
});
test("looped physical root motion advances character-space planted contact targets each cycle", async () => {
  const source = character();
  const runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.addCharacter(source.id, compileCharacter(source, "interactive"), source);
    runtime.playMotion(source.id, "walk", 0);
    for (let tick = 0; tick < 151; tick++) runtime.advance(1 / 60);
    const state = runtime.checkpoint().instances[0].creatureState;
    const contacts = Object.entries(state?.contacts ?? {});
    expect(contacts.length).toBeGreaterThan(0);
    for (const [id, planted] of contacts) {
      const authored = source.creature?.contacts.find((contact) => contact.id === id);
      if (!authored) throw Error("Unknown contact");
      expect(planted.cycle).toBe(1);
      expect(planted.target[2] - authored.target[2]).toBeCloseTo(0.42 / 0.72, 4);
    }
  } finally {
    runtime.dispose();
  }
});
test("Warden walk keeps each fully planted paw fixed in world space", async () => {
  const source = character();
  const report = await auditCreatureMotion(source, compileCharacter(source, "interactive"), "walk");
  expect(report.plantedContacts.length).toBeGreaterThanOrEqual(4);
  expect(report.maximumPlantedSlip).toBeLessThan(0.025);
  expect(report.maximumContactResidual).toBeLessThan(0.025);
}, 15000);
