import { expect, test } from "bun:test";
import type { Vec3 } from "./math";
import { sampleWorldPath, worldPathPolyline, worldPathSampleCount } from "./world-path";
import { worldRoomModules } from "./world-room";

test("rounded routes stay within their control envelope and share module/count geometry", () => {
  const path = {
    points: [
      [0, 0, 0],
      [10, 2, 0],
      [10, 4, 10],
    ] as Vec3[],
    cornerRadius: 4,
  };
  const points = worldPathPolyline(path);
  expect(points[0]).toEqual(path.points[0]);
  expect(points[points.length - 1]).toEqual(path.points[2]);
  expect(points.some((point) => point[0] === 9 && point[2] === 1)).toBe(true);
  expect(
    points.every(
      (point) =>
        point[0] >= 0 && point[0] <= 10 && point[1] >= 0 && point[1] <= 4 && point[2] >= 0 && point[2] <= 10,
    ),
  ).toBe(true);
  const samples = sampleWorldPath(path, 2);
  expect(samples).toHaveLength(worldPathSampleCount(path, 2));
  expect(samples[0].yaw).toBeCloseTo(Math.PI / 2);
  expect(samples.at(-1)?.yaw).toBeCloseTo(0);
  expect(() => sampleWorldPath(path, 0.001)).toThrow("budget");
  expect(sampleWorldPath({ ...path, cornerRadius: 0 }, 5)).toHaveLength(5);
});

test("wall module fitting preserves precise doorway clearance and outer dimensions", () => {
  const room = {
    size: [9, 7] as [number, number],
    moduleWidth: 2,
    doorWidth: 2.5,
    doorSide: "north" as const,
  };
  const modules = worldRoomModules(room);
  const north = modules.filter((module) => module.side === 0);
  expect(Math.min(...north.map((module) => module.position[0] - 1))).toBeCloseTo(-4.5);
  expect(Math.max(...north.map((module) => module.position[0] + 1))).toBeCloseTo(4.5);
  expect(
    Math.max(...north.filter((module) => module.position[0] < 0).map((module) => module.position[0] + 1)),
  ).toBeCloseTo(-1.25);
  expect(
    Math.min(...north.filter((module) => module.position[0] > 0).map((module) => module.position[0] - 1)),
  ).toBeCloseTo(1.25);
  expect(() => worldRoomModules({ ...room, moduleWidth: 4 })).toThrow("doorway");
});
