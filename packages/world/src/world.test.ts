import { describe, expect, test } from "bun:test";
import { referenceProject, type TerrainDefinition, type WorldDefinition } from "@wrela/model";
import { absolutePosition, relativePosition, worldPosition } from "./coordinates";
import { planTerrain } from "./planner";
import { generatePlacements, PersistentWorldState } from "./population";
import { BoundedScheduler } from "./scheduler";
import { WorldSession } from "./session";

const project = referenceProject(),
  terrain = project.documents.find((d) => d.kind === "terrain") as TerrainDefinition,
  world = project.documents.find((d) => d.kind === "world") as WorldDefinition;
describe("world coordinates and identity", () => {
  test("negative cells normalize and preserve large offsets across rebases", () => {
    const position = worldPosition([-0.25, 0, 2 ** 29 + 0.125]);
    expect(position.cell[0]).toBe(-1);
    expect(position.offset[0]).toBe(255.75);
    expect(absolutePosition(position)).toEqual([-0.25, 0, 2 ** 29 + 0.125]);
    expect(relativePosition(position, worldPosition([0, 0, 2 ** 29]))).toEqual([-0.25, 0, 0.125]);
    expect(() => worldPosition([2 ** 31, 0, 0])).toThrow();
  });
  test("canonical populations agree across independently requested regions", () => {
    const whole = generatePlacements(world, terrain, { minX: -64, minZ: -64, maxX: 64, maxZ: 64 });
    const parts = [-64, 0].flatMap((minX) =>
      generatePlacements(world, terrain, { minX, minZ: -64, maxX: minX + 64, maxZ: 64 }),
    );
    expect(parts.sort((a, b) => a.id.localeCompare(b.id))).toEqual(
      whole.sort((a, b) => a.id.localeCompare(b.id)),
    );
    expect(new Set(whole.map((p) => p.id)).size).toBe(whole.length);
  });
  test("overrides and dormant entities survive save/reopen, incompatible generators fail", () => {
    const state = new PersistentWorldState(world.id, world.generatorVersion);
    state.overrides.set("tree/-1/3", { removed: true });
    state.sleep({
      id: "traveler",
      definition: "polar-bunny",
      position: [-200, 4, 0],
      velocity: [1, 0, 0],
      state: { health: 80 },
    });
    const reopened = new PersistentWorldState(world.id, world.generatorVersion);
    reopened.load(state.save());
    expect(reopened.overrides.get("tree/-1/3")?.removed).toBe(true);
    expect(reopened.wake("traveler")?.state.health).toBe(80);
    expect(reopened.dormant.size).toBe(0);
    expect(() => new PersistentWorldState(world.id, "future").load(state.save())).toThrow();
  });
});
describe("bounded streaming", () => {
  test("two interest sources retain independent coverage with balanced stitched adjacency", () => {
    const plan = planTerrain(
      [
        { id: "camera", position: [-200, 2, 0], visualRadius: 200, collisionRadius: 0 },
        { id: "body", position: [350, 2, -20], visualRadius: 20, collisionRadius: 40 },
      ],
      { maxPatches: 120 },
    );
    expect(plan.length).toBeLessThanOrEqual(120);
    expect(plan.some((p) => p.collision && 350 >= p.x && 350 <= p.x + p.size)).toBe(true);
    let stitched = 0;
    for (const a of plan)
      for (const b of plan) {
        const vertical =
            (a.x + a.size === b.x || b.x + b.size === a.x) &&
            Math.max(a.z, b.z) < Math.min(a.z + a.size, b.z + b.size),
          horizontal =
            (a.z + a.size === b.z || b.z + b.size === a.z) &&
            Math.max(a.x, b.x) < Math.min(a.x + a.size, b.x + b.size);
        if (vertical || horizontal) expect(Math.abs(a.level - b.level)).toBeLessThanOrEqual(1);
        if ((vertical || horizontal) && a.level + 1 === b.level) {
          expect(Object.values(a.stitch).some(Boolean)).toBe(true);
          stitched++;
        }
      }
    expect(stitched).toBeGreaterThan(0);
  });
  test("planner output is independent of interest insertion order", () => {
    const a = {
        id: "a",
        position: [-120, 0, 0] as [number, number, number],
        visualRadius: 120,
        collisionRadius: 20,
      },
      b = {
        id: "b",
        position: [240, 0, 40] as [number, number, number],
        visualRadius: 100,
        collisionRadius: 20,
      };
    expect(planTerrain([a, b])).toEqual(planTerrain([b, a]));
  });
  test("scheduler enforces concurrency and reports superseded work", async () => {
    const scheduler = new BoundedScheduler<number>(2, 4);
    let active = 0,
      peak = 0;
    const jobs = Array.from({ length: 6 }, (_, i) =>
      scheduler.request(String(i), i, async () => {
        peak = Math.max(peak, ++active);
        await Bun.sleep(5);
        active--;
        return i;
      }),
    );
    expect(await Promise.all(jobs)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(peak).toBe(2);
    scheduler.dispose();
    await expect(scheduler.request("late", 0, async () => 0)).rejects.toThrow();
  });
  test("rapid travel bounds retained products and collision readiness", async () => {
    const session = new WorldSession(world, terrain, {
      maxPatches: 60,
      maxCacheBytes: 100000,
      resolution: 8,
    });
    expect(session.queryGround(0, 0).status).toBe("not-resident");
    for (let step = 0; step < 12; step++) {
      const position: [number, number, number] = [step * 130 - 700, 0, step * -30];
      const ready = await session.teleport(position, { visualRadius: 80, collisionRadius: 24 });
      expect(ready.ready).toBe(true);
      expect(session.queryGround(position[0], position[2]).status).toBe("ready");
      expect(session.metrics.resident).toBeLessThanOrEqual(60);
      expect(session.metrics.cacheBytes).toBeLessThanOrEqual(100000);
    }
    session.dispose();
  }, 15000);
  test("edits preserve the complete old revision until all replacements are ready", async () => {
    const session = new WorldSession(world, terrain, { maxPatches: 40, resolution: 8 });
    await session.teleport([0, 0, 0], { visualRadius: 50, collisionRadius: 20 });
    const before = session.residentPatches(),
      revision = session.metrics.revision;
    session.replaceTerrain({ ...terrain, baseHeight: terrain.baseHeight + 4 });
    expect(session.residentPatches()).toBe(before);
    expect(session.metrics.revision).toBe(revision);
    const ready = await session.prepare();
    expect(ready.ready).toBe(true);
    expect(new Set(session.residentPatches().map((p) => p.revision)).size).toBe(1);
    expect(session.metrics.revision).toBeGreaterThan(revision);
    session.dispose();
  });
  test("superseded edits never publish after latest request", async () => {
    const session = new WorldSession(world, terrain, { maxPatches: 40, resolution: 8 });
    session.setInterest({ id: "player", position: [0, 0, 0], visualRadius: 50, collisionRadius: 20 });
    session.update();
    session.replaceTerrain({ ...terrain, baseHeight: 30, interventions: [] });
    await session.prepare();
    const query = session.queryGround(0, 0);
    expect(query.status).toBe("ready");
    if (query.status === "ready") expect(query.height).toBeGreaterThan(15);
    session.dispose();
  });
});

describe("adversarial save and collision contracts", () => {
  test("identity-affecting generator changes reject saves before mutating existing overrides", () => {
    const original = new WorldSession(world, terrain);
    original.persistence.overrides.set("kept", { removed: true });
    const save = original.save();
    const changed = new WorldSession(
      { ...world, populations: world.populations.map((rule) => ({ ...rule, spacing: rule.spacing + 1 })) },
      terrain,
    );
    changed.persistence.overrides.set("existing", { removed: true });
    expect(() => changed.loadSave(save)).toThrow("fingerprint");
    expect(changed.persistence.overrides.has("existing")).toBe(true);
    const malformed = { ...save, overrides: [["bad", { position: [NaN, 0, 0] }]] } as unknown as typeof save;
    expect(() => original.loadSave(malformed)).toThrow();
    expect(original.persistence.overrides.has("kept")).toBe(true);
    const duplicate = { ...save, overrides: [...save.overrides, ...save.overrides] };
    expect(() => original.loadSave(duplicate)).toThrow("duplicate");
    expect(original.persistence.overrides.size).toBe(1);
    original.dispose();
    changed.dispose();
  });
  test("ground query samples the resident collision triangle rather than claiming unsampled terrain exists", async () => {
    const sharp = {
      ...terrain,
      amplitude: 0,
      baseHeight: 0,
      interventions: [
        {
          id: "sharp",
          kind: "raise" as const,
          center: [1, 1] as [number, number],
          radius: 1,
          strength: 30,
          targetHeight: 0,
        },
      ],
    };
    const session = new WorldSession(world, sharp, { resolution: 8 });
    await session.teleport([1, 0, 1], { visualRadius: 40, collisionRadius: 20 });
    const query = session.queryGround(1, 1);
    expect(query.status).toBe("ready");
    if (query.status === "ready") {
      expect(query.analyticHeight).toBe(30);
      expect(query.height).toBe(0);
      expect(query.approximation).toBe("resident-triangle-heightfield");
      expect(query.sampleSpacing).toBe(4);
    }
    session.dispose();
  });
  test("empty interest sets publish cleanly and malformed planner budgets fail immediately", async () => {
    const session = new WorldSession(world, terrain);
    await session.prepare();
    expect(session.metrics.pending).toBe(false);
    expect(session.metrics.resident).toBe(0);
    expect(() =>
      planTerrain([{ id: "huge", position: [0, 0, 0], visualRadius: 8000, collisionRadius: 0 }], {
        baseSize: 4,
        levels: 0,
        maxRadius: 8192,
        maxPatches: 8,
      }),
    ).toThrow("budget");
    session.dispose();
  });
});

test("relocated procedural instances belong to their destination region with one stable identity", () => {
  const sourceRegion = { minX: -64, minZ: -64, maxX: 64, maxZ: 64 },
    destinationRegion = { minX: 960, minZ: 960, maxX: 1040, maxZ: 1040 };
  const original = generatePlacements(world, terrain, sourceRegion)[0];
  expect(original).toBeDefined();
  const overrides = new Map([
    [
      original.id,
      {
        position: [1000, 7, 1000] as [number, number, number],
        rotation: [0, 0.4, 0] as [number, number, number],
      },
    ],
  ]);
  const destination = generatePlacements(world, terrain, destinationRegion, overrides, 1);
  expect(destination.map((placement) => placement.id)).toEqual([original.id]);
  expect(destination[0].position).toEqual([1000, 7, 1000]);
  expect(destination[0].rotation).toBe(0.4);
  expect(
    generatePlacements(world, terrain, sourceRegion, overrides).some(
      (placement) => placement.id === original.id,
    ),
  ).toBe(false);
  const union = generatePlacements(
    world,
    terrain,
    { minX: -64, minZ: -64, maxX: 1040, maxZ: 1040 },
    overrides,
  );
  expect(union.filter((placement) => placement.id === original.id)).toHaveLength(1);
});
