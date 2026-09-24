import { z } from "zod";
import type { Joint, Motion } from "./documents";

const id = z.string().min(1).max(120);
const time = z.number().finite().min(0).max(120);
const vec3 = z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]);
const scalar = z.union([z.string().max(256), z.number().finite(), z.boolean()]);
export const performanceEventSchema = z.object({
  id: z.string().min(1).max(100),
  time,
  payload: z.record(z.string().max(100), scalar).refine((value) => Object.keys(value).length <= 16),
});
export const performanceClipSchema = z.object({
  motion: id,
  events: z.array(performanceEventSchema).max(64).default([]),
  /** Additive local joint channels, useful for jaw, eyelids, eyes and brows. */
  facialKeys: z
    .array(z.object({ joint: id, time, rotation: vec3, translation: vec3 }))
    .max(512)
    .default([]),
  contacts: z
    .array(
      z.object({
        joint: id,
        start: time,
        end: time,
        groundHeight: z.number().finite(),
        tolerance: z.number().finite().min(0.001).max(1),
      }),
    )
    .max(64)
    .default([]),
  /** Model-space contact targets. Root correction ramps in before the contact and out afterward. */
  alignments: z
    .array(
      z.object({
        id,
        joint: id,
        time,
        target: vec3,
        blendIn: z.number().finite().min(0.001).max(10),
        blendOut: z.number().finite().min(0.001).max(10),
        weight: z.number().finite().min(0).max(1),
      }),
    )
    .max(16)
    .default([]),
});
export const performanceSchema = z.object({
  clips: z.array(performanceClipSchema).max(32).default([]),
  transitions: z
    .array(z.object({ from: id, to: id, duration: z.number().finite().min(0).max(10) }))
    .max(128)
    .default([]),
  locomotion: z
    .object({
      enabled: z.boolean(),
      speed: z.number().finite().min(0).max(50),
      samples: z
        .array(z.object({ motion: id, speed: z.number().finite().min(0).max(50) }))
        .min(2)
        .max(8),
    })
    .optional(),
});
export type CharacterPerformance = z.infer<typeof performanceSchema>;
export type PerformanceClip = z.infer<typeof performanceClipSchema>;

/** Semantic references must be checked against the owning character, after schema parsing. */
export function performanceIssues(
  joints: readonly Joint[],
  motions: readonly Motion[],
  performance?: CharacterPerformance,
): string[] {
  if (!performance) return [];
  const issues: string[] = [];
  const jointIds = new Set(joints.map((joint) => joint.id));
  const clips = new Map(motions.map((motion) => [motion.id, motion]));
  const seen = new Set<string>();
  for (const clip of performance.clips) {
    const motion = clips.get(clip.motion);
    if (!motion) {
      issues.push(`Performance references missing motion ${clip.motion}`);
      continue;
    }
    if (seen.has(clip.motion)) issues.push(`Duplicate performance clip ${clip.motion}`);
    seen.add(clip.motion);
    const eventIds = new Set<string>();
    for (const event of clip.events) {
      if (event.time > motion.duration) issues.push(`Event ${event.id} exceeds ${motion.id} duration`);
      if (eventIds.has(event.id)) issues.push(`Duplicate event ${event.id} in ${motion.id}`);
      eventIds.add(event.id);
    }
    const channels = new Set<string>();
    for (const key of clip.facialKeys) {
      const channel = `${key.joint}:${key.time}`;
      if (channels.has(channel)) issues.push(`Duplicate facial key ${channel}`);
      channels.add(channel);
      if (!jointIds.has(key.joint)) issues.push(`Missing facial joint ${key.joint}`);
      if (key.time > motion.duration) issues.push(`Facial key exceeds ${motion.id} duration`);
    }
    const alignmentIds = new Set<string>();
    for (const alignment of clip.alignments) {
      if (!jointIds.has(alignment.joint)) issues.push(`Missing alignment joint ${alignment.joint}`);
      if (alignment.time > motion.duration) issues.push(`Alignment ${alignment.id} exceeds clip duration`);
      if (alignmentIds.has(alignment.id)) issues.push(`Duplicate alignment ${alignment.id}`);
      alignmentIds.add(alignment.id);
    }
    for (const contact of clip.contacts) {
      if (!jointIds.has(contact.joint)) issues.push(`Missing contact joint ${contact.joint}`);
      if (contact.start >= contact.end || contact.end > motion.duration)
        issues.push(`Invalid contact interval for ${contact.joint}`);
    }
  }
  const transitions = new Set<string>();
  for (const transition of performance.transitions) {
    if (!clips.has(transition.from) || !clips.has(transition.to))
      issues.push(`Transition references missing motion`);
    const key = `${transition.from}:${transition.to}`;
    if (transitions.has(key)) issues.push(`Duplicate transition ${key}`);
    transitions.add(key);
  }
  const speeds = new Set<number>();
  for (const sample of performance.locomotion?.samples ?? []) {
    const motion = clips.get(sample.motion);
    if (!motion || !motion.loop) issues.push(`Locomotion requires a looping motion: ${sample.motion}`);
    if (speeds.has(sample.speed)) issues.push(`Duplicate locomotion speed ${sample.speed}`);
    speeds.add(sample.speed);
  }
  return issues;
}
