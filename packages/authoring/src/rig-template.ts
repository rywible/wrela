import type { Bounds, Joint, Vec3 } from "@wrela/model";

export type RigTemplateKind = "biped" | "quadruped" | "chain";
type TemplateJoint = {
  id: string;
  name: string;
  parent: string | null;
  at: Vec3;
  radius: number;
  minimum?: number;
  maximum?: number;
};

/** Model-space rest anchors. Templates suggest envelopes and limits; no source is mutated. */
export function rigTemplate(kind: RigTemplateKind, bounds: Bounds): Joint[] {
  const size = bounds.max.map((value, axis) => value - bounds.min[axis]) as Vec3;
  if (
    [...bounds.min, ...bounds.max].some((value) => !Number.isFinite(value)) ||
    size.some((value) => value <= 0 || value > 200)
  )
    throw new RangeError("Rig template bounds must have positive extents no greater than 200 metres");
  let definitions: TemplateJoint[];
  if (kind === "chain") {
    definitions = Array.from({ length: 6 }, (_, index) => ({
      id: index === 0 ? "root" : `segment-${index}`,
      name: index === 0 ? "Root" : `Segment ${index}`,
      parent: index === 0 ? null : index === 1 ? "root" : `segment-${index - 1}`,
      at: [0.5, 0.08 + index * 0.168, 0.5],
      radius: 0.2,
      minimum: index === 0 ? -0.5 : -1.2,
      maximum: index === 0 ? 0.5 : 1.2,
    }));
  } else if (kind === "biped") {
    definitions = [
      {
        id: "root",
        name: "Hips",
        parent: null,
        at: [0.5, 0.48, 0.5],
        radius: 0.24,
        minimum: -0.6,
        maximum: 0.6,
      },
      {
        id: "spine",
        name: "Spine",
        parent: "root",
        at: [0.5, 0.62, 0.5],
        radius: 0.22,
        minimum: -0.5,
        maximum: 0.5,
      },
      {
        id: "chest",
        name: "Chest",
        parent: "spine",
        at: [0.5, 0.74, 0.5],
        radius: 0.22,
        minimum: -0.5,
        maximum: 0.5,
      },
      {
        id: "neck",
        name: "Neck",
        parent: "chest",
        at: [0.5, 0.83, 0.5],
        radius: 0.13,
        minimum: -0.6,
        maximum: 0.6,
      },
      {
        id: "head",
        name: "Head",
        parent: "neck",
        at: [0.5, 0.92, 0.5],
        radius: 0.18,
        minimum: -0.9,
        maximum: 0.9,
      },
    ];
    for (const [side, sign] of [
      ["left", -1],
      ["right", 1],
    ] as const) {
      const label = side === "left" ? "Left" : "Right";
      definitions.push(
        {
          id: `${side}-shoulder`,
          name: `${label} shoulder`,
          parent: "chest",
          at: [0.5 + sign * 0.2, 0.74, 0.5],
          radius: 0.15,
          minimum: -1.8,
          maximum: 1.8,
        },
        {
          id: `${side}-elbow`,
          name: `${label} elbow`,
          parent: `${side}-shoulder`,
          at: [0.5 + sign * 0.32, 0.63, 0.5],
          radius: 0.12,
          minimum: -0.15,
          maximum: 2.4,
        },
        {
          id: `${side}-wrist`,
          name: `${label} wrist`,
          parent: `${side}-elbow`,
          at: [0.5 + sign * 0.42, 0.52, 0.5],
          radius: 0.1,
          minimum: -0.8,
          maximum: 0.8,
        },
        {
          id: `${side}-hip`,
          name: `${label} hip`,
          parent: "root",
          at: [0.5 + sign * 0.12, 0.45, 0.5],
          radius: 0.17,
          minimum: -1.5,
          maximum: 1.5,
        },
        {
          id: `${side}-knee`,
          name: `${label} knee`,
          parent: `${side}-hip`,
          at: [0.5 + sign * 0.13, 0.25, 0.5],
          radius: 0.14,
          minimum: -2.4,
          maximum: 0.15,
        },
        {
          id: `${side}-ankle`,
          name: `${label} ankle`,
          parent: `${side}-knee`,
          at: [0.5 + sign * 0.13, 0.08, 0.5],
          radius: 0.1,
          minimum: -0.65,
          maximum: 0.65,
        },
        {
          id: `${side}-foot`,
          name: `${label} foot`,
          parent: `${side}-ankle`,
          at: [0.5 + sign * 0.13, 0.06, 0.72],
          radius: 0.12,
          minimum: -0.4,
          maximum: 0.4,
        },
      );
    }
  } else if (kind === "quadruped") {
    definitions = [
      {
        id: "root",
        name: "Hips",
        parent: null,
        at: [0.5, 0.58, 0.3],
        radius: 0.26,
        minimum: -0.6,
        maximum: 0.6,
      },
      {
        id: "spine",
        name: "Spine",
        parent: "root",
        at: [0.5, 0.61, 0.5],
        radius: 0.25,
        minimum: -0.5,
        maximum: 0.5,
      },
      {
        id: "chest",
        name: "Chest",
        parent: "spine",
        at: [0.5, 0.64, 0.68],
        radius: 0.23,
        minimum: -0.5,
        maximum: 0.5,
      },
      {
        id: "neck",
        name: "Neck",
        parent: "chest",
        at: [0.5, 0.77, 0.79],
        radius: 0.16,
        minimum: -0.8,
        maximum: 0.8,
      },
      {
        id: "head",
        name: "Head",
        parent: "neck",
        at: [0.5, 0.85, 0.9],
        radius: 0.2,
        minimum: -0.9,
        maximum: 0.9,
      },
      {
        id: "tail",
        name: "Tail",
        parent: "root",
        at: [0.5, 0.61, 0.08],
        radius: 0.13,
        minimum: -1.2,
        maximum: 1.2,
      },
    ];
    for (const [end, z, parent] of [
      ["front", 0.68, "chest"],
      ["hind", 0.3, "root"],
    ] as const)
      for (const [side, sign] of [
        ["left", -1],
        ["right", 1],
      ] as const) {
        const id = `${end}-${side}`,
          label = `${end === "front" ? "Front" : "Hind"} ${side}`;
        definitions.push(
          {
            id: `${id}-upper`,
            name: `${label} upper leg`,
            parent,
            at: [0.5 + sign * 0.25, 0.55, z],
            radius: 0.17,
            minimum: -1.4,
            maximum: 1.4,
          },
          {
            id: `${id}-knee`,
            name: `${label} knee`,
            parent: `${id}-upper`,
            at: [0.5 + sign * 0.28, 0.3, z - 0.04],
            radius: 0.13,
            minimum: -2.1,
            maximum: 0.2,
          },
          {
            id: `${id}-paw`,
            name: `${label} paw`,
            parent: `${id}-knee`,
            at: [0.5 + sign * 0.28, 0.07, z + 0.04],
            radius: 0.13,
            minimum: -0.7,
            maximum: 0.7,
          },
        );
      }
  } else throw new RangeError(`Unknown rig template: ${kind}`);
  const envelopeScale = Math.max(...size);
  return definitions.map((joint) => ({
    id: joint.id,
    name: joint.name,
    parent: joint.parent,
    position: joint.at.map((value, axis) => bounds.min[axis] + value * size[axis]) as Vec3,
    rotation: [0, 0, 0],
    radius: Math.max(0.01, Math.min(10, joint.radius * envelopeScale)),
    minimum: joint.minimum ?? -0.8,
    maximum: joint.maximum ?? 0.8,
  }));
}
