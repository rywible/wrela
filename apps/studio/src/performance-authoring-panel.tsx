import {
  emptyPerformance,
  movePerformanceKey,
  type Operation,
  removePerformanceClip,
  retimePerformanceClip,
  upsertMotionKey,
  validatePerformanceEdit,
} from "@wrela/authoring";
import type { CharacterDefinition, CharacterPerformance, PerformanceClip, Vec3 } from "@wrela/model";

import { inspectPerformanceContacts, performanceMotionTrail } from "@wrela/runtime";
import { useEffect, useMemo, useState } from "react";
import type { DomainPanelProps } from "./domain-authoring";
import { PerformanceClipPanel, performanceClipOperations } from "./performance-clip-panel";
import { PerformancePhysicalReview } from "./performance-physical-review";
import {
  PerformanceBlendPanel,
  PerformanceContactPanel,
  PerformanceEventsPanel,
} from "./performance-track-panels";

type Props = DomainPanelProps<CharacterDefinition>;
const createClip = (motion: string): PerformanceClip => ({
  motion,
  events: [],
  facialKeys: [],
  contacts: [],
  alignments: [],
});
const number = (value: string) => Number(value);
export function PerformanceAuthoringPanel({
  document,
  onChange,
  onApply,
  onSeek,
  onPlay,
  onSeekMotion,
  onTransition,
}: Props) {
  const [selected, select] = useState(document.motions[0]?.id ?? "");
  const motion = document.motions.find((clip) => clip.id === selected) ?? document.motions[0];
  const [time, setTime] = useState(0);
  const [jointId, setJoint] = useState(document.joints[0]?.id ?? "");
  const joint = document.joints.find((entry) => entry.id === jointId) ?? document.joints[0];
  const [rotation, setRotation] = useState<Vec3>([0, 0, 0]);
  const [translation, setTranslation] = useState<Vec3>([0, 0, 0]);
  const [projection, setProjection] = useState<"side" | "front" | "top">("side");
  const [error, setError] = useState("");
  const [lane, setLane] = useState<"body" | "face">("body");
  const [moveTime, setMoveTime] = useState(0);
  const performance = document.performance ?? emptyPerformance();
  const clip = performance.clips.find((entry) => entry.motion === motion?.id);
  useEffect(() => {
    const key = (lane === "body" ? motion?.keys : clip?.facialKeys)?.find(
      (entry) => entry.joint === joint?.id && Math.abs(entry.time - time) < 0.005,
    );
    setMoveTime(time);
    setRotation(key ? [...key.rotation] : [0, 0, 0]);
    setTranslation(key ? [...key.translation] : [0, 0, 0]);
  }, [motion, clip, joint, time, lane]);
  const trail = useMemo(
    () =>
      motion && joint
        ? performanceMotionTrail(document.joints, document.motions, document.performance, motion.id, joint.id)
        : [],
    [document, motion, joint],
  );
  const contacts = motion
    ? inspectPerformanceContacts(document.joints, document.motions, document.performance, motion.id, time)
    : [];
  const update = (edit: (value: CharacterPerformance) => void) => {
    try {
      const value = structuredClone(performance);
      edit(value);
      onChange(["performance"], validatePerformanceEdit(document, value), "Edit character performance");
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const editClip = (edit: (value: PerformanceClip) => void) =>
    update((value) => {
      if (!motion) return;
      let target = value.clips.find((entry) => entry.motion === motion.id);
      if (!target) {
        target = createClip(motion.id);
        value.clips.push(target);
      }
      edit(target);
    });
  if (!motion || !joint)
    return (
      <section>
        <h3>Performance</h3>
        <p>Create a skeleton and motion clip to author a performance.</p>
        <button
          type="button"
          onClick={() =>
            onChange(
              ["motions"],
              [{ id: "performance-clip", name: "Performance clip", duration: 1, loop: true, keys: [] }],
              "Create motion clip",
            )
          }
        >
          Create clip
        </button>
      </section>
    );
  const scrub = (seconds: number) => {
    setTime(seconds);
    if (onSeekMotion) onSeekMotion(motion.id, seconds);
    else onSeek?.(seconds);
  };
  const tracks = {
    document,
    motion,
    joint,
    time,
    translation,
    performance,
    clip,
    update,
    editClip,
    scrub,
    contacts,
    onTransition,
  };
  const projected = trail.map((point) => ({
    ...point,
    x: projection === "side" ? point.position[2] : point.position[0],
    y: projection === "top" ? point.position[2] : point.position[1],
  }));
  const xs = projected.map((point) => point.x),
    ys = projected.map((point) => point.y);
  const minX = Math.min(...xs),
    minY = Math.min(...ys),
    span = Math.max(0.1, Math.max(...xs) - minX, Math.max(...ys) - minY);
  const screen = (point: (typeof projected)[number]) => ({
    x: 10 + (120 * (point.x - minX)) / span,
    y: 130 - (120 * (point.y - minY)) / span,
  });
  const cursor = projected.reduce<(typeof projected)[number] | undefined>(
    (nearest, point) =>
      !nearest || Math.abs(point.time - time) < Math.abs(nearest.time - time) ? point : nearest,
    undefined,
  );
  return (
    <section className="performance-authoring">
      <h3>Performance</h3>
      {error && <p role="alert">{error}</p>}
      <label className="row-control">
        Clip{" "}
        <select
          value={motion.id}
          onChange={(event) => {
            select(event.target.value);
            setTime(0);
            if (onSeekMotion) onSeekMotion(event.target.value, 0);
            else onPlay?.(event.target.value);
          }}
        >
          {document.motions.map((value) => (
            <option key={value.id} value={value.id}>
              {value.name}
            </option>
          ))}
        </select>
      </label>
      <label className="row-control">
        Clip name
        <input
          aria-label="Clip name"
          value={motion.name}
          onChange={(event) =>
            onChange(
              ["motions", document.motions.indexOf(motion), "name"],
              event.target.value,
              "Rename motion clip",
            )
          }
        />
      </label>
      <button
        type="button"
        onClick={() => {
          const used = new Set(document.motions.map((entry) => entry.id));
          let index = 1;
          while (used.has(`clip-${index}`)) index++;
          const id = `clip-${index}`;
          onChange(
            ["motions"],
            [...document.motions, { id, name: `New clip ${index}`, duration: 1, loop: false, keys: [] }],
            "Create motion clip",
          );
          select(id);
          setTime(0);
        }}
      >
        Add clip
      </button>
      <button
        type="button"
        onClick={() => {
          const changed = removePerformanceClip(document, motion.id);
          const operations: Operation[] = [
            { kind: "document.set" as const, target: document.id, path: ["motions"], value: changed.motions },
            {
              kind: "document.set" as const,
              target: document.id,
              path: ["performance"],
              value: changed.performance,
            },
          ];
          if (changed.creature)
            operations.push({
              kind: "document.set" as const,
              target: document.id,
              path: ["creature"],
              value: changed.creature,
            });
          onApply(operations, "Remove motion clip and references");
          select(document.motions.find((entry) => entry.id !== motion.id)?.id ?? "");
          setTime(0);
        }}
      >
        Remove clip
      </button>
      <button type="button" onClick={() => onPlay?.(motion.id)} disabled={!onPlay}>
        Play clip
      </button>
      <label className="row-control">
        Duration (seconds)
        <input
          type="number"
          min={0.1}
          max={120}
          step={0.1}
          value={motion.duration}
          onChange={(event) => {
            try {
              const changed = retimePerformanceClip(document, motion.id, number(event.target.value));
              onApply(performanceClipOperations(document, changed), "Retime performance clip");
              setTime(
                Math.min(time, changed.motions.find((entry) => entry.id === motion.id)?.duration ?? time),
              );
              setError("");
            } catch (reason) {
              setError(String(reason));
            }
          }}
        />
      </label>
      <PerformanceClipPanel
        character={document}
        motion={motion}
        onApply={onApply}
        onSelect={(id) => {
          select(id);
          setTime(0);
        }}
        onError={setError}
      />
      <label className="row-control">
        Loop
        <input
          type="checkbox"
          checked={motion.loop}
          onChange={(event) =>
            onChange(
              ["motions", document.motions.indexOf(motion), "loop"],
              event.target.checked,
              "Set clip loop",
            )
          }
        />
      </label>
      <label className="row-control">
        Time {time.toFixed(2)} s
        <input
          aria-label="Performance time"
          type="range"
          min={0}
          max={motion.duration}
          step={0.01}
          value={Math.min(time, motion.duration)}
          onChange={(event) => scrub(number(event.target.value))}
        />
      </label>
      <label className="row-control">
        Joint{" "}
        <select value={joint.id} onChange={(event) => setJoint(event.target.value)}>
          {document.joints.map((value) => (
            <option key={value.id} value={value.id}>
              {value.name}
            </option>
          ))}
        </select>
      </label>
      <fieldset>
        <legend>Pose key at cursor</legend>
        <label className="row-control">
          Editing track
          <select value={lane} onChange={(event) => setLane(event.target.value as typeof lane)}>
            <option value="body">Body pose</option>
            <option value="face">Additive face</option>
          </select>
        </label>
        {(["rotation", "translation"] as const).map((kind) => (
          <div key={kind}>
            {kind}
            {[0, 1, 2].map((axis) => (
              <label className="row-control" key={axis}>
                {["X", "Y", "Z"][axis]}
                <input
                  type="number"
                  step={0.05}
                  value={(kind === "rotation" ? rotation : translation)[axis]}
                  onChange={(event) => {
                    const vector = [...(kind === "rotation" ? rotation : translation)] as Vec3;
                    vector[axis] = number(event.target.value);
                    (kind === "rotation" ? setRotation : setTranslation)(vector);
                  }}
                />
              </label>
            ))}
          </div>
        ))}
        <button
          type="button"
          onClick={() => {
            try {
              onChange(
                ["motions"],
                document.motions.map((entry) =>
                  entry.id === motion.id
                    ? upsertMotionKey(entry, { joint: joint.id, time, rotation, translation })
                    : entry,
                ),
                "Set motion key",
              );
              setError("");
            } catch (reason) {
              setError(String(reason));
            }
          }}
        >
          Set body key
        </button>
        <button
          type="button"
          onClick={() =>
            editClip((entry) => {
              entry.facialKeys = entry.facialKeys.filter(
                (key) => key.joint !== joint.id || key.time !== time,
              );
              entry.facialKeys.push({ joint: joint.id, time, rotation, translation });
            })
          }
        >
          Set additive facial key
        </button>
        <button
          type="button"
          onClick={() =>
            onChange(
              ["motions"],
              document.motions.map((entry) =>
                entry.id === motion.id
                  ? {
                      ...entry,
                      keys: entry.keys.filter(
                        (key) => key.joint !== joint.id || Math.abs(key.time - time) > 0.005,
                      ),
                    }
                  : entry,
              ),
              "Remove motion key",
            )
          }
        >
          Remove body key
        </button>
        <button
          type="button"
          onClick={() =>
            editClip((entry) => {
              entry.facialKeys = entry.facialKeys.filter(
                (key) => key.joint !== joint.id || Math.abs(key.time - time) > 0.005,
              );
            })
          }
        >
          Remove facial key
        </button>
      </fieldset>
      <details>
        <summary>Keys for {joint.name}</summary>
        <label className="row-control">
          Move selected {lane} key to (seconds)
          <input
            type="number"
            min={0}
            max={motion.duration}
            step={0.01}
            value={moveTime}
            onChange={(event) => setMoveTime(Number(event.target.value))}
          />
        </label>
        <button
          type="button"
          onClick={() => {
            try {
              if (lane === "body")
                onChange(
                  ["motions", document.motions.indexOf(motion), "keys"],
                  movePerformanceKey(motion.keys, joint.id, time, moveTime, motion.duration),
                  "Move body key",
                );
              else {
                const keys = movePerformanceKey(
                  clip?.facialKeys ?? [],
                  joint.id,
                  time,
                  moveTime,
                  motion.duration,
                );
                editClip((entry) => {
                  entry.facialKeys = keys;
                });
              }
              scrub(moveTime);
              setError("");
            } catch (reason) {
              setError(reason instanceof Error ? reason.message : String(reason));
            }
          }}
        >
          Move selected key
        </button>
        {motion.keys
          .filter((entry) => entry.joint === joint.id)
          .map((entry) => (
            <button
              key={`body-${entry.time}`}
              type="button"
              onClick={() => {
                setLane("body");
                scrub(entry.time);
              }}
            >
              Body · {entry.time.toFixed(2)} s
            </button>
          ))}
        {clip?.facialKeys
          .filter((entry) => entry.joint === joint.id)
          .map((entry) => (
            <button
              key={`face-${entry.time}`}
              type="button"
              onClick={() => {
                setLane("face");
                scrub(entry.time);
              }}
            >
              Face · {entry.time.toFixed(2)} s
            </button>
          ))}
      </details>
      <PerformanceEventsPanel {...tracks} />
      <PerformanceBlendPanel {...tracks} />
      <PerformanceContactPanel {...tracks} />
      <PerformancePhysicalReview character={document} motion={motion} />
      <details>
        <summary>Joint motion trail</summary>
        <label className="row-control">
          Projection
          <select
            value={projection}
            onChange={(event) => setProjection(event.target.value as typeof projection)}
          >
            <option value="side">Side (travel / height)</option>
            <option value="front">Front (width / height)</option>
            <option value="top">Top (width / travel)</option>
          </select>
        </label>
        <svg
          viewBox="0 0 240 140"
          role="img"
          aria-label={`${joint.name} motion trail, ${projection} projection`}
        >
          <title>
            {joint.name} motion trail, {projection} projection
          </title>
          <polyline
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            points={projected.map((point) => `${screen(point).x},${screen(point).y}`).join(" ")}
          />
          {cursor && <circle cx={screen(cursor).x} cy={screen(cursor).y} r="4" fill="currentColor" />}
        </svg>
        <p>
          {joint.name} · {trail.length} samples · {projection} projection
        </p>
      </details>
    </section>
  );
}
