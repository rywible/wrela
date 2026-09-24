import { describe, expect, test } from "bun:test";
import { generateTerrainPatch } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import type { TerrainDefinition, WorldDefinition } from "@wrela/model";
import { planTerrain } from "./planner";
import { generatePlacements, PersistentWorldState } from "./population";
import { BoundedScheduler } from "./scheduler";
import { WorldSession } from "./session";

const project = referenceProject();
const terrain = project.documents.find((document) => document.kind === "terrain") as TerrainDefinition;
const world = project.documents.find((document) => document.kind === "world") as WorldDefinition;
const flat = { ...terrain, amplitude: 0, baseHeight: 0, interventions: [] };

describe("critical streaming", () => {
  test("a delayed distant visual product does not block prepared collision or sustained travel", async () => {
    let release!: () => void;
    const visual = new Promise<void>((resolve) => {
      release = resolve;
    });
    const session = new WorldSession(world, flat, {
      baseSize: 32,
      levels: 0,
      maxPatches: 4,
      resolution: 8,
      maxCacheBytes: 100000,
      maxLiveBytes: 120000,
      concurrency: 1,
      generate: async (source, patch, resolution) => {
        if (!patch.collision) await visual;
        return generateTerrainPatch(source, patch.x, patch.z, patch.size, resolution, patch.stitch);
      },
    });
    session.setInterest({
      id: "remote-camera",
      position: [10000, 0, 16],
      visualRadius: 1,
      collisionRadius: 0,
    });
    for (let step = 0; step < 40; step++) {
      const x = step * 32 + 16;
      session.setInterest({ id: "actor", position: [x, 0, 16], visualRadius: 0, collisionRadius: 1 });
      const prepared = await session.prepare({ scope: "collision", timeoutMs: 1000 });
      expect(prepared).toEqual({ ready: true, missing: [] });
      expect(session.queryGround(x, 16).status).toBe("ready");
      expect(session.metrics.collisionPending).toBe(false);
      expect(session.metrics.pending).toBe(true);
      expect(session.metrics.resident).toBeLessThanOrEqual(2);
      expect(session.metrics.liveBytes).toBeLessThanOrEqual(120000);
      expect(session.metrics.sourceRevision).toBe(1);
    }
    expect((await session.prepare({ timeoutMs: 1 })).ready).toBe(false);
    release();
    expect((await session.prepare()).ready).toBe(true);
    expect(session.metrics.pending).toBe(false);
    expect(session.metrics.collisionRevision).toBeGreaterThan(1);
    session.dispose();
  });

  test("metadata and collision-role changes reuse geometry, while shape changes invalidate it", async () => {
    let generated = 0;
    const session = new WorldSession(world, flat, {
      baseSize: 32,
      levels: 0,
      maxPatches: 4,
      resolution: 8,
      generate: async (source, patch, resolution) => {
        generated++;
        return generateTerrainPatch(source, patch.x, patch.z, patch.size, resolution, patch.stitch);
      },
    });
    session.setInterest({ id: "camera", position: [16, 0, 16], visualRadius: 1, collisionRadius: 0 });
    await session.prepare();
    const mesh = session.residentPatches()[0].mesh;
    session.setInterest({ id: "camera", position: [16, 0, 16], visualRadius: 1, collisionRadius: 1 });
    await session.prepare({ scope: "collision" });
    session.replaceTerrain({ ...flat, name: "Renamed terrain", material: "other-material" });
    await session.prepare();
    expect(generated).toBe(1);
    expect(session.collisionPatches()[0].mesh).toBe(mesh);
    expect(session.metrics.sourceRevision).toBe(2);
    session.replaceTerrain({ ...flat, baseHeight: 8 });
    await session.prepare();
    expect(generated).toBe(2);
    const query = session.queryGround(16, 16);
    expect(query.status === "ready" && query.height).toBe(8);
    session.dispose();
  });

  test("live-memory admission accounts for installed products even with an empty cache", async () => {
    const session = new WorldSession(world, flat, {
      baseSize: 32,
      levels: 0,
      maxPatches: 4,
      resolution: 8,
      maxCacheBytes: 0,
      maxLiveBytes: 4000,
    });
    session.setInterest({ id: "actor", position: [16, 0, 16], visualRadius: 1, collisionRadius: 1 });
    expect((await session.prepare()).ready).toBe(true);
    expect(session.metrics.cacheBytes).toBe(0);
    expect(session.metrics.liveBytes).toBe(3480);
    session.setInterest({ id: "camera", position: [144, 0, 16], visualRadius: 1, collisionRadius: 0 });
    expect((await session.prepare()).ready).toBe(false);
    expect(session.metrics.failures.join(" ")).toContain("live-memory budget");
    expect(session.queryGround(16, 16).status).toBe("ready");
    expect(session.metrics.liveBytes).toBeLessThanOrEqual(4000);
    session.dispose();
  });

  test("LOD hysteresis retains coverage and shared-edge balance through small camera reversals", () => {
    let previous = new Set<string>();
    let changes = 0;
    let last = "";
    for (let step = 0; step < 30; step++) {
      const plan = planTerrain(
        [{ id: "camera", position: [17 + (step % 2) * 0.01, 0, 17], visualRadius: 200, collisionRadius: 10 }],
        {},
        previous,
      );
      const signature = plan
        .map((patch) => patch.id)
        .sort()
        .join("|");
      if (last && last !== signature) changes++;
      last = signature;
      previous = new Set(plan.map((patch) => patch.id));
      expect(plan.some((patch) => patch.collision && patch.x <= 17 && patch.x + patch.size >= 17)).toBe(true);
    }
    expect(changes).toBeLessThanOrEqual(1);
  });
});

describe("generation failure recovery", () => {
  test("one unresponsive job times out without blocking urgent replacement work", async () => {
    const scheduler = new BoundedScheduler<number>(1, 4, 15);
    let release!: (value: number) => void;
    const blocked = scheduler.request(
      "blocked",
      1,
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const rejected = expect(blocked).rejects.toThrow("timed out");
    const urgent = scheduler.request("urgent", 100, async () => 42);
    await rejected;
    expect(await urgent).toBe(42);
    expect(scheduler.metrics.quarantined).toBe(1);
    release(0);
    await Bun.sleep(0);
    expect(scheduler.metrics.quarantined).toBe(0);
    scheduler.dispose();
  });

  test("repeated unresponsive generators have a bounded quarantine and fail explicitly", async () => {
    const scheduler = new BoundedScheduler<number>(1, 8, 5);
    const jobs = ["a", "b", "c", "d"].map((key) =>
      scheduler.request(key, 1, async () => new Promise<number>(() => {})),
    );
    const results = await Promise.allSettled(jobs);
    expect(results.every((result) => result.status === "rejected")).toBe(true);
    expect(scheduler.metrics.quarantined).toBe(2);
    expect(scheduler.metrics.queued).toBe(0);
    scheduler.dispose();
  });
});

describe("population composition and persistence", () => {
  const dense: WorldDefinition = {
    ...world,
    populations: ["trees", "stones"].map((id, index) => ({
      ...world.populations[0],
      id,
      seed: 32 + index,
      spacing: 2,
      density: 1,
      minHeight: -100,
      maxHeight: 100,
      maxSlope: 1,
    })),
  };
  const region = { minX: -115, minZ: -115, maxX: 115, maxZ: 115 };
  test("saturated populations allocate every rule and every quadrant with stable traversal", () => {
    const placements = generatePlacements(dense, flat, region, new Map(), 200);
    expect(placements).toHaveLength(200);
    for (const id of ["trees", "stones"])
      expect(placements.filter((placement) => placement.id.includes(`/${id}/`))).toHaveLength(100);
    for (const x of [-1, 1])
      for (const z of [-1, 1])
        expect(
          placements.filter(
            (placement) => Math.sign(placement.position[0]) === x && Math.sign(placement.position[2]) === z,
          ).length,
        ).toBeGreaterThan(25);
    const reordered = generatePlacements(
      { ...dense, populations: [...dense.populations].reverse() },
      flat,
      region,
      new Map(),
      200,
    );
    expect(reordered).toEqual(placements);
    const shifted = generatePlacements(dense, flat, { ...region, minX: -114, maxX: 116 }, new Map(), 200);
    const old = new Set(placements.map((placement) => placement.id));
    expect(shifted.filter((placement) => old.has(placement.id)).length).toBeGreaterThan(185);
  });

  test("a retained and moved instance survives a clearing at its former site and save/reopen", () => {
    const original = generatePlacements(world, terrain, { minX: -64, minZ: -64, maxX: 64, maxZ: 64 })[0];
    const state = new PersistentWorldState(world.id, world.generatorVersion);
    state.retain({ ...original, position: [1000, 7, 1000] }, { touched: true });
    const reopened = new PersistentWorldState(world.id, world.generatorVersion);
    reopened.load(state.save());
    const cleared: TerrainDefinition = {
      ...terrain,
      interventions: [
        ...terrain.interventions,
        {
          id: "removed-origin",
          kind: "clearing",
          center: [original.position[0], original.position[2]],
          radius: 1000,
          strength: 1,
          targetHeight: 0,
        },
      ],
    };
    const placements = generatePlacements(
      world,
      cleared,
      { minX: 990, minZ: 990, maxX: 1010, maxZ: 1010 },
      reopened.overrides,
      1,
    );
    expect(placements).toEqual([{ ...original, position: [1000, 7, 1000] }]);
    expect(reopened.overrides.get(original.id)?.state?.touched).toBe(true);
  });
});

test("incremental LOD publication never overlaps coverage or exposes a mismatched shared edge", async () => {
  const resolution = 8;
  const session = new WorldSession(world, terrain, {
    maxPatches: 60,
    resolution,
    concurrency: 4,
    generate: async (source, patch, cells) => {
      await Bun.sleep(Math.abs(patch.x + patch.z) % 3);
      return generateTerrainPatch(source, patch.x, patch.z, patch.size, cells, patch.stitch);
    },
  });
  let snapshots = 0;
  for (const x of [17, 75, 137, -31]) {
    session.setInterests([{ id: "camera", position: [x, 0, 17], visualRadius: 140, collisionRadius: 20 }]);
    session.update();
    const ready = session.prepare();
    do {
      await Bun.sleep(0);
      const patches = session.residentPatches();
      snapshots++;
      for (let a = 0; a < patches.length; a++)
        for (let b = a + 1; b < patches.length; b++) {
          const left = patches[a],
            right = patches[b];
          const overlapX = Math.min(left.x + left.size, right.x + right.size) - Math.max(left.x, right.x);
          const overlapZ = Math.min(left.z + left.size, right.z + right.size) - Math.max(left.z, right.z);
          expect(overlapX > 0 && overlapZ > 0).toBe(false);
          const vertical = overlapX === 0 && overlapZ > 0;
          const horizontal = overlapZ === 0 && overlapX > 0;
          if (!vertical && !horizontal) continue;
          const start = vertical ? Math.max(left.z, right.z) : Math.max(left.x, right.x);
          const end = vertical
            ? Math.min(left.z + left.size, right.z + right.size)
            : Math.min(left.x + left.size, right.x + right.size);
          const fixed = vertical ? Math.max(left.x, right.x) : Math.max(left.z, right.z);
          const edgeHeight = (patch: typeof left, value: number) => {
            const step = patch.size / resolution;
            const along = (value - (vertical ? patch.z : patch.x)) / step;
            const index = Math.min(resolution - 1, Math.floor(along)),
              fraction = along - index;
            const edge = Math.round((fixed - (vertical ? patch.x : patch.z)) / step);
            const first = vertical ? index * (resolution + 1) + edge : edge * (resolution + 1) + index;
            const second = first + (vertical ? resolution + 1 : 1);
            return (
              patch.mesh.positions[first * 3 + 1] * (1 - fraction) +
              patch.mesh.positions[second * 3 + 1] * fraction
            );
          };
          for (let value = start; value <= end; value += Math.min(left.size, right.size) / resolution) {
            const difference = Math.abs(edgeHeight(left, value) - edgeHeight(right, value));
            if (difference >= 0.0001)
              throw new Error(
                JSON.stringify({
                  x,
                  snapshots,
                  difference,
                  value,
                  left: { id: left.id, stitch: left.stitch, revision: left.revision },
                  right: { id: right.id, stitch: right.stitch, revision: right.revision },
                }),
              );
          }
        }
    } while (session.metrics.pending && snapshots < 500);
    expect((await ready).ready).toBe(true);
  }
  expect(snapshots).toBeGreaterThan(4);
  session.dispose();
}, 10000);

test("urgent collision work preempts optional generation without losing its eventual result", async () => {
  const scheduler = new BoundedScheduler<number>(1, 4, 1000);
  let release!: (value: number) => void;
  let attempts = 0;
  const optional = scheduler.request("visual", 1, async () => {
    if (++attempts > 1) return 99;
    return new Promise<number>((resolve) => {
      release = resolve;
    });
  });
  await Bun.sleep(0);
  for (let index = 0; index < 20; index++)
    expect(await scheduler.request(`collision-${index}`, 100, async () => index, { urgent: true })).toBe(
      index,
    );
  expect(scheduler.metrics.quarantined).toBe(1);
  expect(attempts).toBe(1);
  release(0);
  expect(await optional).toBe(99);
  expect(scheduler.metrics.quarantined).toBe(0);
  scheduler.dispose();
});
