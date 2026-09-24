import { describe, expect, test } from "bun:test";
import type { CharacterPerformance, Joint, Motion } from "@wrela/model";

import { crossedMotionEvents } from "./motion-events";
import {
  inspectPerformanceContacts,
  jointPosePosition,
  performanceBoundaryKeys,
  performanceMotionTrail,
  resolvePerformanceBlend,
  reviewPerformanceContacts,
  sampleCharacterPerformance,
} from "./performance";

const joints: Joint[] = [
  {
    id: "root",
    name: "Root",
    parent: null,
    position: [0, 0, 0],
    rotation: [0, 0, 0],
    radius: 0.1,
    minimum: -Math.PI,
    maximum: Math.PI,
  },
  {
    id: "jaw",
    name: "Jaw",
    parent: "root",
    position: [0, 1, 0],
    rotation: [0, 0, 0],
    radius: 0.1,
    minimum: -Math.PI,
    maximum: Math.PI,
  },
];
const motion = (id: string, distance: number, duration = 1): Motion => ({
  id,
  name: id,
  duration,
  loop: true,
  keys: [
    { joint: "root", time: 0, rotation: [0, 0, 0], translation: [0, 0, 0] },
    { joint: "root", time: duration, rotation: [0, 0, 0], translation: [distance, 0, 0] },
  ],
});
const performance = (): CharacterPerformance => ({
  clips: [
    {
      motion: "walk",
      events: [{ id: "foot", time: 0.25, payload: { foot: "left" } }],
      facialKeys: [],
      contacts: [],
      alignments: [],
    },
  ],
  transitions: [{ from: "walk", to: "run", duration: 0.4 }],
});
describe("authored performance", () => {
  test("phase-synchronizes locomotion with unequal clip lengths and accumulated roots", () => {
    const source = performance();
    source.locomotion = {
      enabled: true,
      speed: 1,
      samples: [
        { motion: "walk", speed: 0 },
        { motion: "run", speed: 2 },
      ],
    };
    const motions = [motion("walk", 2), motion("run", 6, 2)];
    expect(
      sampleCharacterPerformance(joints, motions, source, "walk", 0.5).get("root")?.translation[0],
    ).toBeCloseTo(2);
    expect(
      sampleCharacterPerformance(joints, motions, source, "walk", 1.5, true).get("root")?.translation[0],
    ).toBeCloseTo(6);
    expect(resolvePerformanceBlend(source, "walk", "run", 0.2)).toBe(0.4);
    expect(resolvePerformanceBlend(source, "run", "walk", 0.2)).toBe(0.2);
  });
  test("additive face keys retain the body pose and do not mutate inputs", () => {
    const source = performance();
    source.clips[0].facialKeys.push({ joint: "jaw", time: 0, rotation: [0, 0, 0], translation: [0, 0.2, 0] });
    const before = JSON.stringify(source);
    const pose = sampleCharacterPerformance(joints, [motion("walk", 2)], source, "walk", 0.5);
    expect(jointPosePosition(joints, pose, "jaw")?.[0]).toBeCloseTo(1);
    expect(jointPosePosition(joints, pose, "jaw")?.[1]).toBeCloseTo(1.2);
    expect(JSON.stringify(source)).toBe(before);
  });
  test("interaction root correction reaches an authored contact target and releases smoothly", () => {
    const source = performance();
    source.clips[0].alignments.push({
      id: "reach",
      joint: "jaw",
      time: 0.5,
      target: [3, 2, 0],
      blendIn: 0.2,
      blendOut: 0.2,
      weight: 1,
    });
    const motions = [motion("walk", 2)];
    expect(
      jointPosePosition(joints, sampleCharacterPerformance(joints, motions, source, "walk", 0.5), "jaw"),
    ).toEqual([3, 2, 0]);
    expect(
      sampleCharacterPerformance(joints, motions, source, "walk", 0.8).get("root")?.translation[0],
    ).toBeCloseTo(1.6);
  });
  test("contact diagnostics distinguish a stable foot from sliding and hovering", () => {
    const source = performance();
    source.clips[0].contacts.push({ joint: "root", start: 0, end: 1, groundHeight: 0, tolerance: 0.02 });
    expect(inspectPerformanceContacts(joints, [motion("walk", 0)], source, "walk", 0.5)[0].valid).toBe(true);
    expect(
      inspectPerformanceContacts(joints, [motion("walk", 2)], source, "walk", 0.5)[0].slideSpeed,
    ).toBeCloseTo(2);
    source.clips[0].contacts[0].groundHeight = -0.2;
    expect(inspectPerformanceContacts(joints, [motion("walk", 0)], source, "walk", 0.5)[0].valid).toBe(false);
  });
  test("motion trail includes authored loop endpoint and semantic events cross once", () => {
    const source = performance(),
      motions = [motion("walk", 2)];
    const trail = performanceMotionTrail(joints, motions, source, "walk", "root", 3);
    expect(trail.map((point) => point.position[0])).toEqual([0, 1, 2]);
    expect(crossedMotionEvents(motions[0], source.clips[0].events, 0.2, 0.25)).toHaveLength(1);
    expect(crossedMotionEvents(motions[0], source.clips[0].events, 0.25, 0.3)).toHaveLength(0);
  });
});

test("loop root accumulation is continuous at exact boundaries, including frame zero", () => {
  const motions = [motion("walk", 2)];
  expect(
    sampleCharacterPerformance(joints, motions, undefined, "walk", 0, true).get("root")?.translation[0],
  ).toBe(0);
  expect(
    sampleCharacterPerformance(joints, motions, undefined, "walk", 1, true).get("root")?.translation[0],
  ).toBe(2);
  expect(
    sampleCharacterPerformance(joints, motions, undefined, "walk", 2, true).get("root")?.translation[0],
  ).toBe(4);
});

test("contact diagnostics measure actual velocity at frame zero and across a root-motion loop seam", () => {
  const source = performance();
  source.clips[0].contacts.push({ joint: "root", start: 0, end: 1, groundHeight: 0, tolerance: 0.02 });
  const clips = [motion("walk", 2)];
  expect(inspectPerformanceContacts(joints, clips, source, "walk", 0)[0].slideSpeed).toBeCloseTo(2);
  expect(inspectPerformanceContacts(joints, clips, source, "walk", 1)[0].slideSpeed).toBeCloseTo(2);
  expect(inspectPerformanceContacts(joints, clips, source, "walk", 0.001)[0].slideSpeed).toBeCloseTo(2);
});

test("contact review covers short windows and the authored endpoint without wrapping to the start", () => {
  const source = performance();
  source.clips[0].contacts.push({ joint: "root", start: 0.995, end: 1, groundHeight: 0, tolerance: 0.02 });
  const clips = [motion("walk", 2)];
  const result = reviewPerformanceContacts(joints, clips, source, "walk");
  expect(result).toHaveLength(1);
  expect(result[0]).toMatchObject({ samples: 2, valid: false });
  expect(result[0].maximumSlideSpeed).toBeCloseTo(2);
});

test("trim boundary keys round-trip the same quaternion pose, including compound rotations", () => {
  const clip: Motion = {
    ...motion("walk", 2),
    loop: false,
    keys: [
      { joint: "root", time: 0, translation: [0, 0, 0], rotation: [0.3, -0.7, 0.4] },
      { joint: "root", time: 1, translation: [2, 0, 0], rotation: [-0.4, 0.6, 1.1] },
    ],
  };
  const boundary = performanceBoundaryKeys(joints, clip, 0.35);
  const sampled = sampleCharacterPerformance(joints, [clip], undefined, "walk", 0.35).get("root");
  const roundTrip = sampleCharacterPerformance(
    joints,
    [{ ...clip, keys: boundary }],
    undefined,
    "walk",
    0.35,
  ).get("root");
  if (!sampled || !roundTrip) throw new Error("Missing root pose");
  expect(roundTrip.translation).toEqual(sampled.translation);
  const dot = roundTrip.rotation.reduce((sum, value, index) => sum + value * sampled.rotation[index], 0);
  expect(Math.abs(dot)).toBeCloseTo(1, 10);
});

test("blended locomotion trails retain displacement at the end of unequal-duration loops", () => {
  const source = performance();
  source.locomotion = {
    enabled: true,
    speed: 1,
    samples: [
      { motion: "walk", speed: 0 },
      { motion: "run", speed: 2 },
    ],
  };
  const trail = performanceMotionTrail(
    joints,
    [motion("walk", 2), motion("run", 6, 2)],
    source,
    "walk",
    "root",
    3,
  );
  expect(trail.map((entry) => entry.position[0])).toEqual([0, 2, 4]);
});
