import { expect, test } from "bun:test";
import { compileDocument } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import type { CharacterDefinition } from "@wrela/model/documents";
import { RuntimeSession } from "@wrela/runtime/session";
import { CREATURE_ENCOUNTER_STEP, CreatureEncounterGame, type CreatureEncounterInput } from "./rules";

function setup() {
  const fixture = createCreatureFixture("ash-warden");
  const character = fixture.project.documents.find(
    (document) => document.id === fixture.characterId,
  ) as CharacterDefinition;
  return {
    fixture,
    character,
    options: { creatureId: character.id, encounter: fixture.encounter, motions: character.motions },
  };
}
const idle: CreatureEncounterInput = { move: [0, 0], dodge: false, strike: false };
test("authored attacks telegraph, damage once, recover, and eventually defeat an idle player", () => {
  const game = new CreatureEncounterGame(setup().options);
  for (let tick = 0; tick < 1800 && game.snapshot().outcome === "playing"; tick++) {
    const state = game.step(CREATURE_ENCOUNTER_STEP, idle);
    expect(Math.hypot(state.creaturePosition[0], state.creaturePosition[2])).toBeLessThanOrEqual(7.6);
  }
  const state = game.snapshot();
  expect(state.outcome).toBe("lost");
  expect(state.playerHealth).toBe(0);
  const hits = state.events.filter((event) => event.kind === "player-hit");
  expect(hits).toHaveLength(4);
  for (const hit of hits) {
    const telegraph = state.events
      .filter((event) => event.kind === "telegraph" && event.tick < hit.tick)
      .at(-1)!;
    const commit = state.events.find(
      (event) => event.kind === "commit" && event.tick > telegraph.tick && event.tick < hit.tick,
    );
    expect(commit).toBeDefined();
    expect((hit.tick - telegraph.tick) * CREATURE_ENCOUNTER_STEP).toBeGreaterThanOrEqual(0.98);
    expect((hit.tick - telegraph.tick) * CREATURE_ENCOUNTER_STEP).toBeLessThan(1.24);
  }
  const ended = game.snapshot();
  game.step(0.2, { move: [1, 0], dodge: true, strike: true });
  expect(game.snapshot()).toEqual(ended);
});
test("timed dodges have a startup and recovery, and defeat authored damage windows", () => {
  const game = new CreatureEncounterGame(setup().options);
  expect(game.step(CREATURE_ENCOUNTER_STEP, { ...idle, dodge: true }).invulnerable).toBe(false);
  for (let i = 0; i < 4; i++) game.step(CREATURE_ENCOUNTER_STEP, idle);
  expect(game.snapshot().invulnerable).toBe(true);
  for (let i = 0; i < 30; i++) game.step(CREATURE_ENCOUNTER_STEP, idle);
  expect(game.snapshot().invulnerable).toBe(false);
  game.reset();
  for (let i = 0; i < 1200; i++) {
    const prior = game.snapshot();
    game.step(CREATURE_ENCOUNTER_STEP, {
      ...idle,
      dodge: prior.phase === "attack" && prior.phaseTime > 0.2 && prior.phaseTime < 0.23,
    });
  }
  expect(game.snapshot().playerHealth).toBe(100);
  expect(game.snapshot().events.some((event) => event.kind === "dodged")).toBe(true);
});
test("strike range and cooldown prevent remote/repeated hits; recovery gives a punishable opening", () => {
  const game = new CreatureEncounterGame(setup().options);
  for (let i = 0; i < 20; i++) game.step(CREATURE_ENCOUNTER_STEP, { ...idle, strike: true });
  expect(game.snapshot().creatureHealth).toBe(96);
  expect(game.snapshot().events.filter((event) => event.kind === "strike")).toHaveLength(1);
  game.reset();
  for (let i = 0; i < 1200 && game.snapshot().outcome === "playing"; i++) {
    const state = game.snapshot();
    const dx = state.creaturePosition[0] - state.playerPosition[0],
      dz = state.creaturePosition[2] - state.playerPosition[2],
      length = Math.hypot(dx, dz);
    game.step(CREATURE_ENCOUNTER_STEP, {
      move: length > 1.4 ? [dx / length, dz / length] : [0, 0],
      dodge: false,
      strike: state.strikeReady,
    });
  }
  expect(game.snapshot().outcome).toBe("won");
  expect(game.snapshot().events.some((event) => event.kind === "enemy-hit" && event.amount === 24)).toBe(
    true,
  );
  expect(game.snapshot().events.some((event) => event.kind === "enemy-hit" && event.amount === 6)).toBe(true);
});
test("fixed tick state is reproducible across frame subdivision and restart", () => {
  const options = setup().options,
    first = new CreatureEncounterGame(options),
    second = new CreatureEncounterGame(options);
  for (let i = 0; i < 120; i++) first.step(CREATURE_ENCOUNTER_STEP, idle);
  for (let i = 0; i < 60; i++) second.step(CREATURE_ENCOUNTER_STEP * 2, idle);
  expect(first.snapshot()).toEqual(second.snapshot());
  first.reset();
  expect(first.snapshot()).toEqual(new CreatureEncounterGame(options).snapshot());
  expect(() => first.step(1, idle)).toThrow();
  first.dispose();
  expect(() => first.step(CREATURE_ENCOUNTER_STEP, idle)).toThrow("disposed");
});
test("encounter drives real RuntimeSession movement, root motion, and continuous recovery", async () => {
  const { character, options } = setup();
  const artifact = compileDocument(character, "interactive");
  if (!artifact || artifact.kind !== "character") throw new Error("Fixture did not compile");
  const runtime = await RuntimeSession.create();
  try {
    runtime.addCharacter(character.id, artifact, character);
    const initialHeight = runtime.bodyState(character.id).position[1];
    const game = new CreatureEncounterGame({ ...options, runtime });
    let maxStep = 0,
      previous = runtime.bodyState(character.id).position[2];
    for (let tick = 0; tick < 310; tick++) {
      game.step(CREATURE_ENCOUNTER_STEP, idle);
      runtime.advance(CREATURE_ENCOUNTER_STEP);
      const body = runtime.bodyState(character.id);
      maxStep = Math.max(maxStep, Math.abs(body.position[2] - previous));
      previous = body.position[2];
      expect(body.position[1]).toBeGreaterThan(initialHeight - 0.2);
    }
    expect(runtime.bodyState(character.id).position[2]).toBeGreaterThan(2);
    expect(maxStep).toBeLessThan(0.15);
    expect(game.snapshot().events.some((event) => event.kind === "recovery")).toBe(true);
    expect(() => game.step(1 / 30, idle)).toThrow("each 1/60");
  } finally {
    runtime.dispose();
  }
}, 10000);
