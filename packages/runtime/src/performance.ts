import type { CharacterPerformance, Joint, Motion, Vec3 } from "@wrela/model";

import { blendPoses, type Pose, poseMatrices, quatMultiply, sampleMotion } from "./animation";

export function resolvePerformanceBlend(
  performance: CharacterPerformance | undefined,
  from: string | undefined,
  to: string,
  fallback: number,
): number {
  return (
    performance?.transitions.find((transition) => transition.from === from && transition.to === to)
      ?.duration ?? fallback
  );
}
export function performanceMotionEvents(
  performance: CharacterPerformance | undefined,
  motion: string | undefined,
) {
  return performance?.clips.find((clip) => clip.motion === motion)?.events;
}
export function jointPosePosition(joints: Joint[], pose: Pose, jointId: string): Vec3 | undefined {
  const index = joints.findIndex((joint) => joint.id === jointId);
  if (index < 0) return undefined;
  const matrix = poseMatrices(joints, pose).subarray(index * 16, index * 16 + 16);
  const position = joints[index].position;
  return [0, 1, 2].map(
    (axis) =>
      matrix[axis] * position[0] +
      matrix[4 + axis] * position[1] +
      matrix[8 + axis] * position[2] +
      matrix[12 + axis],
  ) as Vec3;
}
const clipTime = (motion: Motion, time: number) =>
  motion.loop
    ? ((time % motion.duration) + motion.duration) % motion.duration
    : Math.max(0, Math.min(motion.duration, time));
const smooth = (value: number) => {
  const x = Math.min(1, Math.max(0, value));
  return x * x * (3 - 2 * x);
};

/** Same evaluator drives runtime rendering and authoring diagnostics. No state is changed on seeks. */
export function sampleCharacterPerformance(
  joints: Joint[],
  motions: Motion[],
  performance: CharacterPerformance | undefined,
  motionId: string | undefined,
  time: number,
  accumulateRoot = false,
  locomotionSpeed?: number,
): Pose {
  const motion = motions.find((clip) => clip.id === motionId);
  let pose = sampleMotion(joints, motion, time, accumulateRoot);
  if (!motion || !performance) return pose;
  const locomotion = performance.locomotion;
  const speed = locomotionSpeed ?? locomotion?.speed ?? 0;
  if (locomotion?.enabled && locomotion.samples.some((sample) => sample.motion === motion.id)) {
    const samples = [...locomotion.samples].sort((a, b) => a.speed - b.speed);
    const lower = [...samples].reverse().find((sample) => sample.speed <= speed) ?? samples[0];
    const upper = samples.find((sample) => sample.speed >= speed) ?? samples[samples.length - 1];
    const a = motions.find((clip) => clip.id === lower.motion);
    const b = motions.find((clip) => clip.id === upper.motion);
    if (a && b) {
      const phase = time / motion.duration;
      const weight = upper.speed === lower.speed ? 0 : (speed - lower.speed) / (upper.speed - lower.speed);
      pose = blendPoses(
        sampleMotion(joints, a, phase * a.duration, accumulateRoot),
        sampleMotion(joints, b, phase * b.duration, accumulateRoot),
        weight,
      );
    }
  }
  const clip = performance.clips.find((entry) => entry.motion === motion.id);
  if (!clip) return pose;
  if (clip.facialKeys.length) {
    const facialPose = sampleMotion(joints, facialMotion(motion, clip.facialKeys), time);
    for (const id of new Set(clip.facialKeys.map((key) => key.joint))) {
      const base = pose.get(id),
        face = facialPose.get(id);
      if (base && face)
        pose.set(id, {
          rotation: quatMultiply(base.rotation, face.rotation),
          translation: base.translation.map((value, axis) => value + face.translation[axis]) as Vec3,
        });
    }
  }
  const localTime = clipTime(motion, time);
  for (const alignment of clip.alignments) {
    const distance = localTime - alignment.time;
    const weight =
      alignment.weight *
      smooth(distance <= 0 ? 1 + distance / alignment.blendIn : 1 - distance / alignment.blendOut);
    if (!weight) continue;
    const position = jointPosePosition(joints, pose, alignment.joint);
    let root = joints.find((joint) => joint.id === alignment.joint);
    const visited = new Set<string>();
    while (root?.parent && !visited.has(root.id)) {
      visited.add(root.id);
      root = joints.find((joint) => joint.id === root?.parent);
    }
    const rootPose = root && pose.get(root.id);
    if (position && root && rootPose)
      pose.set(root.id, {
        ...rootPose,
        translation: rootPose.translation.map(
          (value, axis) => value + (alignment.target[axis] - position[axis]) * weight,
        ) as Vec3,
      });
  }
  return pose;
}
// Preserve immutable clip identity so animation track indexing remains cached across frames.
const facialMotions = new WeakMap<Motion, WeakMap<object, Motion>>();
function facialMotion(motion: Motion, keys: Motion["keys"]): Motion {
  let byKeys = facialMotions.get(motion);
  if (!byKeys) {
    byKeys = new WeakMap();
    facialMotions.set(motion, byKeys);
  }
  let result = byKeys.get(keys);
  if (!result) {
    result = { ...motion, id: `${motion.id}:face`, keys };
    byKeys.set(keys, result);
  }
  return result;
}
export type PerformanceContactDiagnostic = {
  joint: string;
  time: number;
  heightError: number;
  slideSpeed: number;
  valid: boolean;
};
export function inspectPerformanceContacts(
  joints: Joint[],
  motions: Motion[],
  performance: CharacterPerformance | undefined,
  motionId: string,
  time: number,
): PerformanceContactDiagnostic[] {
  const motion = motions.find((clip) => clip.id === motionId);
  const clip = performance?.clips.find((entry) => entry.motion === motionId);
  if (!motion || !clip || !performance) return [];
  const t = clipTime(motion, time),
    step = 1 / 120;
  const pose = sampleCharacterPerformance(joints, motions, performance, motionId, time, true);
  const previous = sampleCharacterPerformance(
    joints,
    motions,
    performance,
    motionId,
    time > 0 ? Math.max(0, time - step) : Math.min(motion.duration, time + step),
    true,
  );
  return clip.contacts
    .filter((contact) => t >= contact.start && t <= contact.end)
    .flatMap((contact) => {
      const position = jointPosePosition(joints, pose, contact.joint),
        before = jointPosePosition(joints, previous, contact.joint);
      if (!position || !before) return [];
      const heightError = position[1] - contact.groundHeight;
      const interval = time > 0 ? Math.min(time, step) : Math.min(motion.duration, step);
      const slideSpeed = Math.hypot(position[0] - before[0], position[2] - before[2]) / interval;
      return [
        {
          joint: contact.joint,
          time: t,
          heightError,
          slideSpeed,
          valid: Math.abs(heightError) <= contact.tolerance && slideSpeed <= 0.05,
        },
      ];
    });
}
export function performanceMotionTrail(
  joints: Joint[],
  motions: Motion[],
  performance: CharacterPerformance | undefined,
  motionId: string,
  joint: string,
  samples = 48,
): { time: number; position: Vec3 }[] {
  const motion = motions.find((clip) => clip.id === motionId);
  if (!motion) return [];
  const count = Math.min(240, Math.max(2, Math.floor(samples)));
  const previewMotions = motions.map((value) => (value === motion ? { ...value, loop: false } : value));
  return Array.from({ length: count }, (_, index) => {
    const time = (index * motion.duration) / (count - 1);
    // Sample the authored end key, not the wrapped start pose.
    const position = jointPosePosition(
      joints,
      sampleCharacterPerformance(joints, previewMotions, performance, motionId, time, true),
      joint,
    );
    return position ? { time, position } : undefined;
  }).filter((point): point is { time: number; position: Vec3 } => !!point);
}

/** Evaluate editable boundary keys with the exact runtime interpolation used for rendering. */
export function performanceBoundaryKeys(joints: Joint[], motion: Motion, time: number): Motion["keys"] {
  const pose = sampleMotion(joints, { ...motion, loop: false }, time);
  return [...new Set(motion.keys.map((key) => key.joint))].flatMap((joint) => {
    const value = pose.get(joint);
    if (!value) return [];
    const [x, y, z, w] = value.rotation;
    const sine = Math.max(-1, Math.min(1, 2 * (x * z + y * w)));
    const rotation: Vec3 =
      Math.abs(sine) < 0.9999999
        ? [
            Math.atan2(2 * (x * w - y * z), 1 - 2 * (x * x + y * y)),
            Math.asin(sine),
            Math.atan2(2 * (z * w - x * y), 1 - 2 * (y * y + z * z)),
          ]
        : [Math.atan2(2 * (y * z + x * w), 1 - 2 * (x * x + z * z)), Math.asin(sine), 0];
    return [{ joint, time, rotation, translation: [...value.translation] as Vec3 }];
  });
}

export type PerformanceContactReview = {
  joint: string;
  start: number;
  end: number;
  samples: number;
  maximumHeightError: number;
  maximumSlideSpeed: number;
  worstTime: number;
  valid: boolean;
};

/** Inspect each authored contact through its whole interval, including short windows between frames. */
export function reviewPerformanceContacts(
  joints: Joint[],
  motions: Motion[],
  performance: CharacterPerformance | undefined,
  motionId: string,
): PerformanceContactReview[] {
  const motion = motions.find((entry) => entry.id === motionId);
  const clip = performance?.clips.find((entry) => entry.motion === motionId);
  if (!motion || !clip || !performance) return [];
  const previewMotions = motions.map((entry) => (entry === motion ? { ...entry, loop: false } : entry));
  return clip.contacts.map((contact) => {
    // Bounded work: at most 241 points per contact, with both interval boundaries represented.
    const count = Math.max(2, Math.min(241, Math.ceil((contact.end - contact.start) * 60) + 1));
    let maximumHeightError = 0,
      maximumSlideSpeed = 0,
      worstTime = contact.start,
      severity = -1;
    for (let index = 0; index < count; index++) {
      const time = contact.start + (index / (count - 1)) * (contact.end - contact.start);
      const diagnostic = inspectPerformanceContacts(
        joints,
        previewMotions,
        { ...performance, clips: [{ ...clip, contacts: [contact] }] },
        motionId,
        time,
      )[0];
      if (!diagnostic) continue;
      maximumHeightError = Math.max(maximumHeightError, Math.abs(diagnostic.heightError));
      maximumSlideSpeed = Math.max(maximumSlideSpeed, diagnostic.slideSpeed);
      const score = Math.max(
        Math.abs(diagnostic.heightError) / contact.tolerance,
        diagnostic.slideSpeed / 0.05,
      );
      if (score > severity) {
        severity = score;
        worstTime = time;
      }
    }
    return {
      joint: contact.joint,
      start: contact.start,
      end: contact.end,
      samples: count,
      maximumHeightError,
      maximumSlideSpeed,
      worstTime,
      valid: maximumHeightError <= contact.tolerance && maximumSlideSpeed <= 0.05,
    };
  });
}
