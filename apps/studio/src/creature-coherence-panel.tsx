import {
  authorCreatureExpression,
  authorCreatureGarment,
  authorCreatureGarmentLandmarks,
  authorCreatureReview,
  fitCreatureLandmarkSpan,
  measureCreatureLandmarks,
  mountCreatureAttachment,
  type Operation,
  releaseCreatureGarmentFit,
} from "@wrela/authoring";
import type { CharacterDefinition, Vec3 } from "@wrela/model";

import { useEffect, useState } from "react";
import type { DomainPanelProps } from "./domain-authoring";

function NumberControl({
  label,
  value,
  change,
  min,
  max,
}: {
  label: string;
  value: number;
  change: (value: number) => void;
  min?: number;
  max?: number;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  return (
    <label className="row-control">
      <span>{label}</span>
      <input
        aria-label={label}
        type="number"
        step="0.01"
        min={min}
        max={max}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={(event) => {
          const next = event.currentTarget.valueAsNumber;
          if (event.currentTarget.validity.valid && Number.isFinite(next)) {
            if (next !== value) change(next);
          } else setDraft(String(value));
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
    </label>
  );
}
function VectorControl({
  label,
  value,
  change,
}: {
  label: string;
  value: Vec3;
  change: (value: Vec3) => void;
}) {
  return (
    <>
      {value.map((component, axis) => (
        <NumberControl
          key={axis}
          label={`${label} ${["x", "y", "z"][axis]}`}
          value={component}
          change={(next) => change(value.map((v, i) => (i === axis ? next : v)) as Vec3)}
        />
      ))}
    </>
  );
}
const freshId = (prefix: string) => `${prefix.slice(0, 55)}-${crypto.randomUUID().slice(0, 8)}`;

/** Domain source tools complement the existing candidate, sculpt, groom and
 * movement review panel; every action is an ordinary undoable source edit. */
export function CreatureCoherencePanel({
  document: character,
  onChange,
  onApply,
  onReview,
  onSeek,
  onPlay,
}: DomainPanelProps<CharacterDefinition>) {
  const [regionId, setRegionId] = useState("");
  const [firstId, setFirstId] = useState("");
  const [secondId, setSecondId] = useState("");
  const [distance, setDistance] = useState(1);
  const [descendants, setDescendants] = useState(true);
  const [corners, setCorners] = useState<string[]>([]);
  const [jointId, setJointId] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [expressionId, setExpressionId] = useState("");
  const [rotation, setRotation] = useState<Vec3>([0, 0, 0]);
  const [translation, setTranslation] = useState<Vec3>([0, 0, 0]);
  const [motionId, setMotionId] = useState("");
  const [error, setError] = useState("");
  const source = character.creature;
  const region = source?.regions.find((item) => item.id === regionId) ?? source?.regions[0];
  const landmarks = source?.landmarks.filter((item) => item.region === region?.id) ?? [];
  const first = landmarks.find((item) => item.id === firstId) ?? landmarks[0];
  const second = landmarks.find((item) => item.id === secondId) ?? landmarks[1];
  const joint =
    character.joints.find((item) => item.id === jointId) ??
    character.joints.find((item) => region?.jointIds.includes(item.id)) ??
    character.joints[0];
  const availableNodes = character.field.nodes.filter(
    (node) =>
      !node.children.length &&
      !source?.attachments.some((attachment) => attachment.nodeIds.includes(node.id)),
  );
  const node = availableNodes.find((item) => item.id === nodeId) ?? availableNodes[0];
  const motion = character.motions.find((item) => item.id === motionId) ?? character.motions[0];
  useEffect(() => {
    const existing = source?.expressions
      .find((item) => item.id === expressionId)
      ?.weights.find((item) => item.joint === joint?.id);
    setRotation(existing?.rotation ?? [0, 0, 0]);
    setTranslation(existing?.translation ?? [0, 0, 0]);
  }, [character.id, expressionId, joint?.id]);
  const apply = (label: string, make: () => Operation[]) => {
    try {
      setError("");
      onApply(make(), label);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };
  const landmarkOptions = landmarks.map((item) => (
    <option key={item.id} value={item.id}>
      {item.id}
    </option>
  ));
  if (!source || !region)
    return (
      <p className="hint">
        Initialize creature anatomy to author coherent proportions, garments and expressions.
      </p>
    );
  return (
    <section className="creature-coherence-panel">
      <h3>Anatomy, clothing and performance</h3>
      {error && <p role="alert">{error}</p>}
      <label className="row-control">
        <span>Anatomical region</span>
        <select
          aria-label="Coherence anatomical region"
          value={region.id}
          onChange={(event) => {
            setRegionId(event.target.value);
            setCorners([]);
          }}
        >
          {source.regions.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
      <details open>
        <summary>Landmarks and measured proportions</summary>
        <p className="hint">
          Landmarks use the region's local frame. Fitting scales its dependent shape, rig, detail and garment
          source together.
        </p>
        <button
          type="button"
          onClick={() =>
            apply("Add anatomical landmark", () => [
              {
                kind: "creature.landmark",
                target: character.id,
                value: { id: freshId(`${region.id}-landmark`), region: region.id, position: [0, 0, 0] },
              },
            ])
          }
        >
          Add landmark
        </button>
        {landmarks.map((landmark) => (
          <details key={landmark.id}>
            <summary>{landmark.id}</summary>
            <VectorControl
              label="Landmark position (m)"
              value={landmark.position}
              change={(value) =>
                apply("Move anatomical landmark and its fittings", () => [
                  {
                    kind: "creature.landmark",
                    target: character.id,
                    value: { ...landmark, position: value },
                  },
                ])
              }
            />
          </details>
        ))}
        {first && second && (
          <>
            <label className="row-control">
              <span>From landmark</span>
              <select value={first.id} onChange={(event) => setFirstId(event.target.value)}>
                {landmarkOptions}
              </select>
            </label>
            <label className="row-control">
              <span>To landmark</span>
              <select value={second.id} onChange={(event) => setSecondId(event.target.value)}>
                {landmarkOptions}
              </select>
            </label>
            <output>
              Current span: {measureCreatureLandmarks(character, first.id, second.id).distance.toFixed(3)} m
            </output>
            <NumberControl label="Target span (m)" value={distance} min={0.001} change={setDistance} />
            <label className="row-control">
              <span>Include descendant anatomy</span>
              <input
                type="checkbox"
                checked={descendants}
                onChange={(event) => setDescendants(event.target.checked)}
              />
            </label>
            <button
              type="button"
              onClick={() =>
                apply("Fit measured anatomical span", () =>
                  fitCreatureLandmarkSpan(character, {
                    first: first.id,
                    second: second.id,
                    distance,
                    descendants,
                  }),
                )
              }
            >
              Fit proportion to measurement
            </button>
          </>
        )}
      </details>
      <details>
        <summary>Fitted clothing</summary>
        <p className="hint">
          Place four corners on the back of the region, refine their positions, then create a cloth panel
          pinned at its top corners. Editing a fitting landmark keeps the panel and its pins attached.
        </p>
        <button
          type="button"
          onClick={() => {
            const id = freshId(`${region.id}-garment`);
            apply("Place garment fitting landmarks", () => {
              const operations = authorCreatureGarmentLandmarks(character, region.id, id);
              setCorners(
                operations.flatMap((operation) =>
                  operation.kind === "creature.landmark" ? [operation.value.id] : [],
                ),
              );
              return operations;
            });
          }}
        >
          Place garment corners
        </button>
        {landmarks.length >= 4 && (
          <>
            {["Top left", "Top right", "Bottom left", "Bottom right"].map((label, index) => (
              <label className="row-control" key={label}>
                <span>{label}</span>
                <select
                  value={landmarks.find((item) => item.id === corners[index])?.id ?? landmarks[index]?.id}
                  onChange={(event) =>
                    setCorners(
                      [0, 1, 2, 3].map((i) =>
                        i === index ? event.target.value : (corners[i] ?? landmarks[i].id),
                      ),
                    )
                  }
                >
                  {landmarkOptions}
                </select>
              </label>
            ))}
            <button
              type="button"
              onClick={() =>
                apply("Create landmark-fitted cloth", () =>
                  authorCreatureGarment(character, {
                    id: freshId(`${region.id}-cloth`),
                    region: region.id,
                    landmarks: [0, 1, 2, 3].map(
                      (index) =>
                        landmarks.find((item) => item.id === corners[index])?.id ?? landmarks[index].id,
                    ) as [string, string, string, string],
                    material: region.material,
                  }),
                )
              }
            >
              Create pinned cloth panel
            </button>
          </>
        )}
        {source.cloth
          .filter((cloth) => cloth.region === region.id)
          .map((cloth) => (
            <details key={cloth.id}>
              <summary>{cloth.id}</summary>
              {cloth.fittingLandmarks && (
                <>
                  <p className="hint">This garment follows {cloth.fittingLandmarks.join(", ")}.</p>
                  <button
                    type="button"
                    onClick={() =>
                      apply("Release garment fitting landmarks", () =>
                        releaseCreatureGarmentFit(character, cloth.id),
                      )
                    }
                  >
                    Keep current fit and release landmarks
                  </button>
                </>
              )}
              <NumberControl
                label="Cloth stiffness"
                value={cloth.stiffness}
                min={0}
                max={1}
                change={(value) =>
                  onChange(["creature", "cloth", source.cloth.indexOf(cloth), "stiffness"], value)
                }
              />
              <NumberControl
                label="Cloth damping"
                value={cloth.damping}
                min={0}
                max={1}
                change={(value) =>
                  onChange(["creature", "cloth", source.cloth.indexOf(cloth), "damping"], value)
                }
              />
              <NumberControl
                label="Cloth collision thickness (m)"
                value={cloth.collisionRadius}
                min={0}
                max={1}
                change={(value) =>
                  onChange(["creature", "cloth", source.cloth.indexOf(cloth), "collisionRadius"], value)
                }
              />
            </details>
          ))}
      </details>
      <details>
        <summary>Mounted components</summary>
        <p className="hint">
          Select a component shape and the joint that carries it. Its dimensions remain independently authored
          as anatomy changes.
        </p>
        <label className="row-control">
          <span>Attachment joint</span>
          <select value={joint?.id} onChange={(event) => setJointId(event.target.value)}>
            {character.joints.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        {first && node ? (
          <>
            <label className="row-control">
              <span>Mount landmark</span>
              <select value={first.id} onChange={(event) => setFirstId(event.target.value)}>
                {landmarkOptions}
              </select>
            </label>
            <label className="row-control">
              <span>Shape to mount</span>
              <select value={node.id} onChange={(event) => setNodeId(event.target.value)}>
                {availableNodes.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              onClick={() =>
                apply("Mount component on anatomy", () =>
                  mountCreatureAttachment(character, {
                    id: freshId("attachment"),
                    landmark: first.id,
                    node: node.id,
                    joint: joint?.id,
                  }),
                )
              }
            >
              Mount selected shape
            </button>
          </>
        ) : (
          <p className="hint">Add a landmark and an unmounted leaf shape to attach a component.</p>
        )}
        {source.attachments.map((attachment) => (
          <details key={attachment.id}>
            <summary>{attachment.id}</summary>
            <label className="row-control">
              <span>Fit component clearance</span>
              <input
                type="checkbox"
                checked={attachment.placement === "surface"}
                onChange={(event) => {
                  const { placement: _placement, ...value } = attachment;
                  apply("Change component fitting", () => [
                    {
                      kind: "creature.attachment",
                      target: character.id,
                      value: event.target.checked ? { ...value, placement: "surface" } : value,
                    },
                  ]);
                }}
              />
            </label>
            <VectorControl
              label="Attachment offset (m)"
              value={attachment.offset}
              change={(value) =>
                onChange(["creature", "attachments", source.attachments.indexOf(attachment), "offset"], value)
              }
            />
            <NumberControl
              label="Minimum clearance (m)"
              value={attachment.minimumClearance}
              min={0}
              max={100}
              change={(value) =>
                onChange(
                  ["creature", "attachments", source.attachments.indexOf(attachment), "minimumClearance"],
                  value,
                )
              }
            />
          </details>
        ))}
      </details>
      <details>
        <summary>Expressions</summary>
        <label className="row-control">
          <span>Expression</span>
          <select value={expressionId} onChange={(event) => setExpressionId(event.target.value)}>
            <option value="">New expression</option>
            {source.expressions.map((item) => (
              <option key={item.id} value={item.id}>
                {item.id}
              </option>
            ))}
          </select>
        </label>
        <label className="row-control">
          <span>Joint</span>
          <select value={joint?.id} onChange={(event) => setJointId(event.target.value)}>
            {character.joints.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <VectorControl label="Expression rotation (rad)" value={rotation} change={setRotation} />
        <VectorControl label="Expression translation (m)" value={translation} change={setTranslation} />
        <button
          type="button"
          disabled={!joint}
          onClick={() =>
            apply("Author expression joint", () => [
              authorCreatureExpression(character, {
                id: expressionId || freshId("expression"),
                joint: joint.id,
                rotation,
                translation,
              }),
            ])
          }
        >
          Save expression joint
        </button>
        {source.expressions.map((expression, index) => (
          <NumberControl
            key={expression.id}
            label={`${expression.id} blend`}
            value={expression.weight}
            min={0}
            max={1}
            change={(value) => onChange(["creature", "expressions", index, "weight"], value)}
          />
        ))}
      </details>
      <details>
        <summary>Shape and movement review</summary>
        <label className="row-control">
          <span>Review motion</span>
          <select value={motion?.id ?? ""} onChange={(event) => setMotionId(event.target.value)}>
            {character.motions.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          onClick={() =>
            apply("Create anatomy review views", () => [
              authorCreatureReview(character, {
                id: freshId("anatomy-review"),
                region: region.id,
                motion: motion?.id,
              }),
            ])
          }
        >
          Save front, side, back and close-up views
        </button>
        {source.reviewScenarios.map((scenario) => (
          <details key={scenario.id}>
            <summary>{scenario.name}</summary>
            {scenario.cameras.map((camera) => (
              <span className="button-row" key={camera.id}>
                <button
                  type="button"
                  disabled={!onReview}
                  onClick={() => onReview?.(scenario.id, camera.id, "lit")}
                >
                  {camera.id}
                </button>
                <button
                  type="button"
                  disabled={!onReview}
                  onClick={() => onReview?.(scenario.id, camera.id, "silhouette")}
                >
                  {camera.id} silhouette
                </button>
              </span>
            ))}
            <button
              type="button"
              disabled={!onPlay || !scenario.motion}
              onClick={() => onPlay?.(scenario.motion)}
            >
              Play reviewed movement
            </button>
            {[0, 0.25, 0.5, 0.75].map((fraction) => (
              <button
                type="button"
                key={fraction}
                disabled={!onSeek}
                onClick={() => onSeek?.(scenario.duration * fraction)}
              >
                {Math.round(fraction * 100)}% pose
              </button>
            ))}
          </details>
        ))}
      </details>
    </section>
  );
}
