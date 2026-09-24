import type { Operation } from "@wrela/authoring";
import {
  type CharacterDefinition,
  cross,
  dot,
  normalize,
  type Document as SourceDocument,
  sub,
  type Vec3,
} from "@wrela/model";
import { useRef, useState } from "react";
import type { StudioController } from "./controller";
export function AuthoringHandles({
  studio,
  document: doc,
  workspace,
  width,
  height,
  selectedNode,
  onSelect,
  edit,
}: {
  studio: Pick<StudioController, "getSnapshot" | "authoring">;
  document: Extract<SourceDocument, { kind: "object" | "character" }>;
  workspace: string;
  width: number;
  height: number;
  selectedNode: string;
  onSelect: (id: string) => void;
  edit: (ops: Operation[], label?: string, gesture?: string) => void;
}) {
  const camera = studio.getSnapshot().camera,
    forward = normalize(sub(camera.target, camera.position)),
    right = normalize(cross(forward, [0, 1, 0])),
    up = cross(right, forward),
    tangent = Math.tan((camera.fov * Math.PI) / 360);
  const rig = doc.kind === "character" && (workspace === "Rig" || workspace === "Pose");
  const points = rig
    ? doc.joints.map((j, i) => ({
        id: j.id,
        name: j.name,
        position: j.position,
        parent: j.parent,
        path: ["joints", i, "position"] as (string | number)[],
      }))
    : doc.field.nodes
        .filter((n) => !n.children.length)
        .map((n) => ({
          id: n.id,
          name: n.name,
          position: n.position,
          parent: null,
          path: ["field", "nodes", doc.field.nodes.indexOf(n), "position"] as (string | number)[],
        }));
  const project = (p: Vec3) => {
    const delta = sub(p, camera.position),
      depth = dot(delta, forward);
    return {
      x: ((dot(delta, right) / ((depth * tangent * width) / height) + 1) * width) / 2,
      y: ((1 - dot(delta, up) / (depth * tangent)) * height) / 2,
      depth,
    };
  };
  const drag = useRef<{
    id: string;
    x: number;
    y: number;
    position: Vec3;
    gesture: string;
    path: (string | number)[];
    factor: number;
  } | null>(null);
  return (
    <fieldset className="authoring-handles" aria-label={rig ? "Skeleton handles" : "Shape handles"}>
      {rig && (
        <svg aria-hidden="true" width={width} height={height}>
          {points.map((p) => {
            const parent = points.find((j) => j.id === p.parent);
            if (!parent) return null;
            const a = project(p.position),
              b = project(parent.position);
            return <line key={p.id} x1={a.x} y1={a.y} x2={b.x} y2={b.y} />;
          })}
        </svg>
      )}
      {points.map((p) => {
        const screen = project(p.position);
        if (screen.depth <= 0 || screen.x < 0 || screen.y < 0 || screen.x > width || screen.y > height)
          return null;
        return (
          <button
            type="button"
            key={p.id}
            className={`spatial-handle ${selectedNode === p.id ? "selected" : ""}`}
            style={{ left: screen.x, top: screen.y }}
            title={`Drag ${p.name}`}
            aria-label={`Move ${p.name}`}
            onPointerDown={(e) => {
              e.stopPropagation();
              e.currentTarget.setPointerCapture(e.pointerId);
              onSelect(p.id);
              drag.current = {
                id: p.id,
                x: e.clientX,
                y: e.clientY,
                position: [...p.position],
                path: p.path,
                gesture: crypto.randomUUID(),
                factor: (2 * screen.depth * tangent) / height,
              };
            }}
            onPointerMove={(e) => {
              const d = drag.current;
              if (!d || d.id !== p.id) return;
              e.stopPropagation();
              const dx = (e.clientX - d.x) * d.factor,
                dy = (e.clientY - d.y) * d.factor;
              const position = d.position.map((v, i) => v + right[i] * dx - up[i] * dy) as Vec3;
              edit(
                [{ kind: "document.set", target: doc.id, path: d.path, value: position }],
                `Move ${p.name}`,
                d.gesture,
              );
            }}
            onPointerUp={(e) => {
              e.stopPropagation();
              if (drag.current) studio.authoring.endGesture(drag.current.gesture);
              drag.current = null;
            }}
            onPointerCancel={() => {
              drag.current = null;
            }}
          >
            <span />
            <small>{p.name}</small>
          </button>
        );
      })}
    </fieldset>
  );
}
export function PoseEditor({
  studio,
  character,
  edit,
  act,
}: {
  studio: Pick<StudioController, "host" | "patch" | "getSnapshot" | "authoring">;
  character: CharacterDefinition;
  edit: (ops: Operation[], label?: string) => void;
  act: (fn: () => unknown | Promise<unknown>, message?: string) => void;
}) {
  const [joint, setJoint] = useState(character.joints[0].id),
    [rotation, setRotation] = useState<Vec3>([0, 0, 0]),
    [translation, setTranslation] = useState<Vec3>([0, 0, 0]),
    [motion, setMotion] = useState(character.motions[0]?.id ?? "");
  const applyPose = (kind: "rotation" | "translation", axis: number, value: number) => {
    const next = [...(kind === "rotation" ? rotation : translation)] as Vec3;
    next[axis] = value;
    if (kind === "rotation") setRotation(next);
    else setTranslation(next);
    act(() => {
      studio.host?.setPose(
        character.id,
        joint,
        kind === "rotation" ? next : rotation,
        kind === "translation" ? next : translation,
      );
      studio.patch({ playing: false });
    });
  };
  return (
    <section>
      <h3>Pose & key</h3>
      <label className="row-control">
        <span>Joint</span>
        <select
          aria-label="Pose joint"
          value={joint}
          onChange={(e) => {
            setJoint(e.target.value);
            const pose = studio.host?.getPose(character.id, e.target.value);
            setRotation(pose?.rotation ?? [0, 0, 0]);
            setTranslation(pose?.translation ?? [0, 0, 0]);
          }}
        >
          {character.joints.map((j) => (
            <option key={j.id} value={j.id}>
              {j.name}
            </option>
          ))}
        </select>
      </label>
      <p className="hint">Preview a pose, then add it to a motion. Rest anatomy stays independent.</p>
      {rotation.map((v, i) => (
        <NumberControl
          onGestureEnd={(id) => studio.authoring.endGesture(id)}
          key={`r${i}`}
          label={`Pose rotation ${"XYZ"[i]}`}
          value={v}
          min={-3.14}
          max={3.14}
          step={0.02}
          onChange={(value) => applyPose("rotation", i, value)}
        />
      ))}
      {translation.map((v, i) => (
        <NumberControl
          onGestureEnd={(id) => studio.authoring.endGesture(id)}
          key={`t${i}`}
          label={`Pose offset ${"XYZ"[i]}`}
          value={v}
          min={-2}
          max={2}
          step={0.02}
          onChange={(value) => applyPose("translation", i, value)}
        />
      ))}
      <label className="row-control">
        <span>Motion</span>
        <select aria-label="Key motion" value={motion} onChange={(e) => setMotion(e.target.value)}>
          {character.motions.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </select>
      </label>
      <div className="button-row">
        <button
          type="button"
          className="primary"
          disabled={!motion}
          onClick={() => {
            const clip = character.motions.find((m) => m.id === motion);
            if (clip)
              edit(
                [
                  {
                    kind: "character.addKey",
                    target: character.id,
                    motion,
                    joint,
                    time: Math.min(clip.duration, Number(studio.getSnapshot().time.toFixed(2))),
                    rotation,
                    translation,
                  },
                ],
                "Key pose",
              );
          }}
        >
          Add pose key
        </button>
        <button
          type="button"
          onClick={() =>
            act(() => {
              studio.host?.clearPose(character.id);
              setRotation([0, 0, 0]);
              setTranslation([0, 0, 0]);
              studio.patch({});
            })
          }
        >
          Clear pose
        </button>
      </div>
    </section>
  );
}
export function NumberControl({
  onGestureEnd,
  label,
  value,
  min,
  max,
  step = 0.01,
  onChange,
}: {
  onGestureEnd: (gesture: string) => void;
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number, gesture?: string) => void;
}) {
  const gesture = useRef("");
  return (
    <div className="number-control">
      <div>
        <span>{label}</span>
        <input
          aria-label={label}
          type="number"
          value={Number(value.toFixed(4))}
          min={min}
          max={max}
          step={step}
          onChange={(e) => {
            if (e.target.value !== "") onChange(Number(e.target.value));
          }}
        />
      </div>
      <input
        aria-label={`${label} slider`}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onPointerDown={() => {
          gesture.current = crypto.randomUUID();
        }}
        onPointerUp={() => {
          onGestureEnd(gesture.current);
          gesture.current = "";
        }}
        onChange={(e) => onChange(Number(e.target.value), gesture.current || undefined)}
      />
    </div>
  );
}
