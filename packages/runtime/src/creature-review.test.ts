import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import type { CharacterDefinition } from "@wrela/model/documents";
import { contentKey } from "@wrela/model/math";
import { reviewCreature } from "./creature-review";

const fixture = () =>
  createCreatureFixture("ash-warden").project.documents.find(
    (document) => document.id === "ash-warden",
  ) as CharacterDefinition;
test("review compares planted contacts to unconstrained animation without mutating source", () => {
  const character = fixture(),
    before = contentKey(character);
  const scenario = character.creature!.reviewScenarios.find((entry) => entry.motion === "walk")!;
  const report = reviewCreature(character, scenario);
  expect(report.status).toBe("passed");
  expect(report.metrics.contactSamples).toBeGreaterThan(10);
  expect(report.metrics.baselineContactError!).toBeGreaterThan(0.1);
  expect(report.metrics.solvedContactError!).toBeLessThan(0.025);
  expect(report.visualApproval).toBe("not-reviewed");
  expect(report.captures.every((capture) => capture.status === "pending")).toBe(true);
  expect(contentKey(character)).toBe(before);
  expect(reviewCreature(character, scenario)).toEqual(report);
});
test("review does not approve stale correspondence or absent contact solvers", () => {
  const character = fixture();
  const scenario = character.creature!.reviewScenarios[0];
  character.creature!.charts[0].revision++;
  const stale = reviewCreature(character, scenario);
  expect(stale.status).toBe("unavailable");
  expect(stale.diagnostics.some((diagnostic) => diagnostic.message.includes("revision"))).toBe(true);
  character.creature!.charts[0].revision--;
  character.creature!.ikChains = [];
  const unavailable = reviewCreature(character, scenario);
  expect(unavailable.status).toBe("failed");
  expect(
    unavailable.diagnostics.some((diagnostic) => diagnostic.kind === "solver" && !diagnostic.passed),
  ).toBe(true);
});
test("foot penetration is detected even in a motion without authored contacts", () => {
  const character = fixture();
  const scenario = character.creature!.reviewScenarios.find((entry) => entry.motion === "hit")!;
  character.creature!.contacts = character.creature!.contacts.filter((contact) => contact.motion !== "hit");
  const motion = character.motions.find((entry) => entry.id === "hit")!;
  motion.keys.push(
    { joint: "root", time: 0, rotation: [0, 0, 0], translation: [0, -1, 0] },
    { joint: "root", time: motion.duration, rotation: [0, 0, 0], translation: [0, -1, 0] },
  );
  const report = reviewCreature(character, scenario);
  expect(report.metrics.contactSamples).toBe(0);
  expect(report.metrics.solvedContactError).toBeNull();
  expect(report.metrics.maximumPenetration).toBeGreaterThan(0.9);
  expect(report.status).toBe("failed");
});
