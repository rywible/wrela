import {
  duplicatePerformanceClip,
  type Operation,
  type PerformanceClipEdit,
  trimPerformanceClip,
} from "@wrela/authoring";
import type { CharacterDefinition, Motion } from "@wrela/model";

import { performanceBoundaryKeys } from "@wrela/runtime";
import { useEffect, useState } from "react";

export function performanceClipOperations(
  character: CharacterDefinition,
  changed: PerformanceClipEdit,
): Operation[] {
  const operations: Operation[] = [
    { kind: "document.set", target: character.id, path: ["motions"], value: changed.motions },
    { kind: "document.set", target: character.id, path: ["performance"], value: changed.performance },
  ];
  if (changed.creature)
    operations.push({
      kind: "document.set",
      target: character.id,
      path: ["creature"],
      value: changed.creature,
    });
  return operations;
}

/** Whole-clip edits remain a single standard undo transaction across semantic and physical tracks. */
export function PerformanceClipPanel({
  character,
  motion,
  onApply,
  onSelect,
  onError,
}: {
  character: CharacterDefinition;
  motion: Motion;
  onApply: (operations: Operation[], label: string) => void;
  onSelect: (id: string) => void;
  onError: (reason: string) => void;
}) {
  const [start, setStart] = useState(0),
    [end, setEnd] = useState(motion.duration);
  useEffect(() => {
    setStart(0);
    setEnd(motion.duration);
  }, [motion.id, motion.duration]);
  const run = (edit: () => void) => {
    try {
      edit();
      onError("");
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  return (
    <details>
      <summary>Duplicate and trim clip</summary>
      <button
        type="button"
        onClick={() =>
          run(() => {
            let index = 1;
            while (character.motions.some((entry) => entry.id === `copied-clip-${index}`)) index++;
            const id = `copied-clip-${index}`;
            onApply(
              performanceClipOperations(character, duplicatePerformanceClip(character, motion.id, id)),
              "Duplicate motion and performance tracks",
            );
            onSelect(id);
          })
        }
      >
        Duplicate clip and tracks
      </button>
      <label className="row-control">
        Keep from (seconds)
        <input
          type="number"
          min={0}
          max={motion.duration}
          step={0.01}
          value={start}
          onChange={(event) => setStart(Number(event.target.value))}
        />
      </label>
      <label className="row-control">
        Keep to (seconds)
        <input
          type="number"
          min={0}
          max={motion.duration}
          step={0.01}
          value={end}
          onChange={(event) => setEnd(Number(event.target.value))}
        />
      </label>
      <p>
        Trimming preserves evaluated boundary poses, clips contact intervals, and moves retained events to the
        new timeline. Review the new endpoints before enabling Loop.
      </p>
      <button
        type="button"
        onClick={() =>
          run(() => {
            const facialKeys =
              character.performance?.clips.find((clip) => clip.motion === motion.id)?.facialKeys ?? [];
            const face = { ...motion, keys: facialKeys };
            const boundaries = (clip: Motion) => ({
              start: performanceBoundaryKeys(character.joints, clip, start),
              end: performanceBoundaryKeys(character.joints, clip, end),
            });
            const changed = trimPerformanceClip(
              character,
              motion.id,
              start,
              end,
              boundaries(motion),
              boundaries(face),
            );
            onApply(performanceClipOperations(character, changed), "Trim motion and all performance tracks");
            onSelect(motion.id);
          })
        }
      >
        Trim clip to range
      </button>
    </details>
  );
}
