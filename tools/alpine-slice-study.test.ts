import { expect, test } from "bun:test";
import type { FrameMeasurements, GpuFrameTiming } from "@wrela/model";

import {
  assessAlpineBudget,
  createAlpineSliceStudy,
  matchAlpineGpuTimings,
  summarizeAlpineBudgets,
} from "./alpine-slice-study";

test("held-out alpine layout is produced by normal source operations", () => {
  const primary = createAlpineSliceStudy("primary", "portable");
  const heldout = createAlpineSliceStudy("river-bend", "portable");
  expect(primary.operations).toHaveLength(0);
  expect(heldout.operations.length).toBeGreaterThan(8);
  expect(heldout.project.entry).toBe(primary.project.entry);
  expect(heldout.project.documents.length).toBe(primary.project.documents.length);
  expect(heldout.cameras).not.toEqual(primary.cameras);
});
test("budget evidence requires matching output and sufficient real GPU samples", () => {
  const frame: FrameMeasurements = {
    cpuMs: 2,
    gpuMs: 10,
    triangles: 1000,
    drawCalls: 10,
    gpuBytes: 64 * 1024 ** 2,
    frame: 0,
    adapter: "test-adapter",
    outputResolution: [1920, 1080],
    renderResolution: [1280, 720],
  };
  const frames = Array.from({ length: 40 }, (_, index) => ({ ...frame, frame: index }));
  expect(assessAlpineBudget("balanced", frames, 32 * 1024 ** 2).met).toBe(true);
  expect(assessAlpineBudget("balanced", frames, 32 * 1024 ** 2).actualRenderResolutions).toEqual([
    "1280×720",
  ]);
  expect(
    assessAlpineBudget(
      "balanced",
      frames.map((value) => ({ ...value, gpuMs: null })),
      0,
    ).met,
  ).toBe(false);
  expect(assessAlpineBudget("balanced", frames.slice(0, 4), 0).met).toBe(false);
  expect(assessAlpineBudget("portable", frames, 0).met).toBe(false);
  expect(
    assessAlpineBudget(
      "balanced",
      frames.map((value, index) => ({ ...value, gpuMs: index < 36 ? 10 : 30 })),
      0,
    ).met,
  ).toBe(false);
});

test("asynchronous GPU timing joins only its own rendered frame and missing frames stay unknown", () => {
  const frame = { cpuMs: 2, gpuMs: 999, triangles: 1, drawCalls: 1, gpuBytes: 1, adapter: "test" };
  const measurements = [101, 102, 103].map((id) => ({ ...frame, frame: id }));
  const timing = (id: number, gpuMs: number): GpuFrameTiming => ({
    frame: id,
    gpuMs,
    shadowMs: 0,
    sceneMs: gpuMs,
    displayMs: 0,
  });
  const matched = matchAlpineGpuTimings(measurements, [timing(80, 77), timing(103, 3), timing(101, 1)]);
  expect(matched.map((entry) => entry.gpuMs)).toEqual([1, null, 3]);
  expect(measurements.map((entry) => entry.gpuMs)).toEqual([999, 999, 999]);
  expect(() => matchAlpineGpuTimings(measurements, [timing(101, 1), timing(101, 2)])).toThrow("Duplicate");
});

test("budget cannot pass with duplicate frames, malformed resolutions or invalid resource/timing values", () => {
  const frames: FrameMeasurements[] = Array.from({ length: 40 }, (_, frame) => ({
    frame,
    cpuMs: 2,
    gpuMs: 10,
    triangles: 1,
    drawCalls: 1,
    gpuBytes: 1,
    adapter: "test",
    outputResolution: [1920, 1080],
    renderResolution: [1280, 720],
  }));
  expect(
    assessAlpineBudget(
      "balanced",
      frames.map((entry) => ({ ...entry, frame: 1 })),
      0,
    ).met,
  ).toBe(false);
  expect(
    assessAlpineBudget(
      "balanced",
      frames.map((entry) => ({ ...entry, outputResolution: [] as unknown as [number, number] })),
      0,
    ).met,
  ).toBe(false);
  expect(
    assessAlpineBudget(
      "balanced",
      frames.map((entry) => ({ ...entry, gpuMs: -1 })),
      0,
    ).met,
  ).toBe(false);
  expect(
    assessAlpineBudget(
      "balanced",
      frames.map((entry) => ({ ...entry, cpuMs: Number.NaN })),
      0,
    ).met,
  ).toBe(false);
  expect(assessAlpineBudget("balanced", frames, -1).met).toBe(false);
});

test("all captured camera budgets retain explicit failures and roll up independently of capture completion", () => {
  const measurements: FrameMeasurements[] = Array.from({ length: 48 }, (_, frame) => ({
    frame,
    cpuMs: 2,
    gpuMs: 10,
    triangles: 1,
    drawCalls: 1,
    gpuBytes: 1,
    adapter: "test",
    outputResolution: [1920, 1080],
    renderResolution: [1280, 720],
  }));
  const passing = assessAlpineBudget("balanced", measurements, 1);
  const slow = assessAlpineBudget(
    "balanced",
    measurements.map((frame) => ({ ...frame, cpuMs: 8, gpuMs: 24 })),
    1,
  );
  const frames = [
    { id: "approach", budget: passing },
    { id: "shelter", budget: slow },
    { id: "overlook", budget: passing },
  ];
  expect(summarizeAlpineBudgets(frames, 3)).toEqual({
    budgetMet: false,
    budgetFailures: ["shelter: GPU p95 24.000 ms exceeds 12 ms", "shelter: CPU p95 8.000 ms exceeds 4 ms"],
  });
  expect(frames.map((frame) => frame.id)).toEqual(["approach", "shelter", "overlook"]);
  expect(
    summarizeAlpineBudgets(
      frames.map((frame) => ({ ...frame, budget: passing })),
      3,
    ),
  ).toEqual({
    budgetMet: true,
    budgetFailures: [],
  });
  expect(summarizeAlpineBudgets([], 3)).toEqual({
    budgetMet: false,
    budgetFailures: ["Measured 0/3 required camera budgets"],
  });
});
