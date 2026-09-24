import { expect, test } from "bun:test";
import type { StudioController } from "./controller";
import { DomainMotionPreview } from "./domain-motion-preview";

function fixture() {
  const calls: (string | number | boolean)[][] = [];
  const state = { selection: "actor" };
  const studio = {
    seek: async (time: number) => {
      calls.push(["seek", time]);
    },
    host: {
      playMotion: (target: string, motion: string, blend?: number) => {
        calls.push(["motion", target, motion, blend ?? "authored"]);
      },
    },
    authoring: { getSnapshot: () => state },
    patch: (value: { playing: boolean }) => {
      calls.push(["playing", value.playing]);
    },
  };
  return {
    preview: new DomainMotionPreview(
      studio as unknown as Pick<StudioController, "seek" | "host" | "authoring" | "patch">,
    ),
    calls,
    state,
  };
}

test("transition audition replays the source before applying the authored destination blend", async () => {
  const { preview, calls } = fixture();
  await preview.transition("actor", "walk", "idle", 0.5, 0.2, true);
  expect(calls).toEqual([
    ["seek", 0],
    ["motion", "actor", "walk", 0],
    ["seek", 0.5 + 1 / 60],
    ["motion", "actor", "idle", "authored"],
    ["seek", 0.5 + 0.2 + 2 / 60],
    ["playing", true],
  ]);
});

test("new scrubbing cancels queued transition playback and changing selection leaves it untouched", async () => {
  const { preview, calls, state } = fixture();
  const obsolete = preview.transition("actor", "walk", "idle", 0.5, 0.2, true);
  const latest = preview.seek("actor", "walk", 0.75);
  await Promise.all([obsolete, latest]);
  expect(calls.filter((entry) => entry[0] === "motion")).toEqual([["motion", "actor", "walk", 0]]);
  calls.length = 0;
  state.selection = "other";
  await preview.transition("actor", "walk", "idle", 0.5, 0.2);
  expect(calls).toEqual([]);
});

test("invalid transition intervals cannot mutate preview state", async () => {
  const { preview, calls } = fixture();
  await expect(preview.transition("actor", "walk", "idle", 119, 2)).rejects.toThrow("120 seconds");
  await expect(preview.transition("actor", "walk", "walk", 0, 0)).rejects.toThrow("different");
  await expect(preview.transition("actor", "walk", "idle", 0, Number.NaN)).rejects.toThrow();
  expect(calls).toEqual([]);
});
