import {
  type CharacterDefinition,
  type CreatureContact,
  creatureContactSchema,
  idSchema,
  type Motion,
  motionSchema,
  type Vec3,
  vec3Schema,
} from "@wrela/model";

import { z } from "zod";

const nonzeroScale = z.tuple([
  z.number().finite().positive(),
  z.number().finite().positive(),
  z.number().finite().positive(),
]);
const transformSchema = z.strictObject({
  scale: nonzeroScale,
  rotation: vec3Schema,
  translation: vec3Schema,
});
export const creatureRetargetSchema = z.strictObject({
  motion: idSchema,
  targetMotion: z.strictObject({ id: idSchema, name: z.string().min(1).max(120) }),
  /** Authored displacement units, never inferred from bounding boxes. */
  translationScale: nonzeroScale,
  mapping: z
    .array(
      z.strictObject({
        source: idSchema,
        target: idSchema,
        /** Rotation carrying source local axes into target local axes, in XYZ Euler radians. */
        basis: vec3Schema.default([0, 0, 0]),
      }),
    )
    .min(1)
    .max(64),
  /** Contact points are in separate spaces from joint-local motion displacements. */
  characterContacts: transformSchema.optional(),
  worldContacts: z
    .union([
      z.strictObject({ kind: z.literal("preserve") }),
      z.strictObject({ kind: z.literal("transform"), transform: transformSchema }),
    ])
    .optional(),
});
export type CreatureRetargetInput = z.input<typeof creatureRetargetSchema>;
export type CreatureRetargetProposal = {
  motion: Motion;
  contacts: CreatureContact[];
  provenance: {
    sourceCharacter: string;
    sourceMotion: string;
    targetCharacter: string;
    mapping: z.infer<typeof creatureRetargetSchema>["mapping"];
    translationScale: Vec3;
  };
  diagnostics: { code: string; message: string }[];
};
type Matrix = [Vec3, Vec3, Vec3];
function multiply(a: Matrix, b: Matrix): Matrix {
  return a.map((row) => [0, 1, 2].map((j) => row.reduce((sum, v, i) => sum + v * b[i][j], 0))) as Matrix;
}
function transpose(a: Matrix): Matrix {
  return [0, 1, 2].map((i) => a.map((row) => row[i])) as Matrix;
}
function rotation([x, y, z]: Vec3): Matrix {
  const cx = Math.cos(x),
    sx = Math.sin(x),
    cy = Math.cos(y),
    sy = Math.sin(y),
    cz = Math.cos(z),
    sz = Math.sin(z);
  // Runtime motion's quaternion convention is Rx * Ry * Rz.
  return [
    [cy * cz, -cy * sz, sy],
    [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
    [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy],
  ];
}
function euler(m: Matrix): Vec3 {
  const y = Math.asin(Math.max(-1, Math.min(1, m[0][2])));
  return Math.abs(m[0][2]) < 0.9999999
    ? [Math.atan2(-m[1][2], m[2][2]), y, Math.atan2(-m[0][1], m[0][0])]
    : [Math.atan2(m[2][1], m[1][1]), y, 0];
}
function vector(m: Matrix, p: Vec3): Vec3 {
  return m.map((row) => row.reduce((sum, v, i) => sum + v * p[i], 0)) as Vec3;
}
function transform(p: Vec3, t: z.infer<typeof transformSchema>): Vec3 {
  const q = vector(rotation(t.rotation), p.map((v, i) => v * t.scale[i]) as Vec3);
  return q.map((v, i) => v + t.translation[i]) as Vec3;
}
function checkMotion(character: CharacterDefinition, motion: Motion) {
  const ids = new Set(character.joints.map((j) => j.id));
  if (ids.size !== character.joints.length) throw new Error("Skeleton joint identities are ambiguous");
  const seen = new Set<string>();
  for (const key of motion.keys) {
    if (!ids.has(key.joint)) throw new Error(`Motion references missing joint ${key.joint}`);
    if (key.time > motion.duration) throw new Error(`Motion key exceeds duration: ${key.joint}`);
    const id = `${key.joint}:${key.time}`;
    if (seen.has(id)) throw new Error(`Ambiguous duplicate motion key ${id}`);
    seen.add(id);
  }
}
/** Pure source-to-source retarget. Caller reviews and adopts ordinary editable data.
 * It does not claim that copied timing and poses produce a solved target gait. */
export function retargetCreatureMotion(
  source: CharacterDefinition,
  target: CharacterDefinition,
  input: CreatureRetargetInput,
): CreatureRetargetProposal {
  const request = creatureRetargetSchema.parse(input);
  const matches = source.motions.filter((m) => m.id === request.motion);
  if (matches.length !== 1) throw new Error(`Source motion ${request.motion} must resolve uniquely`);
  const original = motionSchema.parse(matches[0]);
  checkMotion(source, original);
  const sourceJoints = new Map(source.joints.map((j) => [j.id, j])),
    targetJoints = new Map(target.joints.map((j) => [j.id, j]));
  if (targetJoints.size !== target.joints.length) throw new Error("Target joint identities are ambiguous");
  const mapping = new Map<string, (typeof request.mapping)[number]>(),
    mappedTargets = new Set<string>();
  for (const entry of request.mapping) {
    if (!sourceJoints.has(entry.source) || !targetJoints.has(entry.target))
      throw new Error(`Retarget mapping references missing joint ${entry.source} -> ${entry.target}`);
    if (mapping.has(entry.source) || mappedTargets.has(entry.target))
      throw new Error("Retarget mapping must be one-to-one; ambiguous mapping");
    mapping.set(entry.source, entry);
    mappedTargets.add(entry.target);
  }
  const get = (id: string) => {
    const entry = mapping.get(id);
    if (!entry) throw new Error(`Missing explicit retarget mapping for ${id}`);
    return entry;
  };
  // Named mappings establish correspondence; refuse hidden hierarchy reinterpretation.
  for (const entry of request.mapping) {
    const from = sourceJoints.get(entry.source),
      to = targetJoints.get(entry.target);
    if (!from || !to) throw new Error("Missing retarget joint");
    if (from.parent && mapping.has(from.parent) && mapping.get(from.parent)?.target !== to.parent)
      throw new Error(`Mapped parent hierarchy disagrees at ${entry.target}`);
  }
  const keys = original.keys.map((key) => {
    const entry = get(key.joint),
      basis = rotation(entry.basis);
    const identity = entry.basis.every((v) => v === 0);
    return {
      ...key,
      joint: entry.target,
      rotation: identity
        ? ([...key.rotation] as Vec3)
        : euler(multiply(multiply(basis, rotation(key.rotation)), transpose(basis))),
      translation: vector(basis, key.translation.map((v, i) => v * request.translationScale[i]) as Vec3),
    };
  });
  const contactIds = new Set<string>();
  const contacts = (source.creature?.contacts ?? [])
    .filter((c) => c.motion === original.id)
    .map((contact) => {
      const entry = get(contact.joint);
      if (contactIds.has(contact.id)) throw new Error(`Ambiguous source contact identity ${contact.id}`);
      contactIds.add(contact.id);
      if (contact.start >= contact.end || contact.end > original.duration)
        throw new Error(`Invalid source contact interval ${contact.id}`);
      let point: Vec3;
      if (contact.space === "character") {
        if (!request.characterContacts)
          throw new Error("Character contacts need an explicit coordinate transform");
        point = transform(contact.target, request.characterContacts);
      } else {
        if (!request.worldContacts)
          throw new Error("World contacts need an explicit preserve or transform policy");
        point =
          request.worldContacts.kind === "preserve"
            ? [...contact.target]
            : transform(contact.target, request.worldContacts.transform);
      }
      const id = `${request.targetMotion.id}-${contact.id}`;
      idSchema.parse(id);
      if (target.creature?.contacts.some((c) => c.id === id))
        throw new Error(`Target already contains contact ${id}; choose a new motion identity`);
      return creatureContactSchema.parse({
        ...structuredClone(contact),
        id,
        motion: request.targetMotion.id,
        joint: entry.target,
        target: point,
      });
    });
  const motion = motionSchema.parse({ ...original, ...request.targetMotion, keys });
  checkMotion(target, motion);
  return {
    motion,
    contacts,
    provenance: {
      sourceCharacter: source.id,
      sourceMotion: original.id,
      targetCharacter: target.id,
      mapping: request.mapping,
      translationScale: request.translationScale,
    },
    diagnostics: [
      {
        code: "retarget.requires-review",
        message:
          "Retargeted motion retains source timing. Review target joint limits, contacts, collisions, and silhouette before adoption.",
      },
    ],
  };
}

export const creaturePoseSourceSchema = z.strictObject({
  id: idSchema,
  name: z.string().min(1).max(120),
  joints: z
    .array(z.strictObject({ joint: idSchema, rotation: vec3Schema, translation: vec3Schema }))
    .min(1)
    .max(64),
});
export const creatureClipSourceSchema = z.strictObject({
  id: idSchema,
  name: z.string().min(1).max(120),
  duration: z.number().finite().min(0.1).max(120),
  loop: z.boolean(),
  poses: z.array(creaturePoseSourceSchema).min(1).max(128),
  keys: z
    .array(z.strictObject({ time: z.number().finite().min(0).max(120), pose: idSchema }))
    .min(1)
    .max(2048),
});
export type CreaturePoseSource = z.infer<typeof creaturePoseSourceSchema>;
export type CreatureClipSource = z.infer<typeof creatureClipSourceSchema>;
/** Import an explicit reusable pose library and timed pose references as editable joint keys. */
export function authorCreatureMotion(character: CharacterDefinition, input: CreatureClipSource): Motion {
  const source = creatureClipSourceSchema.parse(input),
    poses = new Map(source.poses.map((p) => [p.id, p]));
  if (poses.size !== source.poses.length) throw new Error("Pose identities must be unique");
  for (const pose of poses.values()) {
    if (new Set(pose.joints.map((j) => j.joint)).size !== pose.joints.length)
      throw new Error(`Pose ${pose.id} has ambiguous joint entries`);
    for (const key of pose.joints)
      if (!character.joints.some((j) => j.id === key.joint))
        throw new Error(`Pose ${pose.id} references missing joint ${key.joint}`);
  }
  const keys = source.keys.flatMap((key) => {
    const pose = poses.get(key.pose);
    if (!pose) throw new Error(`Missing authored pose ${key.pose}`);
    return pose.joints.map((j) => ({ ...structuredClone(j), time: key.time }));
  });
  const motion = motionSchema.parse({
    id: source.id,
    name: source.name,
    duration: source.duration,
    loop: source.loop,
    keys,
  });
  checkMotion(character, motion);
  return motion;
}
