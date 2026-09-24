import type { PacketSettings, ResultConstraint, StudyCandidate } from "@wrela/authoring";
import type { Project } from "@wrela/model";
import { type ReviewCaptureCache, reviewAuthoringPacket } from "./packet";
import { reviewAuthoringStudy } from "./study";

/** Explicitly owned, serialized, bounded cross-job renderer/cache. Closing never interrupts an active review. */
export function createAuthoringReviewSession() {
  const cache: ReviewCaptureCache = { captures: new Map(), maxEntries: 32 };
  let tail: Promise<unknown> = Promise.resolve(),
    closed = false;
  function enqueue<T>(run: () => Promise<T>): Promise<T> {
    if (closed) return Promise.reject(Error("Review session is closed"));
    const result = tail.then(run);
    tail = result.catch(() => {});
    return result;
  }
  return {
    study(
      b: Project,
      c: StudyCandidate[],
      cs: ResultConstraint[],
      s: PacketSettings,
      save: (name: string, value: Blob | object) => Promise<string>,
    ) {
      return enqueue(() => reviewAuthoringStudy(b, c, cs, s, save, cache));
    },
    packet(
      b: Project,
      c: Project,
      cs: ResultConstraint[],
      s: PacketSettings,
      save: (name: string, value: Blob | object) => Promise<string>,
    ) {
      return enqueue(() => reviewAuthoringPacket(b, c, cs, s, save, cache));
    },
    async close() {
      closed = true;
      await tail;
      cache.render?.dispose();
      cache.render = undefined;
      cache.captures.clear();
    },
    get retainedCaptures() {
      return cache.captures.size;
    },
  };
}
