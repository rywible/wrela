import {
  type CharacterDefinition,
  creatureAnchorSchema,
  creatureAttachmentSchema,
  creatureChartSchema,
  creatureClothSchema,
  creatureExpressionSchema,
  creaturePatchHasConsistentOrientation,
  creatureReviewScenarioSchema,
  type Vec3,
} from "@wrela/model";

import { requireCreature } from "./creature";
import type { CreatureOperation } from "./creature-commands";

/** These helpers produce ordinary transactional source operations, with no hidden
 * fitting state. The caller adopts them through AuthoringSession (and its undo). */
const difference = (a: Vec3, b: Vec3) => a.map((v, i) => v - b[i]) as Vec3;

function regionOf(character: CharacterDefinition, id: string) {
  const source = requireCreature(character).creature;
  const region = source.regions.find((value) => value.id === id);
  if (!region) throw Error(`Unknown anatomical region ${id}`);
  return region;
}

function unusedIds(character: CharacterDefinition, ids: string[]) {
  const source = requireCreature(character).creature;
  const existing = new Set(
    Object.values(source).flatMap((items) =>
      Array.isArray(items) ? items.map((item: { id: string }) => item.id) : [],
    ),
  );
  for (const id of ids) if (existing.has(id)) throw Error(`Creature identity ${id} already exists`);
}

/** Landmark distances in one anatomical frame remain invariant under its rotation. */
export function measureCreatureLandmarks(character: CharacterDefinition, first: string, second: string) {
  const source = requireCreature(character).creature;
  const a = source.landmarks.find((item) => item.id === first);
  const b = source.landmarks.find((item) => item.id === second);
  if (!a || !b) throw Error("Choose two existing landmarks");
  if (a.region !== b.region) throw Error("Fit landmarks must share an anatomical region");
  return { region: a.region, distance: Math.hypot(...difference(a.position, b.position)) };
}

/** A uniform fit deliberately preserves chart correspondence and avoids pretending
 * that a scalar measurement determines a unique anisotropic anatomical edit. */
export function fitCreatureLandmarkSpan(
  character: CharacterDefinition,
  input: { first: string; second: string; distance: number; descendants?: boolean; preserve?: string[] },
): CreatureOperation[] {
  const measured = measureCreatureLandmarks(character, input.first, input.second);
  if (!Number.isFinite(input.distance) || input.distance <= 0 || measured.distance < 1e-8)
    throw Error("Landmark fitting requires nonzero finite source and target distances");
  const factor = input.distance / measured.distance;
  if (factor < 0.1 || factor > 10) throw Error("Fit ratio must be between 0.1 and 10 per edit");
  return [
    {
      kind: "creature.proportion",
      target: character.id,
      region: measured.region,
      scale: [factor, factor, factor],
      propagate: input.descendants ? "descendants" : "region",
      preserve: input.preserve,
    },
  ];
}

/** Build a patch garment from explicit anatomical landmarks in (u0v0,u1v0,u0v1,u1v1)
 * order. Top corners are pinned to persistent region-local anchors. */
export function authorCreatureGarment(
  character: CharacterDefinition,
  input: {
    id: string;
    region: string;
    landmarks: [string, string, string, string];
    material?: string;
    thickness?: number;
  },
): CreatureOperation[] {
  const source = requireCreature(character).creature;
  regionOf(character, input.region);
  if (new Set(input.landmarks).size !== 4) throw Error("Garment needs four distinct corner landmarks");
  const points = input.landmarks.map((id) => {
    const landmark = source.landmarks.find((item) => item.id === id && item.region === input.region);
    if (!landmark) throw Error(`Garment landmark ${id} must belong to ${input.region}`);
    return [...landmark.position] as Vec3;
  }) as [Vec3, Vec3, Vec3, Vec3];
  // Reject collapsed and folded bilinear patches at every corner, before creating
  // anchors which would otherwise look valid while the generated surface tears.
  if (!creaturePatchHasConsistentOrientation(points))
    throw Error("Garment corners must form a nondegenerate, consistently ordered patch");
  const chartId = `${input.id}-surface`,
    leftId = `${input.id}-pin-left`,
    rightId = `${input.id}-pin-right`;
  unusedIds(character, [input.id, chartId, leftId, rightId]);
  const chart = creatureChartSchema.parse({
    id: chartId,
    kind: "patch",
    region: input.region,
    revision: 0,
    points,
    thickness: input.thickness ?? 0.006,
    material: input.material,
  });
  const anchors = [leftId, rightId].map((id, index) =>
    creatureAnchorSchema.parse({
      id,
      region: input.region,
      landmark: input.landmarks[index],
      coordinates: points[index],
      offset: 0,
      purpose: "attachment",
      tolerance: 0.01,
    }),
  );
  const cloth = creatureClothSchema.parse({
    id: input.id,
    region: input.region,
    chart: chartId,
    chartRevision: 0,
    fittingLandmarks: input.landmarks,
    material: input.material,
    simulationResolution: 6,
    pins: anchors.map((anchor, index) => ({ anchor: anchor.id, coordinates: [index, 0], weight: 1 })),
  });
  return [
    { kind: "creature.chart", target: character.id, value: chart },
    ...anchors.map((value): CreatureOperation => ({ kind: "creature.anchor", target: character.id, value })),
    { kind: "creature.cloth", target: character.id, value: cloth },
  ];
}

/** Freeze the current fit as independently editable source. No geometry or pin
 * position moves when authors deliberately release a garment's measurements. */
export function releaseCreatureGarmentFit(character: CharacterDefinition, id: string): CreatureOperation[] {
  const source = requireCreature(character).creature;
  const cloth = source.cloth.find((item) => item.id === id);
  if (!cloth) throw Error(`Unknown garment ${id}`);
  const { fittingLandmarks, ...independent } = cloth;
  if (!fittingLandmarks) return [];
  const pinned = new Set(cloth.pins.map((pin) => pin.anchor));
  const anchors = source.anchors.filter(
    (anchor) => pinned.has(anchor.id) && anchor.landmark && fittingLandmarks.includes(anchor.landmark),
  );
  return [
    { kind: "creature.cloth", target: character.id, value: independent },
    ...anchors.map((anchor): CreatureOperation => {
      const { landmark: _landmark, ...independentAnchor } = anchor;
      return { kind: "creature.anchor", target: character.id, value: independentAnchor };
    }),
  ];
}

/** Seed editable garment landmarks around the selected region's back plane. This
 * is a fitting starting point; authors can move all four before creating cloth. */
export function authorCreatureGarmentLandmarks(
  character: CharacterDefinition,
  regionId: string,
  id: string,
): CreatureOperation[] {
  const region = regionOf(character, regionId);
  const [x, y, z] = region.extent;
  const points: Vec3[] = [
    [-x, y * 0.7, -z - 0.03],
    [x, y * 0.7, -z - 0.03],
    [-x, -y, -z - 0.08],
    [x, -y, -z - 0.08],
  ];
  const ids = ["top-left", "top-right", "bottom-left", "bottom-right"].map((suffix) => `${id}-${suffix}`);
  unusedIds(character, ids);
  return ids.map((landmark, index) => ({
    kind: "creature.landmark",
    target: character.id,
    value: { id: landmark, region: regionId, position: points[index] },
  }));
}

/** Captures a reusable expression from explicit joint offsets. It composes with
 * motion through the existing expression deformation path. */
export function authorCreatureExpression(
  character: CharacterDefinition,
  input: { id: string; joint: string; rotation: Vec3; translation: Vec3; weight?: number },
): CreatureOperation {
  const source = requireCreature(character).creature;
  if (!character.joints.some((joint) => joint.id === input.joint))
    throw Error(`Unknown joint ${input.joint}`);
  const existing = source.expressions.find((expression) => expression.id === input.id);
  const weights = structuredClone(existing?.weights ?? []);
  const value = { joint: input.joint, rotation: input.rotation, translation: input.translation };
  const index = weights.findIndex((item) => item.joint === input.joint);
  if (index < 0) weights.push(value);
  else weights[index] = value;
  return {
    kind: "creature.expression",
    target: character.id,
    value: creatureExpressionSchema.parse({
      id: input.id,
      weights,
      weight: input.weight ?? existing?.weight ?? 1,
    }),
  };
}

export function mountCreatureAttachment(
  character: CharacterDefinition,
  input: {
    id: string;
    landmark: string;
    node: string;
    joint?: string;
    clearance?: number;
  },
): CreatureOperation[] {
  const source = requireCreature(character).creature;
  const landmark = source.landmarks.find((item) => item.id === input.landmark);
  const node = character.field.nodes.find((item) => item.id === input.node);
  if (!landmark || !node || node.children.length)
    throw Error("Mounting requires an existing landmark and leaf shape");
  if (source.attachments.some((attachment) => attachment.nodeIds.includes(node.id)))
    throw Error(`Shape ${node.id} already belongs to an attachment`);
  if (input.joint && !character.joints.some((joint) => joint.id === input.joint))
    throw Error("Unknown attachment joint");
  const anchorId = `${input.id}-mount`;
  unusedIds(character, [input.id, anchorId]);
  const anchor = creatureAnchorSchema.parse({
    id: anchorId,
    region: landmark.region,
    landmark: landmark.id,
    coordinates: landmark.position,
    offset: 0,
    purpose: "attachment",
    tolerance: 0.01,
  });
  return [
    { kind: "creature.anchor", target: character.id, value: anchor },
    {
      kind: "creature.attachment",
      target: character.id,
      value: creatureAttachmentSchema.parse({
        id: input.id,
        anchor: anchor.id,
        nodeIds: [node.id],
        rigidJoint: input.joint,
        placement: "surface",
        minimumClearance: input.clearance ?? 0,
      }),
    },
  ];
}

/** Deterministic front/side/back silhouettes plus a region closeup, saved as
 * existing review source so captures and movement audits can reuse the setup. */
export function authorCreatureReview(
  character: CharacterDefinition,
  input: { id: string; region: string; motion?: string },
): CreatureOperation {
  const region = regionOf(character, input.region);
  const motion = input.motion ? character.motions.find((item) => item.id === input.motion) : undefined;
  if (input.motion && !motion) throw Error(`Unknown motion ${input.motion}`);
  const { min, max } = character.field.bounds;
  const target = min.map((value, i) => (value + max[i]) / 2) as Vec3;
  const radius = Math.max(0.2, Math.hypot(...difference(max, min)) / 2);
  const distance = (radius / Math.sin((24 * Math.PI) / 180)) * 1.1;
  const closeDistance = (Math.max(0.15, Math.hypot(...region.extent)) / Math.sin((24 * Math.PI) / 180)) * 1.1;
  const at = (offset: Vec3): Vec3 => target.map((v, i) => v + offset[i]) as Vec3;
  return {
    kind: "creature.review",
    target: character.id,
    value: creatureReviewScenarioSchema.parse({
      id: input.id,
      name: `${region.name} shape and performance`,
      motion: motion?.id,
      duration: motion?.duration ?? 1,
      sampleRate: 30,
      cameras: [
        { id: "front", target, position: at([0, 0, distance]) },
        { id: "side", target, position: at([distance, 0, 0]) },
        { id: "back", target, position: at([0, 0, -distance]) },
        {
          id: "closeup",
          target: region.frame.position,
          position: [
            region.frame.position[0],
            region.frame.position[1],
            region.frame.position[2] + closeDistance,
          ],
        },
      ],
      thresholds: { contactSlip: 0.02, penetration: 0.015, anchorError: 0.02, stretch: 1.1 },
    }),
  };
}
