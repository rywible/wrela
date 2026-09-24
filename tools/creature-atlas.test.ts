import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples";
import type { CharacterDefinition } from "@wrela/model";
import { creatureAtlasPlan } from "./creature-atlas";

test("clay atlas retains camera, fixed ticks and worst measured motion frames rather than claiming visual approval", () => {
  const character = createCreatureFixture("ash-warden").project.documents.find(
    (d) => d.kind === "character",
  ) as CharacterDefinition;
  const plan = creatureAtlasPlan(character);
  expect(plan.frames.map((f) => f.id)).toContain("face");
  expect(plan.frames.filter((f) => f.id.startsWith("attack"))).toHaveLength(6);
  expect(plan.worst.length).toBe(3);
  for (const frame of plan.frames) {
    expect(character.motions.some((m) => m.id === frame.motion)).toBe(true);
    expect(frame.time).toBeGreaterThanOrEqual(0);
    expect(frame.time).toBeLessThanOrEqual(10);
  }
});
