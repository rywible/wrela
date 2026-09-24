import {
  type CharacterDefinition,
  type CharacterPerformance,
  type Motion,
  performanceIssues,
  performanceSchema,
} from "@wrela/model";

export * from "./performance-clips";

export function emptyPerformance(): CharacterPerformance {
  return { clips: [], transitions: [] };
}
/** Remove every semantic reference to a clip in one document transaction. */
export function removePerformanceClip(character: CharacterDefinition, motionId: string) {
  if (!character.motions.some((motion) => motion.id === motionId))
    throw new Error(`Unknown motion ${motionId}`);
  const performance = structuredClone(character.performance ?? emptyPerformance());
  performance.clips = performance.clips.filter((clip) => clip.motion !== motionId);
  performance.transitions = performance.transitions.filter(
    (transition) => transition.from !== motionId && transition.to !== motionId,
  );
  if (performance.locomotion) {
    performance.locomotion.samples = performance.locomotion.samples.filter(
      (sample) => sample.motion !== motionId,
    );
    if (performance.locomotion.samples.length < 2) performance.locomotion = undefined;
  }
  const creature = character.creature && structuredClone(character.creature);
  if (creature) {
    creature.contacts = creature.contacts.filter((contact) => contact.motion !== motionId);
    creature.reviewScenarios = creature.reviewScenarios.filter((scenario) => scenario.motion !== motionId);
  }
  return {
    motions: character.motions.filter((motion) => motion.id !== motionId),
    performance,
    creature,
  };
}
/** Atomic immutable retime: pose keys, face tracks, markers and contact windows retain their alignment. */
export function retimePerformanceClip(
  character: Pick<CharacterDefinition, "joints" | "motions" | "performance"> &
    Partial<Pick<CharacterDefinition, "creature">>,
  motionId: string,
  duration: number,
): { motions: Motion[]; performance: CharacterPerformance; creature?: CharacterDefinition["creature"] } {
  if (!Number.isFinite(duration) || duration < 0.1 || duration > 120)
    throw new RangeError("Clip duration must be between 0.1 and 120 seconds");
  const source = character.motions.find((motion) => motion.id === motionId);
  if (!source) throw new Error(`Unknown motion ${motionId}`);
  const scale = duration / source.duration;
  const motions = character.motions.map((motion) =>
    motion === source
      ? { ...motion, duration, keys: motion.keys.map((key) => ({ ...key, time: key.time * scale })) }
      : motion,
  );
  const performance = structuredClone(character.performance ?? emptyPerformance());
  const clip = performance.clips.find((entry) => entry.motion === motionId);
  if (clip) {
    for (const event of clip.events) event.time *= scale;
    for (const key of clip.facialKeys) key.time *= scale;
    for (const contact of clip.contacts) {
      contact.start *= scale;
      contact.end *= scale;
    }
    for (const alignment of clip.alignments) {
      alignment.time *= scale;
      alignment.blendIn = Math.max(0.001, Math.min(10, alignment.blendIn * scale));
      alignment.blendOut = Math.max(0.001, Math.min(10, alignment.blendOut * scale));
    }
  }
  const creature = character.creature && structuredClone(character.creature);
  if (creature) {
    for (const contact of creature.contacts) {
      if (contact.motion !== motionId) continue;
      contact.start *= scale;
      contact.end *= scale;
      contact.blendIn = Math.min(10, contact.blendIn * scale);
      contact.blendOut = Math.min(10, contact.blendOut * scale);
    }
    for (const scenario of creature.reviewScenarios) {
      if (scenario.motion === motionId) scenario.duration = Math.min(120, scenario.duration * scale);
    }
  }
  return { motions, performance, creature };
}
export function upsertMotionKey(motion: Motion, key: Motion["keys"][number]): Motion {
  if (!Number.isFinite(key.time) || key.time < 0 || key.time > motion.duration)
    throw new RangeError("Key time must be within the clip");
  if ([...key.translation, ...key.rotation].some((value) => !Number.isFinite(value)))
    throw new RangeError("Pose values must be finite");
  const keys = motion.keys.filter(
    (entry) => entry.joint !== key.joint || Math.abs(entry.time - key.time) > 1e-6,
  );
  keys.push(structuredClone(key));
  if (keys.length > 2048) throw new RangeError("Clip key budget exceeded");
  keys.sort((a, b) => a.time - b.time || a.joint.localeCompare(b.joint));
  return { ...motion, keys };
}
export function validatePerformanceEdit(
  character: Pick<CharacterDefinition, "joints" | "motions" | "performance">,
  value: unknown,
): CharacterPerformance {
  const performance = performanceSchema.parse(value);
  const issues = performanceIssues(character.joints, character.motions, performance);
  if (issues.length) throw new Error(issues.join("; "));
  return performance;
}

/** Move a timeline key while protecting a different authored key at the destination. */
export function movePerformanceKey(
  keys: Motion["keys"],
  joint: string,
  from: number,
  to: number,
  duration: number,
): Motion["keys"] {
  if (!Number.isFinite(to) || to < 0 || to > duration)
    throw new RangeError("Key time must be within the clip");
  const key = keys.find((entry) => entry.joint === joint && Math.abs(entry.time - from) < 1e-6);
  if (!key) throw new Error("Select an existing key before moving it");
  if (keys.some((entry) => entry !== key && entry.joint === joint && Math.abs(entry.time - to) < 1e-6))
    throw new Error("Another key already occupies that time");
  return keys
    .map((entry) => (entry === key ? { ...structuredClone(entry), time: to } : entry))
    .sort((a, b) => a.time - b.time || a.joint.localeCompare(b.joint));
}
