import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema, performanceSchema, type Vec3 } from "@wrela/model";
import {
  authorCreatureExpression,
  authorCreatureGarment,
  authorCreatureGarmentLandmarks,
  authorCreatureReview,
  fitCreatureLandmarkSpan,
  measureCreatureLandmarks,
  mountCreatureAttachment,
  releaseCreatureGarmentFit,
} from "./creature-coherence";
import { inspectCreatureFields } from "./creature-provenance";
import { AuthoringSession } from "./session";

function fixture() {
  const project = referenceProject();
  const character = project.documents.find(
    (document) => document.kind === "character",
  ) as CharacterDefinition;
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "body-region",
        name: "Body",
        nodeIds: character.field.nodes.filter((node) => !node.children.length).map((node) => node.id),
        jointIds: character.joints.map((joint) => joint.id),
        frame: { position: [0, 1, 0], rotation: [0, 0, 0] },
        extent: [0.5, 1, 0.3],
      },
    ],
    landmarks: [
      { id: "left", region: "body-region", position: [-0.5, 0, 0] },
      { id: "right", region: "body-region", position: [0.5, 0, 0] },
    ],
    articulation: {
      bodies: [
        { joint: character.joints[0].id, mass: 10, radius: 0.1, halfHeight: 0.25, offset: [0, 0.1, 0] },
      ],
      joints: [],
    },
    pelvis: { joint: character.joints[0].id, maxOffset: [0.1, 0.2, 0.1] },
  });
  const session = new AuthoringSession(project);
  const read = () =>
    session.inspect(character.id) as CharacterDefinition & {
      creature: NonNullable<CharacterDefinition["creature"]>;
    };
  return { session, read, character };
}

function fittedGarment() {
  const context = fixture();
  context.session.apply({
    expectedRevision: 0,
    operations: authorCreatureGarmentLandmarks(context.character, "body-region", "cape"),
  });
  context.session.apply({
    expectedRevision: 1,
    operations: authorCreatureGarment(context.read(), {
      id: "cape",
      region: "body-region",
      landmarks: ["cape-top-left", "cape-top-right", "cape-bottom-left", "cape-bottom-right"],
    }),
  });
  return context;
}

test("editing garment fitting landmarks refits the patch and its pins and survives save/undo", () => {
  const { session, read, character } = fittedGarment();
  const before = session.export();
  const landmark = read().creature.landmarks.find((item) => item.id === "cape-top-left");
  if (!landmark) throw Error("Missing fitting landmark");
  const position: Vec3 = [-0.6, 0.75, -0.4];
  session.apply({
    expectedRevision: 2,
    operations: [{ kind: "creature.landmark", target: character.id, value: { ...landmark, position } }],
  });
  const fitted = read().creature;
  expect(fitted.charts.find((item) => item.id === "cape-surface")?.points[0]).toEqual(position);
  expect(fitted.anchors.find((item) => item.id === "cape-pin-left")?.coordinates).toEqual(position);
  expect(fitted.anchors.find((item) => item.id === "cape-pin-left")?.landmark).toBe(landmark.id);
  expect(fitted.cloth[0].chartRevision).toBe(0);
  expect(fitted.cloth[0].fittingLandmarks?.[0]).toBe(landmark.id);
  const controls = inspectCreatureFields(read()).controls;
  expect(
    controls
      .find((control) => control.id === "cape")
      ?.dependencies.some((dependency) => dependency.id === landmark.id),
  ).toBe(true);
  expect(
    controls
      .find((control) => control.id === "cape-pin-left")
      ?.dependencies.some((dependency) => dependency.id === landmark.id),
  ).toBe(true);
  const saved = session.export();
  expect(new AuthoringSession(JSON.parse(saved)).export()).toBe(saved);
  session.undo();
  expect(session.export()).toBe(before);
  session.redo();
  expect(session.export()).toBe(saved);
});

test("garment fits reject folded edits, dangling bindings and bypassed propagation atomically", () => {
  const { session, read, character } = fittedGarment();
  const before = session.export();
  const landmark = read().creature.landmarks.find((item) => item.id === "cape-top-left");
  const opposite = read().creature.landmarks.find((item) => item.id === "cape-bottom-right");
  if (!landmark || !opposite) throw Error("Missing fitting landmarks");
  expect(() =>
    session.apply({
      expectedRevision: 2,
      operations: [
        {
          kind: "creature.landmark",
          target: character.id,
          value: { ...landmark, position: opposite.position },
        },
      ],
    }),
  ).toThrow("fold, collapse or reverse");
  expect(session.export()).toBe(before);
  expect(() =>
    session.apply({
      expectedRevision: 2,
      operations: [{ kind: "creature.remove", target: character.id, domain: "landmarks", id: landmark.id }],
    }),
  ).toThrow("Missing bound landmark");
  expect(session.export()).toBe(before);
  expect(() =>
    session.apply({
      expectedRevision: 2,
      operations: [
        {
          kind: "document.set",
          target: character.id,
          path: [
            "creature",
            "landmarks",
            read().creature.landmarks.findIndex((item) => item.id === landmark.id),
            "position",
          ],
          value: [-0.6, 0.75, -0.4],
        },
      ],
    }),
  ).toThrow("stale");
  expect(session.export()).toBe(before);
});

test("releasing fitting landmarks retains geometry and makes subsequent fitting edits independent", () => {
  const { session, read, character } = fittedGarment();
  const patch = structuredClone(read().creature.charts[0]);
  const anchor = structuredClone(read().creature.anchors[0]);
  session.apply({ expectedRevision: 2, operations: releaseCreatureGarmentFit(read(), "cape") });
  expect(read().creature.cloth[0].fittingLandmarks).toBeUndefined();
  expect(read().creature.anchors[0].landmark).toBeUndefined();
  session.apply({
    expectedRevision: 3,
    operations: [
      {
        kind: "creature.landmark",
        target: character.id,
        value: { id: "cape-top-left", region: "body-region", position: [10, 10, 10] },
      },
    ],
  });
  expect(read().creature.charts[0]).toEqual(patch);
  expect(read().creature.anchors[0].coordinates).toEqual(anchor.coordinates);
});

test("mounted components retain a live landmark binding through edits and proportion changes", () => {
  const { session, read, character } = fixture();
  const node = character.field.nodes.find((item) => !item.children.length);
  if (!node) throw Error("Missing mount shape");
  session.apply({
    expectedRevision: 0,
    operations: mountCreatureAttachment(character, { id: "plate", landmark: "left", node: node.id }),
  });
  const landmark = read().creature.landmarks.find((item) => item.id === "left");
  if (!landmark) throw Error("Missing mount landmark");
  session.apply({
    expectedRevision: 1,
    operations: [
      { kind: "creature.landmark", target: character.id, value: { ...landmark, position: [-0.75, 0.1, 0] } },
    ],
  });
  expect(read().creature.anchors[0].coordinates).toEqual([-0.75, 0.1, 0]);
  session.apply({
    expectedRevision: 2,
    operations: [
      { kind: "creature.proportion", target: character.id, region: "body-region", scale: [2, 2, 2] },
    ],
  });
  expect(read().creature.anchors[0].coordinates).toEqual([-1.5, 0.2, 0]);
  expect(read().creature.anchors[0].landmark).toBe("left");
  expect(read().field.nodes.find((item) => item.id === node.id)?.size).toEqual(node.size);
});

test("measured proportion fit carries landmarks and physical dimensions in one reversible edit", () => {
  const { session, read, character } = fixture();
  const before = session.export();
  session.apply({
    expectedRevision: 0,
    operations: fitCreatureLandmarkSpan(character, { first: "left", second: "right", distance: 2 }),
  });
  const fitted = read();
  expect(measureCreatureLandmarks(fitted, "left", "right").distance).toBe(2);
  expect(fitted.creature.articulation?.bodies[0]).toMatchObject({
    radius: 0.2,
    halfHeight: 0.5,
    offset: [0, 0.2, 0],
    mass: 10,
  });
  expect(fitted.creature.pelvis?.maxOffset).toEqual([0.2, 0.4, 0.2]);
  session.undo();
  expect(session.export()).toBe(before);
});

test("anisotropic articulated body edits fail atomically instead of silently retaining a wrong collider", () => {
  const { session, character } = fixture();
  const before = session.export();
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [
        { kind: "creature.proportion", target: character.id, region: "body-region", scale: [2, 1, 1] },
      ],
    }),
  ).toThrow("collision-body dimensions");
  expect(session.export()).toBe(before);
});

test("garment fitting persists top pins and normalized correspondence through anatomical growth", () => {
  const { session, read, character } = fixture();
  session.apply({
    expectedRevision: 0,
    operations: authorCreatureGarmentLandmarks(character, "body-region", "cape"),
  });
  const garment = authorCreatureGarment(read(), {
    id: "cape",
    region: "body-region",
    landmarks: ["cape-top-left", "cape-top-right", "cape-bottom-left", "cape-bottom-right"],
  });
  session.apply({ expectedRevision: 1, operations: garment });
  const original = read();
  const before = session.export();
  session.apply({
    expectedRevision: 2,
    operations: fitCreatureLandmarkSpan(original, { first: "left", second: "right", distance: 1.5 }),
  });
  const fitted = read();
  expect(fitted.creature.cloth[0].pins).toEqual(original.creature.cloth[0].pins);
  expect(fitted.creature.cloth[0].collisionRadius).toBeCloseTo(0.015);
  expect(fitted.creature.charts[0].revision).toBe(0);
  expect(fitted.creature.charts[0].points[0]).toEqual(
    original.creature.charts[0].points[0].map((value) => value * 1.5) as Vec3,
  );
  expect(fitted.creature.anchors[0].coordinates).toEqual(fitted.creature.charts[0].points[0]);
  session.undo();
  expect(session.export()).toBe(before);
});

test("garment fitting rejects collapsed, crossed and duplicate corners without mutating source", () => {
  const { session, read, character } = fixture();
  session.apply({
    expectedRevision: 0,
    operations: authorCreatureGarmentLandmarks(character, "body-region", "cape"),
  });
  const before = session.export();
  expect(() =>
    authorCreatureGarment(read(), {
      id: "cape",
      region: "body-region",
      landmarks: ["cape-top-left", "cape-bottom-right", "cape-bottom-left", "cape-top-right"],
    }),
  ).toThrow("consistently ordered");
  expect(() =>
    authorCreatureGarment(read(), {
      id: "cape",
      region: "body-region",
      landmarks: ["left", "left", "right", "right"],
    }),
  ).toThrow("distinct");
  expect(session.export()).toBe(before);
});

test("mounting follows a landmark with explicit rigid binding, preserves the attached shape's dimensions", () => {
  const { session, read, character } = fixture();
  const node = character.field.nodes.find((item) => !item.children.length);
  if (!node) throw Error("fixture node");
  session.apply({
    expectedRevision: 0,
    operations: mountCreatureAttachment(character, {
      id: "plate",
      node: node.id,
      landmark: "left",
      joint: character.joints[0].id,
      clearance: 0.01,
    }),
  });
  const original = read();
  session.apply({
    expectedRevision: 1,
    operations: fitCreatureLandmarkSpan(original, { first: "left", second: "right", distance: 2 }),
  });
  const fitted = read();
  expect(fitted.creature.attachments[0].minimumClearance).toBe(0.02);
  expect(fitted.creature.anchors[0].coordinates).toEqual([-1, 0, 0]);
  expect(fitted.field.nodes.find((item) => item.id === node.id)?.size).toEqual(node.size);
  expect(() =>
    mountCreatureAttachment(fitted, { id: "second-plate", node: node.id, landmark: "left" }),
  ).toThrow("already belongs");
});

test("expression editing keeps other joints and scales translation with the rig", () => {
  const { session, read, character } = fixture();
  const first = character.joints[0].id,
    second = character.joints[1].id;
  session.apply({
    expectedRevision: 0,
    operations: [
      authorCreatureExpression(character, {
        id: "snarl",
        joint: first,
        rotation: [0.1, 0, 0],
        translation: [0, 0.1, 0],
        weight: 0.7,
      }),
    ],
  });
  session.apply({
    expectedRevision: 1,
    operations: [
      authorCreatureExpression(read(), {
        id: "snarl",
        joint: second,
        rotation: [0, 0.2, 0],
        translation: [0.1, 0, 0],
      }),
    ],
  });
  session.apply({
    expectedRevision: 2,
    operations: fitCreatureLandmarkSpan(read(), { first: "left", second: "right", distance: 2 }),
  });
  const expression = read().creature.expressions[0];
  expect(expression.weight).toBe(0.7);
  expect(expression.weights).toHaveLength(2);
  expect(expression.weights[0].rotation).toEqual([0.1, 0, 0]);
  expect(expression.weights[0].translation).toEqual([0, 0.2, 0]);
});

test("review authoring stores four reusable cameras with real motion duration", () => {
  const { session, read, character } = fixture();
  const motion = character.motions[0];
  session.apply({
    expectedRevision: 0,
    operations: [authorCreatureReview(character, { id: "review", region: "body-region", motion: motion.id })],
  });
  const review = read().creature.reviewScenarios[0];
  expect(review.cameras.map((camera) => camera.id)).toEqual(["front", "side", "back", "closeup"]);
  expect(review.duration).toBe(motion.duration);
  expect(review.cameras.every((camera) => camera.position.every(Number.isFinite))).toBe(true);
  expect(() => fitCreatureLandmarkSpan(character, { first: "left", second: "left", distance: 1 })).toThrow(
    "nonzero",
  );
});

test("anatomy fitting carries facial performance, interaction alignment and model-space ground checks", () => {
  const { session, read, character } = fixture();
  const joint = character.joints[0].id,
    motion = character.motions[0];
  session.apply({
    expectedRevision: 0,
    operations: [
      {
        kind: "document.set",
        target: character.id,
        path: ["performance"],
        value: performanceSchema.parse({
          clips: [
            {
              motion: motion.id,
              facialKeys: [{ joint, time: 0, translation: [0.1, 0.2, 0], rotation: [0.1, 0, 0] }],
              contacts: [{ joint, start: 0, end: motion.duration, groundHeight: 0, tolerance: 0.01 }],
              alignments: [
                { id: "touch", joint, time: 0, target: [1, 2, 3], blendIn: 0.1, blendOut: 0.1, weight: 1 },
              ],
            },
          ],
        }),
      },
    ],
  });
  session.apply({
    expectedRevision: 1,
    operations: fitCreatureLandmarkSpan(read(), { first: "left", second: "right", distance: 2 }),
  });
  const clip = read().performance?.clips[0];
  expect(clip?.facialKeys[0].translation).toEqual([0.2, 0.4, 0]);
  expect(clip?.facialKeys[0].rotation).toEqual([0.1, 0, 0]);
  expect(clip?.alignments[0].target).toEqual([2, 3, 6]);
  expect(clip?.contacts[0].groundHeight).toBe(-1);
  expect(clip?.contacts[0].tolerance).toBe(0.02);
});
