import { describe, expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema, type Vec3 } from "@wrela/model";
import {
  authorCreatureMotion,
  type CreatureRetargetInput,
  retargetCreatureMotion,
} from "./creature-retarget";

function fixture() {
  const source = referenceProject().documents.find((d) => d.kind === "character") as CharacterDefinition;
  const root = source.joints[0].id;
  source.motions = [
    {
      id: "reach",
      name: "Reach",
      duration: 2,
      loop: false,
      keys: [
        { joint: root, time: 0, rotation: [0, 0, 0], translation: [0, 0, 0] },
        { joint: root, time: 1, rotation: [0.3, 0, 0], translation: [1, 0, 0] },
        { joint: root, time: 2, rotation: [0, 0, 0], translation: [0, 0, 0] },
      ],
    },
  ];
  source.creature = creatureSchema.parse({
    schemaVersion: 1,
    contacts: [
      {
        id: "plant",
        motion: "reach",
        joint: root,
        start: 0.25,
        end: 0.8,
        target: [1, 2, 3],
        space: "character",
        weight: 0.8,
        tolerance: 0.01,
      },
    ],
  });
  const target = structuredClone(source);
  target.id = "target";
  target.motions = [];
  target.creature = creatureSchema.parse({ schemaVersion: 1 });
  target.joints = target.joints.map((j) => ({
    ...j,
    id: `target-${j.id}`,
    parent: j.parent ? `target-${j.parent}` : null,
  }));
  const input: CreatureRetargetInput = {
    motion: "reach",
    targetMotion: { id: "target-reach", name: "Target reach" },
    translationScale: [2, 3, 4],
    mapping: [{ source: root, target: `target-${root}` }],
    characterContacts: { scale: [2, 2, 2], rotation: [0, 0, 0], translation: [1, 0, 0] },
  };
  return { source, target, input, root };
}

describe("explicit creature motion source", () => {
  test("retarget preserves timing and authored contacts without mutating either character", () => {
    const { source, target, input, root } = fixture(),
      before = JSON.stringify({ source, target, input });
    const result = retargetCreatureMotion(source, target, input);
    expect(result.motion.keys.map((k) => k.time)).toEqual([0, 1, 2]);
    expect(result.motion.keys[1].translation).toEqual([2, 0, 0]);
    expect(result.motion.keys[1].rotation).toEqual([0.3, 0, 0]);
    expect(result.motion.keys[1].joint).toBe(`target-${root}`);
    expect(result.contacts[0]).toMatchObject({
      id: "target-reach-plant",
      motion: "target-reach",
      target: [3, 4, 6],
      start: 0.25,
      end: 0.8,
      weight: 0.8,
    });
    expect(JSON.stringify({ source, target, input })).toBe(before);
    result.motion.keys[1].rotation[0] = 9;
    expect(source.motions[0].keys[1].rotation[0]).toBe(0.3);
  });
  test("basis mapping carries both rotation axis and local translation", () => {
    const { source, target, input } = fixture();
    input.mapping[0].basis = [0, 0, Math.PI / 2];
    const result = retargetCreatureMotion(source, target, input),
      key = result.motion.keys[1];
    expect(key.rotation[0]).toBeCloseTo(0, 8);
    expect(key.rotation[1]).toBeCloseTo(0.3, 8);
    expect(key.rotation[2]).toBeCloseTo(0, 8);
    expect(key.translation[0]).toBeCloseTo(0, 8);
    expect(key.translation[1]).toBeCloseTo(2, 8);
  });
  test("missing, duplicate, and many-to-one mappings are rejected", () => {
    const { source, target, input } = fixture();
    input.mapping[0].source = source.joints[1].id;
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/Missing explicit/);
    input.mapping[0].source = source.joints[0].id;
    input.mapping.push({ ...input.mapping[0] });
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/one-to-one/);
    input.mapping[1].source = source.joints[1].id;
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/one-to-one/);
  });
  test("unmapped contacts and ambiguous target hierarchy cannot silently degrade", () => {
    const { source, target, input } = fixture();
    if (!source.creature) throw new Error("fixture");
    source.creature.contacts[0].joint = source.joints[1].id;
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/Missing explicit/);
    input.mapping.push({ source: source.joints[1].id, target: target.joints[1].id });
    target.joints[1].parent = null;
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/hierarchy/);
  });
  test("world contact locations require an explicit disposition", () => {
    const { source, target, input } = fixture();
    if (!source.creature) throw new Error("fixture");
    source.creature.contacts[0].space = "world";
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/World contacts/);
    input.worldContacts = { kind: "preserve" };
    expect(retargetCreatureMotion(source, target, input).contacts[0].target).toEqual([1, 2, 3]);
    input.worldContacts = {
      kind: "transform",
      transform: { scale: [1, 1, 1], rotation: [0, 0, 0], translation: [5, 0, 0] },
    };
    expect(retargetCreatureMotion(source, target, input).contacts[0].target).toEqual([6, 2, 3]);
  });
  test("malformed source motion and overflowing transforms fail before returning a proposal", () => {
    const { source, target, input } = fixture();
    source.motions[0].keys.push(structuredClone(source.motions[0].keys[0]));
    expect(() => retargetCreatureMotion(source, target, input)).toThrow(/duplicate/);
    source.motions[0].keys.pop();
    if (!input.characterContacts) throw new Error("fixture");
    input.characterContacts.scale = [1e308, 1e308, 1e308];
    expect(() => retargetCreatureMotion(source, target, input)).toThrow();
  });
  test("reusable poses expand into ordinary editable motion keys with exact timing", () => {
    const { target } = fixture();
    const joint = target.joints[0].id;
    const motion = authorCreatureMotion(target, {
      id: "pose-clip",
      name: "Pose clip",
      duration: 2,
      loop: true,
      poses: [
        { id: "neutral", name: "Neutral", joints: [{ joint, rotation: [0, 0, 0], translation: [0, 0, 0] }] },
        { id: "lean", name: "Lean", joints: [{ joint, rotation: [0, 0, 0.4], translation: [0.1, 0, 0] }] },
      ],
      keys: [
        { time: 0, pose: "neutral" },
        { time: 0.7, pose: "lean" },
        { time: 2, pose: "neutral" },
      ],
    });
    expect(motion.keys.map((k) => k.time)).toEqual([0, 0.7, 2]);
    expect(motion.keys[1].rotation).toEqual([0, 0, 0.4]);
    expect(target.motions).toEqual([]);
  });
  test("pose import rejects hidden joints, duplicate time keys, and unused invalid poses", () => {
    const { target } = fixture(),
      joint = target.joints[0].id;
    const input = {
      id: "clip",
      name: "Clip",
      duration: 1,
      loop: false,
      poses: [
        {
          id: "p",
          name: "Pose",
          joints: [{ joint, rotation: [0, 0, 0] as Vec3, translation: [0, 0, 0] as Vec3 }],
        },
      ],
      keys: [{ time: 0, pose: "p" }],
    };
    input.keys.push({ time: 0, pose: "p" });
    expect(() => authorCreatureMotion(target, input)).toThrow(/duplicate/);
    input.keys.pop();
    input.poses.push({ ...structuredClone(input.poses[0]), id: "unused" });
    input.poses[1].joints[0].joint = "missing";
    expect(() => authorCreatureMotion(target, input)).toThrow(/missing joint/);
  });
});
