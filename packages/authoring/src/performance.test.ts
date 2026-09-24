import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples";
import type { CharacterDefinition } from "@wrela/model";
import {
  duplicatePerformanceClip,
  movePerformanceKey,
  removePerformanceClip,
  retimePerformanceClip,
  trimPerformanceClip,
  upsertMotionKey,
  validatePerformanceEdit,
} from "./performance";

const character: Pick<CharacterDefinition, "joints" | "motions" | "performance"> = {
  joints: [
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
  ],
  motions: [
    {
      id: "walk",
      name: "Walk",
      duration: 2,
      loop: true,
      keys: [{ joint: "root", time: 1, rotation: [0, 0, 0], translation: [1, 0, 0] }],
    },
  ],
  performance: {
    transitions: [],
    clips: [
      {
        motion: "walk",
        facialKeys: [{ joint: "root", time: 1, rotation: [0, 0, 0], translation: [0, 0, 0] }],
        events: [{ id: "step", time: 1, payload: {} }],
        contacts: [{ joint: "root", start: 0.5, end: 1, groundHeight: 0, tolerance: 0.01 }],
        alignments: [
          { id: "reach", joint: "root", time: 1, target: [0, 0, 0], blendIn: 0.2, blendOut: 0.2, weight: 1 },
        ],
      },
    ],
  },
};
test("retiming keeps body, face, contact and event tracks synchronized immutably", () => {
  const before = JSON.stringify(character);
  const result = retimePerformanceClip(character, "walk", 4);
  expect(result.motions[0].keys[0].time).toBe(2);
  expect(result.performance.clips[0].events[0].time).toBe(2);
  expect(result.performance.clips[0].facialKeys[0].time).toBe(2);
  expect(result.performance.clips[0].contacts[0].start).toBe(1);
  expect(result.performance.clips[0].alignments[0].blendIn).toBe(0.4);
  expect(JSON.stringify(character)).toBe(before);
});
test("keys replace an existing joint/time and reject keys outside the clip", () => {
  const motion = character.motions[0];
  const result = upsertMotionKey(motion, { ...motion.keys[0], translation: [2, 0, 0] });
  expect(result.keys).toHaveLength(1);
  expect(result.keys[0].translation[0]).toBe(2);
  expect(() => upsertMotionKey(motion, { ...motion.keys[0], time: 3 })).toThrow();
});
test("authoring rejects dangling references and malformed contact windows", () => {
  const performance = structuredClone(character.performance);
  if (!performance) throw new Error("Missing fixture performance");
  performance.clips[0].contacts[0].end = 0.1;
  performance.clips[0].facialKeys[0].joint = "missing";
  expect(() => validatePerformanceEdit(character, performance)).toThrow("Missing facial joint");
});
test("removing a clip clears transitions and creature contact references in one transaction", () => {
  const source = character as CharacterDefinition;
  if (!source.performance) throw new Error("Missing fixture performance");
  const result = removePerformanceClip(
    {
      ...source,
      performance: {
        ...source.performance,
        transitions: [{ from: "walk", to: "walk", duration: 0.2 }],
      },
    },
    "walk",
  );
  expect(result.motions).toEqual([]);
  expect(result.performance.clips).toEqual([]);
  expect(result.performance.transitions).toEqual([]);
  expect(character.motions).toHaveLength(1);
});

test("retiming also preserves physical contacts and creature review timing", () => {
  const source = fullCharacter();
  const before = JSON.stringify(source);
  const result = retimePerformanceClip(source, "walk", 4);
  expect(result.creature?.contacts[0]).toMatchObject({ start: 0.5, end: 3, blendIn: 0.2, blendOut: 0.4 });
  expect(result.creature?.reviewScenarios[0].duration).toBe(4);
  expect(JSON.stringify(source)).toBe(before);
});

test("duplicate gives physical contacts and review scenarios unique identities", () => {
  const source = fullCharacter();
  const result = duplicatePerformanceClip(source, "walk", "walk-copy");
  expect(result.motions.at(-1)?.id).toBe("walk-copy");
  expect(result.performance.clips.at(-1)?.events[0]).toEqual(source.performance?.clips[0].events[0]);
  expect(result.creature?.contacts.at(-1)?.motion).toBe("walk-copy");
  expect(result.creature?.reviewScenarios.at(-1)?.motion).toBe("walk-copy");
  expect(new Set(result.creature?.contacts.map((contact) => contact.id)).size).toBe(2);
  const copy = result.motions.at(-1);
  if (!copy) throw new Error("Missing duplicated clip");
  copy.keys[0].translation[0] = 20;
  expect(source.motions[0].keys[0].translation[0]).toBe(1);
  expect(() => duplicatePerformanceClip(source, "walk", "walk")).toThrow("unique");
});

test("trim keeps sampled boundary poses and synchronizes semantic and physical timelines", () => {
  const source = fullCharacter();
  if (!source.performance) throw new Error("Missing fixture performance");
  source.performance.locomotion = {
    enabled: true,
    speed: 1,
    samples: [
      { motion: "walk", speed: 0 },
      { motion: "run", speed: 1 },
    ],
  };
  const body = {
    start: [{ ...source.motions[0].keys[0], translation: [0.5, 0, 0] as [number, number, number] }],
    end: [{ ...source.motions[0].keys[0], translation: [1.5, 0, 0] as [number, number, number] }],
  };
  const face = {
    start: [...source.performance.clips[0].facialKeys],
    end: [...source.performance.clips[0].facialKeys],
  };
  const result = trimPerformanceClip(source, "walk", 0.5, 1.5, body, face);
  expect(result.motions[0]).toMatchObject({ duration: 1, loop: false });
  expect(result.motions[0].keys.map((key) => [key.time, key.translation[0]])).toEqual([
    [0, 0.5],
    [0.5, 1],
    [1, 1.5],
  ]);
  expect(result.performance.clips[0].events[0].time).toBe(0.5);
  expect(result.performance.clips[0].contacts[0]).toMatchObject({ start: 0, end: 0.5 });
  expect(result.creature?.contacts[0]).toMatchObject({ start: 0, end: 1 });
  expect(result.creature?.reviewScenarios[0].duration).toBe(1);
  expect(result.performance.locomotion).toBeUndefined();
  expect(() => trimPerformanceClip(source, "walk", 1, 1.01, body, face)).toThrow("0.1 seconds");
  expect(() => trimPerformanceClip(source, "walk", 0, 1, { start: [], end: [] }, face)).toThrow(
    "boundary key",
  );
});

test("timeline key moves never silently overwrite another authored key", () => {
  const keys = [character.motions[0].keys[0], { ...character.motions[0].keys[0], time: 2 }];
  expect(movePerformanceKey(keys, "root", 1, 0.5, 2).map((key) => key.time)).toEqual([0.5, 2]);
  expect(keys[0].time).toBe(1);
  expect(() => movePerformanceKey(keys, "root", 1, 2, 2)).toThrow("already occupies");
  expect(() => movePerformanceKey(keys, "root", 0, 1, 2)).toThrow("existing key");
});

function fullCharacter(): CharacterDefinition {
  const fixture = createCreatureFixture("ash-warden").project.documents.find(
    (entry) => entry.kind === "character",
  );
  if (!fixture || fixture.kind !== "character" || !fixture.creature)
    throw new Error("Missing creature fixture");
  return {
    ...fixture,
    ...structuredClone(character),
    creature: {
      ...fixture.creature,
      contacts: [
        {
          id: "plant",
          motion: "walk",
          joint: "root",
          start: 0.25,
          end: 1.5,
          target: [0, 0, 0],
          space: "character",
          weight: 1,
          tolerance: 0.01,
          blendIn: 0.1,
          blendOut: 0.2,
          ground: true,
          offset: 0,
        },
      ],
      reviewScenarios: [
        { ...fixture.creature.reviewScenarios[0], id: "review-walk", motion: "walk", duration: 2 },
      ],
    },
  };
}
