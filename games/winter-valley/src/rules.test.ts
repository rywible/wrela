import { expect, test } from "bun:test";
import { WINTER_MARKERS, WinterValleyGame } from "./rules";

test("winter trail requires held nearby interaction, all three markers, and return home", () => {
  const game = new WinterValleyGame();
  game.input({ interact: true });
  for (let n = 0; n < 60; n++) game.step([0, 0, 0]);
  expect(game.inspect().restored).toHaveLength(0);
  for (const marker of WINTER_MARKERS) for (let n = 0; n < 49; n++) game.step(marker.position);
  expect(game.inspect().phase).toBe("returning");
  expect(game.drainEvents().map((event) => event.kind)).toEqual(["restored", "restored", "restored"]);
  game.step([0, 0, 0]);
  expect(game.inspect().phase).toBe("won");
  const saved = game.save();
  game.step([30, 0, 30]);
  expect(game.save()).toEqual(saved);
});
test("winter save resumes exact progress while pause and terminal states stop time", () => {
  const game = new WinterValleyGame();
  game.input({ interact: true });
  for (let n = 0; n < 49; n++) game.step(WINTER_MARKERS[0].position);
  const restored = new WinterValleyGame();
  restored.restore(game.save());
  restored.step([10, 0, 0]);
  expect(restored.save()).toEqual(game.save());
  restored.pause(false);
  for (let n = 0; n < 9100; n++) restored.step([10, 0, 0]);
  expect(restored.inspect().phase).toBe("lost");
  expect(restored.drainEvents().filter((event) => event.kind === "lost")).toHaveLength(1);
  expect(() => restored.restore({ ...game.save(), restored: ["pine", "pine"] })).toThrow();
  expect(() => restored.restore({ ...game.save(), version: 2 })).toThrow();
});
test("semantic diagonal movement is normalized and interrupted restoration resets progress", () => {
  const game = new WinterValleyGame();
  game.input({ move: [1, 1], interact: true });
  expect(Math.hypot(...game.movement())).toBeCloseTo(4.3 / 60);
  for (let n = 0; n < 30; n++) game.step(WINTER_MARKERS[0].position);
  game.step([0, 0, 0]);
  expect(game.inspect().progress).toBe(0);
  game.pause();
  expect(game.movement()).toEqual([0, 0]);
});
