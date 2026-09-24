import { expect, test } from "bun:test";
import { compileCharacter } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples";
import type { CharacterDefinition } from "@wrela/model";
import { auditCreatureMotion } from "./creature-motion-audit";

test("physical root motion does not apply character-space landing targets twice", async () => {
  const character = createCreatureFixture("ash-warden").project.documents.find(
    (d) => d.kind === "character",
  ) as CharacterDefinition;
  const artifact = compileCharacter(character, "interactive");
  const audit = await auditCreatureMotion(character, artifact, "lunge");
  const end = audit.samples.at(-1);
  expect(end).toBeDefined();
  if (!end) throw Error("Missing samples");
  expect(end.root[2]).toBeCloseTo(1.5, 1);
  for (const foot of end.feet) {
    const rest = character.joints.find((j) => j.id === foot.joint);
    if (!rest) throw Error("Missing foot");
    expect(Math.abs(foot.position[2] - end.root[2] - rest.position[2])).toBeLessThan(0.04);
  }
  expect(end.maximumContactResidual).toBeLessThan(0.025);
  expect(audit.samples).toHaveLength(145);
}, 15000);
