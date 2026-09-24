import { describe, expect, test } from "bun:test";
import { type BotanicalGrowthState, botanicalDevelopmentSchema } from "@wrela/model";

import { advanceBotanicalGrowth, growBotanical, seedBotanicalGrowth } from "./botanical-growth";

const source = (species: "lodgepole-pine" | "paper-birch" = "lodgepole-pine", steps = 16) =>
  botanicalDevelopmentSchema.parse({ species, steps });
function valid(state: BotanicalGrowthState) {
  const shoots = new Map(state.shoots.map((shoot) => [shoot.id, shoot]));
  expect(shoots.size).toBe(state.shoots.length);
  expect(new Set(state.buds.map((bud) => bud.id)).size).toBe(state.buds.length);
  for (const shoot of state.shoots) {
    expect([...shoot.start, ...shoot.end, shoot.radius, shoot.tipRadius].every(Number.isFinite)).toBe(true);
    expect(shoot.radius).toBeGreaterThanOrEqual(shoot.tipRadius);
    if (shoot.parent) {
      expect(shoots.has(shoot.parent)).toBe(true);
      const parent = shoots.get(shoot.parent);
      if (!parent) throw Error("Missing parent");
      expect(parent.birth).toBeLessThan(shoot.birth);
      for (let axis = 0; axis < 3; axis++)
        expect(shoot.start[axis]).toBeCloseTo(
          parent.start[axis] + (parent.end[axis] - parent.start[axis]) * (shoot.attachment ?? 1),
          10,
        );
    }
  }
  for (const entry of state.ledger) {
    expect(entry.end).toBeGreaterThanOrEqual(0);
    expect(entry.start + entry.assimilated - entry.maintenance - entry.growth - entry.lost).toBeCloseTo(
      entry.end,
      9,
    );
  }
}
describe("persistent developmental growth", () => {
  for (const species of ["lodgepole-pine", "paper-birch"] as const)
    test(`${species}: replay, continuation, stable organs, and resources`, () => {
      const options = source(species);
      const young = growBotanical(42, { ...options, steps: 8 });
      const adult = growBotanical(42, options, JSON.parse(JSON.stringify(young)));
      expect(adult).toEqual(growBotanical(42, options));
      for (const shoot of young.shoots) {
        const later = adult.shoots.find((s) => s.id === shoot.id);
        if (!later) throw Error("Missing later organ");
        expect(later.start).toEqual(shoot.start);
        expect(later.end).toEqual(shoot.end);
        expect(later.radius).toBeGreaterThanOrEqual(shoot.radius);
      }
      valid(adult);
      expect(adult.shoots.some((s) => s.order >= 2)).toBe(true);
    });
  test("same seed responds to resources and neighbor shade", () => {
    const options = source();
    const open = growBotanical(1, options);
    const dry = growBotanical(1, { ...options, environment: { ...options.environment, water: 0.1 } });
    const shaded = growBotanical(1, {
      ...options,
      environment: {
        ...options.environment,
        neighbors: [{ id: "neighbor", center: [1, 3, 0], radius: 4, height: 8, opacity: 0.97 }],
      },
    });
    const length = (s: BotanicalGrowthState) =>
      s.shoots.reduce((n, s) => n + Math.hypot(...s.end.map((v, i) => v - s.start[i])), 0);
    expect(length(dry)).toBeLessThan(length(open));
    expect(length(shaded)).toBeLessThan(length(open));
    valid(dry);
    valid(shaded);
  });
  test("pruning preserves unrelated IDs and replays ordered events", () => {
    const options = source("paper-birch", 12);
    const initial = growBotanical(12, { ...options, steps: 6 });
    const limb = initial.shoots.find((s) => s.order === 1);
    if (!limb) throw Error("Missing lateral limb");
    const edited = { ...options, events: [{ id: "cut", step: 7, kind: "prune" as const, organ: limb.id }] };
    const result = growBotanical(12, edited);
    expect(result.shoots.find((s) => s.id === limb.id)?.state).toBe("pruned");
    expect(result.appliedEvents).toEqual(["cut"]);
    for (const shoot of initial.shoots) expect(result.shoots.some((s) => s.id === shoot.id)).toBe(true);
    expect(() => growBotanical(12, edited, initial)).toThrow("checkpoint");
    valid(result);
  });
  test("start-of-step iteration order cannot bias growth", () => {
    const options = source("paper-birch", 9);
    const state = growBotanical(21, { ...options, steps: 8 });
    const reversed = structuredClone(state);
    reversed.shoots.reverse();
    reversed.buds.reverse();
    expect(advanceBotanicalGrowth(reversed, options)).toEqual(advanceBotanicalGrowth(state, options));
  });
  test("dark seedlings remain bounded and source rejects duplicate events", () => {
    const options = source();
    options.environment.light = 0;
    const result = growBotanical(1, options);
    valid(result);
    expect(result.shoots.length).toBe(0);
    expect(seedBotanicalGrowth(1, source()).step).toBe(0);
    expect(() =>
      botanicalDevelopmentSchema.parse({
        ...options,
        events: [
          { id: "x", step: 1, kind: "prune", organ: "x" },
          { id: "x", step: 2, kind: "prune", organ: "y" },
        ],
      }),
    ).toThrow();
  });
});

test("uniform sky has no preferred growth direction while a neighboring canopy produces a cue", async () => {
  const { botanicalEnvironment } = await import("./botanical-environment");
  const state = { shoots: [], species: "lodgepole-pine" as const };
  const environment = { light: 1, water: 1, fertility: 1, neighbors: [] };
  expect(botanicalEnvironment(state, environment)([0, 0, 0]).direction).toEqual([0, 0, 0]);
  const shaded = botanicalEnvironment(state, {
    ...environment,
    neighbors: [{ id: "neighbor", center: [2, 3, 0], radius: 2, height: 6, opacity: 0.95 }],
  })([0, 1, 0]);
  expect(shaded.direction[0]).toBeLessThan(0);
  expect(shaded.light).toBeLessThan(1);
});

test("birch has attached perennial short shoots and bounded renewal rather than leaves on old long wood", () => {
  const options = source("paper-birch", 16);
  const state = growBotanical(42, options);
  const short = state.shoots.filter((shoot) => shoot.habit === "short");
  expect(short.length).toBeGreaterThan(0);
  expect(short.some((shoot) => (shoot.attachment ?? 1) < 0.5)).toBe(true);
  for (const shoot of short) {
    expect(Math.hypot(...shoot.end.map((v, i) => v - shoot.start[i]))).toBeLessThan(0.004);
    expect(shoot.foliage.count).toBe(3);
  }
  expect(
    state.shoots
      .filter((shoot) => shoot.habit === "long" && shoot.birth < state.step)
      .every((shoot) => shoot.foliage.retained === 0),
  ).toBe(true);
  valid(state);
});
