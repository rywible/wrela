import { expect, test } from "bun:test";
import { cookProject, createCookedCompiler } from "@wrela/compiler";
import { type CreatureFixtureId, createCreatureFixture } from "@wrela/examples";
import { CreatureStudyRun, selectedCreatureStudy, supportsCreatureStudy } from "./player";

for (const id of ["ash-warden", "reed-penitent"] as CreatureFixtureId[]) {
  test(`${id} exported cooked study retains authored source and advances one runtime tick per game tick`, async () => {
    const fixture = createCreatureFixture(id);
    const character = fixture.project.documents.find((document) => document.id === id);
    if (character?.kind !== "character") throw Error("Missing character");
    // A real source edit must survive delivery; a newly generated fixture would lose this mark.
    character.name = "Exported source edit";
    const idle = character.motions.find((motion) => motion.id === "idle");
    if (!idle) throw Error("Missing idle");
    idle.duration += 0.125;
    expect(selectedCreatureStudy(fixture.project, fixture.project.entry)).toBe(id);
    const source = JSON.stringify(fixture.project);
    const cooked = JSON.parse(
      JSON.stringify(cookProject(fixture.project, "interactive", "player-creature-test")),
    );
    const compiler = createCookedCompiler(cooked, fixture.project); // No fallback: source compile would fail.
    const compiled: string[] = [];
    const run = await CreatureStudyRun.create(
      fixture.project,
      id,
      {
        compile: async (document, quality) => {
          compiled.push(document.id);
          if (document.id === id && document.kind === "character")
            expect(document.motions.find((motion) => motion.id === "idle")?.duration).toBe(idle.duration);
          return compiler(document, quality);
        },
      },
      "interactive",
    );
    try {
      expect(compiled).toContain(id);
      const start = run.inspect().snapshot;
      run.input({ move: [1, 0], dodge: true, strike: false });
      run.update(1 / 60);
      run.input({ dodge: false });
      run.update(5 / 60);
      expect(run.inspect().snapshot.tick).toBe(6);
      expect(run.host.runtime?.clock.tick).toBe(6);
      expect(run.inspect().snapshot.playerPosition[0]).toBeGreaterThan(start.playerPosition[0]);
      expect(run.inspect().snapshot.events.some((event) => event.kind === "dodge")).toBe(true);
      expect(run.scene().surfaces.some((surface) => surface.source === id)).toBe(true);
      expect(run.scene().surfaces.some((surface) => surface.id === "creature-encounter-player")).toBe(true);
      run.pause();
      run.update(0.1);
      expect(run.host.runtime?.clock.tick).toBe(6);
      run.resume();
      run.update(1 / 60);
      expect(run.inspect().snapshot.tick).toBe(7);
      const position = run.inspect().snapshot.playerPosition;
      run.update(1 / 60);
      // Pause clears held movement. Dodge can still finish its physical motion.
      expect(run.inspect().snapshot.playerPosition[0] - position[0]).toBeLessThan(0.3);
      expect(() => run.input({ move: [Number.NaN, 0] })).toThrow();
      expect(() => run.update(Infinity)).toThrow();
      expect(JSON.stringify(fixture.project)).toBe(source);
    } finally {
      run.dispose();
    }
    run.dispose();
    expect(run.inspect().active).toBe(false);
    expect(run.host.runtime?.physics.hasBody(id)).toBe(false);
    expect(() => run.update(1 / 60)).toThrow("closed");
    expect(() => run.scene()).toThrow("closed");
  });
}

test("player does not offer generated encounters for arbitrary characters and rejects invalid authored clips", async () => {
  const fixture = createCreatureFixture("ash-warden");
  expect(supportsCreatureStudy(fixture.project, "not-a-study")).toBe(false);
  await expect(CreatureStudyRun.create(fixture.project, "not-a-study", {})).rejects.toThrow("Select");
  const character = fixture.project.documents.find((document) => document.id === "ash-warden");
  if (character?.kind !== "character") throw Error("Missing character");
  character.motions = character.motions.filter((motion) => motion.id !== "lunge");
  await expect(CreatureStudyRun.create(fixture.project, "ash-warden", {}, "interactive")).rejects.toThrow(
    "authored lunge",
  );
});
