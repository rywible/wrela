import type {
  CharacterDefinition,
  CharacterPerformance,
  Joint,
  Motion,
  PerformanceClip,
  Vec3,
} from "@wrela/model";

import type { PerformanceContactDiagnostic } from "@wrela/runtime";
import { useState } from "react";

type TrackProps = {
  document: CharacterDefinition;
  motion: Motion;
  joint: Joint;
  time: number;
  translation: Vec3;
  performance: CharacterPerformance;
  clip?: PerformanceClip;
  update: (edit: (value: CharacterPerformance) => void) => void;
  editClip: (edit: (value: PerformanceClip) => void) => void;
  scrub: (seconds: number) => void;
  contacts: PerformanceContactDiagnostic[];
  onTransition?: (from: string, to: string, at: number, elapsed: number, playing?: boolean) => void;
};
const number = (value: string) => Number(value);

export { PerformanceEventsPanel } from "./performance-event-panel";

export function PerformanceBlendPanel({
  document,
  motion,
  performance,
  update,
  time,
  onTransition,
}: TrackProps) {
  const [transitionTo, setTransitionTo] = useState(document.motions[1]?.id ?? "");
  const [elapsed, setElapsed] = useState(0);
  const transition = performance.transitions.find(
    (entry) => entry.from === motion.id && entry.to === transitionTo,
  );
  return (
    <details>
      <summary>Transitions and locomotion</summary>
      <label className="row-control">
        Transition to{" "}
        <select value={transitionTo} onChange={(event) => setTransitionTo(event.target.value)}>
          <option value="">Choose clip</option>
          {document.motions
            .filter((entry) => entry.id !== motion.id)
            .map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.name}
              </option>
            ))}
        </select>
      </label>
      <button
        type="button"
        disabled={!transitionTo}
        onClick={() =>
          update((value) => {
            value.transitions = value.transitions.filter(
              (entry) => entry.from !== motion.id || entry.to !== transitionTo,
            );
            value.transitions.push({ from: motion.id, to: transitionTo, duration: 0.2 });
          })
        }
      >
        Add transition
      </button>
      {transition && (
        <fieldset>
          <legend>
            Audition {motion.name} → {transitionTo}
          </legend>
          <p>Transition starts at the current clip cursor ({time.toFixed(2)} s).</p>
          <label className="row-control">
            Seconds after transition
            <input
              type="range"
              min={0}
              max={Math.max(0.1, transition.duration)}
              step={0.01}
              value={elapsed}
              onChange={(event) => {
                const seconds = number(event.target.value);
                setElapsed(seconds);
                onTransition?.(motion.id, transitionTo, time, seconds);
              }}
            />
            {elapsed.toFixed(2)} s
          </label>
          <button
            type="button"
            disabled={!onTransition}
            onClick={() => onTransition?.(motion.id, transitionTo, time, 0, true)}
          >
            Play transition from cursor
          </button>
        </fieldset>
      )}
      {performance.transitions.map((entry, index) => (
        <label className="row-control" key={`${entry.from}:${entry.to}`}>
          {entry.from} → {entry.to}
          <input
            aria-label="Blend seconds"
            type="number"
            min={0}
            max={10}
            step={0.05}
            value={entry.duration}
            onChange={(event) =>
              update((value) => {
                value.transitions[index].duration = number(event.target.value);
              })
            }
          />
          <button
            type="button"
            onClick={() =>
              update((value) => {
                value.transitions.splice(index, 1);
              })
            }
          >
            Remove
          </button>
        </label>
      ))}
      <button
        type="button"
        disabled={document.motions.filter((entry) => entry.loop).length < 2}
        onClick={() =>
          update((value) => {
            value.locomotion = {
              enabled: true,
              speed: 0,
              samples: document.motions
                .filter((entry) => entry.loop)
                .slice(0, 8)
                .map((entry, index) => ({ motion: entry.id, speed: index * 2 })),
            };
          })
        }
      >
        Build locomotion blend from looping clips
      </button>
      {performance.locomotion && (
        <div>
          <label className="row-control">
            Blend enabled
            <input
              type="checkbox"
              checked={performance.locomotion.enabled}
              onChange={(event) =>
                update((value) => {
                  if (value.locomotion) value.locomotion.enabled = event.target.checked;
                })
              }
            />
          </label>
          <label className="row-control">
            Speed (m/s)
            <input
              type="range"
              min={0}
              max={Math.max(1, ...performance.locomotion.samples.map((entry) => entry.speed))}
              step={0.05}
              value={performance.locomotion.speed}
              onChange={(event) =>
                update((value) => {
                  if (value.locomotion) value.locomotion.speed = number(event.target.value);
                })
              }
            />
            {performance.locomotion.speed.toFixed(2)}
          </label>
          {performance.locomotion.samples.map((sample, index) => (
            <label className="row-control" key={sample.motion}>
              {sample.motion}
              <input
                type="number"
                min={0}
                max={50}
                step={0.1}
                value={sample.speed}
                onChange={(event) =>
                  update((value) => {
                    if (value.locomotion) value.locomotion.samples[index].speed = number(event.target.value);
                  })
                }
              />
            </label>
          ))}
        </div>
      )}
    </details>
  );
}

export { PerformanceContactPanel } from "./performance-contact-panel";
