import type { CharacterDefinition, CharacterPerformance, Motion } from "@wrela/model";

type Source = Pick<CharacterDefinition, "joints" | "motions" | "performance"> &
  Partial<Pick<CharacterDefinition, "creature">>;
export type PerformanceClipEdit = {
  motions: Motion[];
  performance: CharacterPerformance;
  creature?: CharacterDefinition["creature"];
};
export type ClipBoundaryKeys = { start: Motion["keys"]; end: Motion["keys"] };

/** Copy editable tracks and physical annotations without sharing mutable source objects. */
export function duplicatePerformanceClip(source: Source, motionId: string, id: string): PerformanceClipEdit {
  const motion = source.motions.find((entry) => entry.id === motionId);
  if (!motion) throw new Error(`Unknown motion ${motionId}`);
  if (!id || id.length > 120 || source.motions.some((entry) => entry.id === id))
    throw new Error("A copied clip needs a unique, non-empty identifier");
  if (source.motions.length >= 32) throw new RangeError("Character clip budget exceeded");
  const performance = structuredClone(source.performance ?? { clips: [], transitions: [] });
  const clip = performance.clips.find((entry) => entry.motion === motionId);
  if (clip) performance.clips.push({ ...structuredClone(clip), motion: id });
  const creature = source.creature && structuredClone(source.creature);
  if (creature) {
    const contacts = creature.contacts.filter((entry) => entry.motion === motionId);
    const used = new Set(creature.contacts.map((entry) => entry.id));
    for (const contact of contacts) {
      let index = 1;
      while (used.has(`copied-contact-${index}`)) index++;
      const contactId = `copied-contact-${index}`;
      used.add(contactId);
      creature.contacts.push({ ...structuredClone(contact), id: contactId, motion: id });
    }
    const reviews = creature.reviewScenarios.filter((entry) => entry.motion === motionId);
    const reviewIds = new Set(creature.reviewScenarios.map((entry) => entry.id));
    for (const review of reviews) {
      let index = 1;
      while (reviewIds.has(`copied-review-${index}`)) index++;
      const reviewId = `copied-review-${index}`;
      reviewIds.add(reviewId);
      creature.reviewScenarios.push({
        ...structuredClone(review),
        id: reviewId,
        motion: id,
        name: `${review.name.slice(0, 115)} copy`,
      });
    }
    if (creature.contacts.length > 256 || creature.reviewScenarios.length > 32)
      throw new RangeError("Copied contacts or review scenarios exceed the character budget");
  }
  return {
    motions: [
      ...source.motions,
      { ...structuredClone(motion), id, name: `${motion.name.slice(0, 115)} copy` },
    ],
    performance,
    creature,
  };
}

function cropKeys(
  keys: Motion["keys"],
  start: number,
  end: number,
  boundaries: ClipBoundaryKeys,
  budget: number,
): Motion["keys"] {
  const channels = new Set(keys.map((key) => key.joint));
  for (const boundary of [boundaries.start, boundaries.end]) {
    if ([...channels].some((joint) => boundary.filter((key) => key.joint === joint).length !== 1))
      throw new Error("Trimming requires one evaluated boundary key for each authored channel");
    if (
      boundary.some((key) => [...key.rotation, ...key.translation].some((value) => !Number.isFinite(value)))
    )
      throw new Error("Clip boundary poses must be finite");
  }
  const result = [
    ...boundaries.start
      .filter((key) => channels.has(key.joint))
      .map((key) => ({ ...structuredClone(key), time: 0 })),
    ...keys
      .filter((key) => key.time > start && key.time < end)
      .map((key) => ({ ...structuredClone(key), time: key.time - start })),
    ...boundaries.end
      .filter((key) => channels.has(key.joint))
      .map((key) => ({ ...structuredClone(key), time: end - start })),
  ].sort((a, b) => a.time - b.time || a.joint.localeCompare(b.joint));
  if (result.length > budget) throw new RangeError("Trim boundary keys exceed the track key budget");
  return result;
}

/** Boundary poses come from the runtime sampler, keeping authoring independent of runtime ownership. */
export function trimPerformanceClip(
  source: Source,
  motionId: string,
  start: number,
  end: number,
  body: ClipBoundaryKeys,
  face: ClipBoundaryKeys = { start: [], end: [] },
): PerformanceClipEdit {
  const motion = source.motions.find((entry) => entry.id === motionId);
  if (!motion) throw new Error(`Unknown motion ${motionId}`);
  if (![start, end].every(Number.isFinite) || start < 0 || end > motion.duration || end - start < 0.1)
    throw new RangeError("Trim range must retain at least 0.1 seconds within the clip");
  const duration = end - start;
  const performance = structuredClone(source.performance ?? { clips: [], transitions: [] });
  const clip = performance.clips.find((entry) => entry.motion === motionId);
  if (clip) {
    clip.facialKeys = cropKeys(clip.facialKeys, start, end, face, 512);
    clip.events = clip.events
      .filter((event) => event.time >= start && event.time <= end)
      .map((event) => ({ ...event, time: event.time - start }));
    clip.contacts = clip.contacts
      .filter((contact) => contact.start < end && contact.end > start)
      .map((contact) => ({
        ...contact,
        start: Math.max(start, contact.start) - start,
        end: Math.min(end, contact.end) - start,
      }));
    clip.alignments = clip.alignments
      .filter((alignment) => alignment.time >= start && alignment.time <= end)
      .map((alignment) => ({ ...alignment, time: alignment.time - start }));
  }
  // Cropping changes the cycle seam. A user explicitly opts back into looping after review.
  if (performance.locomotion) {
    performance.locomotion.samples = performance.locomotion.samples.filter(
      (sample) => sample.motion !== motionId,
    );
    if (performance.locomotion.samples.length < 2) performance.locomotion = undefined;
  }
  const creature = source.creature && structuredClone(source.creature);
  if (creature) {
    creature.contacts = creature.contacts.flatMap((contact) =>
      contact.motion !== motionId
        ? [contact]
        : contact.start < end && contact.end > start
          ? [
              {
                ...contact,
                start: Math.max(start, contact.start) - start,
                end: Math.min(end, contact.end) - start,
              },
            ]
          : [],
    );
    creature.reviewScenarios = creature.reviewScenarios.map((scenario) =>
      scenario.motion === motionId ? { ...scenario, duration } : scenario,
    );
  }
  return {
    motions: source.motions.map((entry) =>
      entry.id === motionId
        ? { ...entry, duration, loop: false, keys: cropKeys(entry.keys, start, end, body, 2048) }
        : entry,
    ),
    performance,
    creature,
  };
}
