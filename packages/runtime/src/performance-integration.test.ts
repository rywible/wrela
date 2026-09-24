import { expect, test } from "bun:test";
import { compileCharacter, deserializeArtifact, serializeArtifact } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import type { CharacterDefinition, Joint, Motion } from "@wrela/model";
import { poseMatrices } from "./animation";
import { sampleCharacterPerformance } from "./performance";
import { RuntimeSession } from "./session";

const joints: Joint[] = [
  {
    id: "root",
    name: "Root",
    parent: null,
    position: [0, 0, 0],
    rotation: [0, 0, 0],
    radius: 0.2,
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
const clip = (id: string, x: number): Motion => ({
  id,
  name: id,
  duration: 1,
  loop: true,
  keys: [
    { joint: "root", time: 0, translation: [0, 0, 0], rotation: [0, 0, 0] },
    { joint: "root", time: 1, translation: [x, 0, 0], rotation: [0, 0, 0] },
  ],
});
function fixture(): CharacterDefinition {
  const source = referenceProject().documents.find((document) => document.kind === "character");
  if (!source || source.kind !== "character") throw new Error("Missing fixture character");
  return {
    ...source,
    creature: undefined,
    joints,
    motions: [clip("walk", 2), clip("run", 6)],
    physics: { ...source.physics, mode: "kinematic" },
    performance: {
      clips: [
        {
          motion: "walk",
          events: [{ id: "step", time: 0.25, payload: { foot: "left" } }],
          contacts: [],
          alignments: [],
          facialKeys: [{ joint: "jaw", time: 0, translation: [0, 0.2, 0], rotation: [0, 0, 0] }],
        },
      ],
      transitions: [{ from: "walk", to: "run", duration: 0.4 }],
      locomotion: {
        enabled: true,
        speed: 1,
        samples: [
          { motion: "walk", speed: 0 },
          { motion: "run", speed: 2 },
        ],
      },
    },
  };
}
test("compiled and cooked performance drives RuntimeSession poses, root motion, events and transitions", async () => {
  const definition = fixture();
  const compiled = compileCharacter(definition, "interactive");
  const restored = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(compiled))));
  if (restored.kind !== "character") throw new Error("Expected restored character");
  expect(restored.performance).toEqual(definition.performance);
  const runtime = await RuntimeSession.create();
  try {
    runtime.addCharacter("actor", restored, definition);
    for (let tick = 0; tick < 30; tick++) runtime.advance(1 / 60);
    const frame = runtime.evaluatedCharacters()[0];
    expect(frame.skinMatrices[16 + 13]).toBeCloseTo(0.2);
    // Blended locomotion moves 4 metres/cycle, not the selected walk clip's 2.
    expect(runtime.checkpoint().instances[0].position[0]).toBeCloseTo(2, 3);
    expect(runtime.drainAnimationEvents().events.map((event) => [event.eventId, event.payload.foot])).toEqual(
      [["step", "left"]],
    );
    runtime.playMotion("actor", "run");
    runtime.advance(1 / 60);
    expect(runtime.checkpoint().instances[0].blendDuration).toBe(0.4);
  } finally {
    runtime.dispose();
  }
});
test("authored interaction alignment affects the real runtime physical root", async () => {
  const definition = fixture();
  if (!definition.performance) throw new Error("Missing performance");
  definition.performance.locomotion = undefined;
  definition.performance.clips[0].alignments.push({
    id: "reach",
    joint: "jaw",
    time: 0.5,
    target: [3, 1.2, 0],
    blendIn: 0.2,
    blendOut: 0.2,
    weight: 1,
  });
  const runtime = await RuntimeSession.create();
  try {
    runtime.addCharacter("actor", compileCharacter(definition, "interactive"), definition);
    for (let tick = 0; tick < 30; tick++) runtime.advance(1 / 60);
    expect(runtime.checkpoint().instances[0].position[0]).toBeCloseTo(3, 3);
  } finally {
    runtime.dispose();
  }
});

test("controller locomotion inputs preserve root continuity and replay/save deterministically", async () => {
  const definition = fixture();
  const artifact = compileCharacter(definition, "interactive");
  const runtime = await RuntimeSession.create();
  const restored = await RuntimeSession.create();
  try {
    runtime.addCharacter("actor", artifact, definition);
    restored.addCharacter("actor", artifact, definition);
    for (let tick = 0; tick < 30; tick++) runtime.advance(1 / 60);
    expect(runtime.checkpoint().instances[0].position[0]).toBeCloseTo(2, 3);
    runtime.setLocomotionSpeed("actor", 2);
    runtime.advance(1 / 60);
    // The run pose moves 6 m/s. Changing speed must not resample all prior root travel.
    expect(runtime.checkpoint().instances[0].position[0]).toBeCloseTo(2.1, 3);
    for (let tick = 31; tick < 60; tick++) runtime.advance(1 / 60);
    const atSixty = runtime.snapshotEntities();
    expect(runtime.checkpoint().instances[0].position[0]).toBeCloseTo(5, 3);
    expect(atSixty[0].state.locomotionSpeed).toBe(2);
    runtime.seek(0);
    expect(runtime.checkpoint().instances[0].locomotionSpeed).toBeUndefined();
    runtime.seek(60);
    expect(runtime.snapshotEntities()).toEqual(atSixty);
    restored.restoreEntityStates(JSON.parse(JSON.stringify(atSixty)));
    runtime.advance(1 / 60);
    restored.advance(1 / 60);
    expect(restored.snapshotEntities()).toEqual(runtime.snapshotEntities());
    expect(() => restored.setLocomotionSpeed("actor", Number.NaN)).toThrow("Locomotion speed");
    expect(() => restored.setLocomotionSpeed("missing", 1)).toThrow("Unknown character");
    for (const speed of [-1, 51, Number.NaN, "fast"]) {
      const invalid = structuredClone(atSixty);
      invalid[0].state.locomotionSpeed = speed;
      expect(() => restored.restoreEntityStates(invalid)).toThrow();
    }
  } finally {
    runtime.dispose();
    restored.dispose();
  }
});

test("dormant entities retain controller locomotion speed and restore it on activation", async () => {
  const definition = fixture();
  const runtime = await RuntimeSession.create();
  try {
    runtime.addCharacter("actor", compileCharacter(definition, "interactive"), definition);
    const sleeping = runtime.snapshotEntities();
    sleeping[0].state.active = false;
    runtime.restoreEntityStates(sleeping);
    runtime.setLocomotionSpeed("actor", 2);
    runtime.advance(1 / 60);
    const dormant = runtime.snapshotEntities();
    expect(dormant[0].state.active).toBe(false);
    expect(dormant[0].state.locomotionSpeed).toBe(2);
    const awake = structuredClone(dormant);
    awake[0].state.active = true;
    runtime.restoreEntityStates(awake);
    runtime.advance(1 / 60);
    expect(runtime.snapshotEntities()[0].state.locomotionSpeed).toBe(2);
    expect(runtime.snapshotEntities()[0].state.active).toBe(true);
    expect(runtime.checkpoint().instances[0].position[0]).toBeCloseTo(0.2, 3);
  } finally {
    runtime.dispose();
  }
});

test("explicit playback blend overrides authored transitions so scrubbing samples the requested pose", async () => {
  const definition = fixture();
  if (!definition.performance) throw new Error("Missing fixture performance");
  definition.performance.locomotion = undefined;
  definition.motions[1].keys.push({ joint: "jaw", time: 0, rotation: [0.5, 0, 0], translation: [0, 0.4, 0] });
  const runtime = await RuntimeSession.create();
  try {
    runtime.addCharacter("actor", compileCharacter(definition, "interactive"), definition);
    runtime.playMotion("actor", "run", 0);
    runtime.advance(1 / 60);
    expect(runtime.checkpoint().instances[0].blendDuration).toBe(0);
    const expected = poseMatrices(
      definition.joints,
      sampleCharacterPerformance(definition.joints, definition.motions, definition.performance, "run", 0),
    );
    const actual = runtime.evaluatedCharacters()[0].skinMatrices;
    expect(actual.length).toBe(expected.length);
    for (let index = 0; index < expected.length; index++)
      expect(actual[index]).toBeCloseTo(expected[index], 6);
    runtime.playMotion("actor", "walk", 0);
    runtime.advance(1 / 60);
    runtime.playMotion("actor", "run", 0.15);
    runtime.advance(1 / 60);
    expect(runtime.checkpoint().instances[0].blendDuration).toBe(0.15);
    expect(() => runtime.playMotion("actor", "walk", Number.NaN)).toThrow();
    expect(() => runtime.playMotion("actor", "walk", -1)).toThrow();
  } finally {
    runtime.dispose();
  }
});
