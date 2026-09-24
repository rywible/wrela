import type { Motion } from "@wrela/model";

export type MotionEventMarker = {
  id: string;
  time: number;
  payload?: Record<string, string | number | boolean>;
};
export type RuntimeMotionEvent = {
  entityId: string;
  definition: string;
  motion: string;
  eventId: string;
  tick: number;
  cycle: number;
  time: number;
  payload: Record<string, string | number | boolean>;
};
export function compileMotionEvents(
  motion: Motion,
  markers: MotionEventMarker[],
): readonly MotionEventMarker[] {
  if (!Number.isFinite(motion.duration) || motion.duration <= 0) throw new Error("Invalid motion duration");
  if (markers.length > 64) throw new Error("A motion supports at most 64 event markers");
  const ids = new Set<string>();
  for (const marker of markers) {
    if (
      !marker.id ||
      marker.id.length > 100 ||
      ids.has(marker.id) ||
      !Number.isFinite(marker.time) ||
      marker.time < 0 ||
      marker.time > motion.duration
    )
      throw new Error("Invalid motion event marker");
    ids.add(marker.id);
    if (Object.keys(marker.payload ?? {}).length > 16) throw new Error("Motion event payload is too large");
    for (const [key, value] of Object.entries(marker.payload ?? {})) {
      if (
        key.length > 100 ||
        (typeof value === "number" && !Number.isFinite(value)) ||
        (typeof value === "string" && value.length > 256) ||
        !["number", "string", "boolean"].includes(typeof value)
      )
        throw new Error("Invalid motion event payload");
    }
  }
  return structuredClone(markers).sort((a, b) => a.time - b.time || a.id.localeCompare(b.id));
}
/** Half-open traversal avoids repeats on exact tick/loop boundaries. */
export function crossedMotionEvents(
  motion: Motion,
  markers: readonly MotionEventMarker[],
  from: number,
  to: number,
): { marker: MotionEventMarker; cycle: number; time: number }[] {
  if (
    !Number.isFinite(motion.duration) ||
    motion.duration <= 0 ||
    !Number.isFinite(from) ||
    !Number.isFinite(to) ||
    to < from
  )
    throw new Error("Invalid motion event traversal");
  const crossed: { marker: MotionEventMarker; cycle: number; time: number }[] = [];
  const epsilon = 1e-9;
  for (const marker of markers) {
    if (!motion.loop) {
      if (marker.time > from + epsilon && marker.time <= to + epsilon)
        crossed.push({ marker, cycle: 0, time: marker.time });
      continue;
    }
    const first = Math.max(0, Math.floor((from + epsilon - marker.time) / motion.duration) + 1);
    const last = Math.floor((to + epsilon - marker.time) / motion.duration);
    // This API samples a fixed runtime tick, never an unbounded seek interval.
    if (last - first > 1) throw new Error("Motion event interval exceeds fixed-step traversal");
    for (let cycle = first; cycle <= last; cycle++)
      crossed.push({ marker, cycle, time: cycle * motion.duration + marker.time });
  }
  return crossed.sort((a, b) => a.time - b.time || a.marker.id.localeCompare(b.marker.id));
}
