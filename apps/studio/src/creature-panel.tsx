import type { Operation } from "@wrela/authoring";
import { type CharacterDefinition, VIEW_MODES } from "@wrela/model";

import { creatureJointFrames, sampleMotion } from "@wrela/runtime";
import { useEffect, useState } from "react";
import type { StudioController } from "./controller";
import { compareCreatureCaptures } from "./creature-review";

type Capture = Awaited<ReturnType<StudioController["capture"]>>;
type Props = {
  character: CharacterDefinition;
  controller: StudioController;
  act: (fn: () => unknown | Promise<unknown>, message?: string) => void;
};

function NumberEdit({
  label,
  value,
  min,
  max,
  step = 0.01,
  commit,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  commit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  return (
    <label className="row-control">
      <span>{label}</span>
      <input
        aria-label={label}
        type="number"
        value={draft}
        min={min}
        max={max}
        step={step}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
        onBlur={(event) => {
          const number = event.currentTarget.valueAsNumber;
          if (event.currentTarget.validity.valid && Number.isFinite(number)) {
            if (number !== value) commit(number);
          } else setDraft(String(value));
        }}
      />
    </label>
  );
}

function CaptureImage({
  capture,
  label,
}: {
  capture: { blob: Blob; metadata?: { revision: number; tick: number } };
  label: string;
}) {
  const [url, setUrl] = useState("");
  useEffect(() => {
    const next = URL.createObjectURL(capture.blob);
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [capture]);
  return (
    <figure>
      {url && <img src={url} alt={`${label} creature capture`} />}
      <figcaption>
        {label}
        {capture.metadata ? ` · revision ${capture.metadata.revision} · tick ${capture.metadata.tick}` : ""}
      </figcaption>
    </figure>
  );
}

export function CreaturePanel({ character, controller, act }: Props) {
  const creature = character.creature;
  const [regionId, setRegionId] = useState("");
  const [candidateOperations, setCandidateOperations] = useState("[]");
  const [candidateVersion, setCandidateVersion] = useState(0);
  const [inspectedCandidate, setInspectedCandidate] = useState("");
  const [baseline, setBaseline] = useState<Capture>();
  const [comparison, setComparison] = useState<Capture>();
  const [capturing, setCapturing] = useState(false);
  const [sequenceSamples, setSequenceSamples] = useState(4);
  const [candidatePreview, setCandidatePreview] =
    useState<Awaited<ReturnType<StudioController["creature"]["previewCandidate"]>>>();
  const [explanation, setExplanation] = useState<unknown>();
  const [scenarioId, setScenarioId] = useState("");
  const [review, setReview] = useState<ReturnType<StudioController["creature"]["review"]>>();
  const region = creature?.regions.find((item) => item.id === regionId) ?? creature?.regions[0];
  const inspection = controller.getSnapshot();
  const candidates = controller.creature.candidates.list();
  const scenario =
    creature?.reviewScenarios.find((item) => item.id === scenarioId) ?? creature?.reviewScenarios[0];
  const selectedCandidate = inspectedCandidate
    ? controller.creature.candidates.inspect(inspectedCandidate)
    : undefined;
  const apply = (operations: Operation[], label: string) =>
    act(() =>
      controller.apply({
        expectedRevision: controller.authoring.getSnapshot().revision,
        operations,
        label,
      }),
    );
  const set = (path: (string | number)[], value: unknown) => {
    if (!creature) throw Error("Creature source is unavailable");
    const [domain, index, ...property] = path;
    const kinds = {
      regions: "creature.region",
      sculpts: "creature.sculpt",
      anchors: "creature.anchor",
      appearance: "creature.appearance",
      grooms: "creature.groom",
      contacts: "creature.contact",
      ikChains: "creature.ik",
      secondaryChains: "creature.secondary",
      expressions: "creature.expression",
    } as const;
    if (!(String(domain) in kinds) || typeof index !== "number") throw Error("Unsupported creature control");
    const key = domain as keyof typeof kinds;
    const source = structuredClone(creature[key][index]);
    let owner = source as unknown as Record<string | number, unknown>;
    for (const part of property.slice(0, -1)) owner = owner[part] as Record<string | number, unknown>;
    owner[property[property.length - 1]] = value;
    apply(
      [{ kind: kinds[key], target: character.id, value: source } as Operation],
      `Edit ${character.name}: ${path.join(" / ")}`,
    );
  };
  const number = (
    label: string,
    path: (string | number)[],
    value: number,
    min?: number,
    max?: number,
    step?: number,
  ) => (
    <NumberEdit
      label={label}
      value={value}
      min={min}
      max={max}
      step={step}
      commit={(next) => set(path, next)}
    />
  );
  const candidateAction = (fn: () => unknown) =>
    act(() => {
      fn();
      setCandidateVersion((value) => value + 1);
    });
  const capture = (isBaseline: boolean) =>
    act(async () => {
      setCapturing(true);
      try {
        const previous = baseline?.metadata;
        const result = await controller.capture(
          isBaseline || !previous
            ? {
                subject: character.id,
                stage: "studio",
                quality: "interactive",
                tick: 0,
                channels: [inspection.mode],
                overlays: inspection.overlays,
                camera: scenario
                  ? {
                      position: scenario.cameras[0].position,
                      target: scenario.cameras[0].target,
                      fov: 48,
                    }
                  : "three-quarter",
              }
            : {
                subject: character.id,
                stage: "studio",
                stageId: previous.stage === "neutral-stage" ? undefined : previous.stage,
                quality: previous.quality,
                tick: previous.tick,
                channels: previous.channels,
                overlays: previous.overlays,
                camera: previous.camera,
              },
        );
        if (isBaseline) {
          setBaseline(result);
          setComparison(undefined);
        } else setComparison(result);
      } finally {
        setCapturing(false);
      }
    });
  const matched =
    baseline && comparison ? compareCreatureCaptures(baseline.metadata, comparison.metadata) : undefined;
  return (
    <div className="creature-panel" data-candidate-version={candidateVersion}>
      <section>
        <h3>Inspect form before finish</h3>
        <p className="hint">
          Check anatomy and moving joints in clay before judging coat and surface detail.
        </p>
        <div className="button-row">
          <button
            type="button"
            onClick={() =>
              act(() => controller.creature.inspectView({ channel: "clay", skeleton: true, hideGroom: true }))
            }
          >
            Clay + skeleton
          </button>
          <button
            type="button"
            onClick={() =>
              act(() =>
                controller.creature.inspectView({ channel: "beauty", skeleton: false, hideGroom: false }),
              )
            }
          >
            Finished surface
          </button>
        </div>
        <label className="row-control">
          <span>Diagnostic view</span>
          <select
            aria-label="Creature diagnostic view"
            value={inspection.mode}
            onChange={(event) =>
              controller.creature.inspectView({ channel: event.target.value as typeof inspection.mode })
            }
          >
            {VIEW_MODES.map((mode) => (
              <option key={mode} value={mode}>
                {mode.replaceAll("-", " ")}
              </option>
            ))}
          </select>
        </label>
        <label className="row-control">
          <span>Show skeleton</span>
          <input
            type="checkbox"
            checked={inspection.overlays.includes("rig")}
            onChange={(event) => controller.creature.inspectView({ skeleton: event.target.checked })}
          />
        </label>
        <label className="row-control">
          <span>Hide coat</span>
          <input
            type="checkbox"
            checked={inspection.hideGroom}
            onChange={(event) => controller.creature.inspectView({ hideGroom: event.target.checked })}
          />
        </label>
        <p className="hint">Presentation only: the camera, pose and authored creature stay intact.</p>
        <p className="hint">
          Click the creature to trace a visible patch back to anatomy and material source.
        </p>
        {inspection.pickedCreatureMaterial?.documentId === character.id && (
          <details open>
            <summary>Selected surface patch</summary>
            <p>
              {inspection.pickedCreatureMaterial.optical.family} · roughness{" "}
              {inspection.pickedCreatureMaterial.optical.roughness.toFixed(2)}
            </p>
            <div className="button-row">
              {inspection.pickedCreatureMaterial.regions.map((item) => (
                <button type="button" key={item.id} onClick={() => setRegionId(item.id)}>
                  {item.id} ({Math.round(item.weight * 100)}%)
                </button>
              ))}
            </div>
            <details>
              <summary>Source and material evidence</summary>
              <pre className="creature-evidence">
                {JSON.stringify(inspection.pickedCreatureMaterial, null, 2)}
              </pre>
            </details>
          </details>
        )}
      </section>
      {(character.id === "ash-warden" || character.id === "reed-penitent") && (
        <section>
          <h3>Play the creature</h3>
          <p className="hint">
            Test attack readability, dodge timing, reach and recovery against a controllable player marker.
          </p>
          <button
            type="button"
            className="primary"
            disabled={controller.getSnapshot().encounterStarting}
            onClick={() => act(() => controller.creature.encounter.start(character.id))}
          >
            {controller.getSnapshot().encounterStarting
              ? "Preparing encounter…"
              : controller.getSnapshot().encounter
                ? "Restart encounter"
                : "Play encounter"}
          </button>
        </section>
      )}
      {!creature ? (
        <section>
          <h3>Connected creature authoring</h3>
          <p className="hint">
            This character has no anatomical source yet. Open a creature study from the Project panel to
            explore connected anatomy, coat, movement, and review tools.
          </p>
        </section>
      ) : (
        <>
          <section>
            <h3>Anatomy</h3>
            <p className="hint">
              Select a body region to inspect its connected surface, coat, and motion controls.
            </p>
            <nav className="creature-regions" aria-label="Anatomical regions">
              {creature.regions.map((item) => (
                <button
                  type="button"
                  key={item.id}
                  aria-pressed={region?.id === item.id}
                  className={region?.id === item.id ? "selected" : ""}
                  onClick={() => {
                    setRegionId(item.id);
                    setExplanation(undefined);
                  }}
                >
                  <span>{item.name}</span>
                  <small>{item.jointIds.length} joints</small>
                </button>
              ))}
            </nav>
            {region && (
              <>
                <h3>{region.name}</h3>
                <p className="hint">
                  {region.nodeIds.length} shape sources ·{" "}
                  {creature.anchors.filter((anchor) => anchor.region === region.id).length} persistent anchors
                </p>
                {(["x", "y", "z"] as const).map((axis, index) => (
                  <NumberEdit
                    key={axis}
                    label={`${region.name} ${axis} extent`}
                    value={region.extent[index]}
                    min={0.001}
                    max={200}
                    commit={(value) => {
                      const scale: [number, number, number] = [1, 1, 1];
                      scale[index] = value / region.extent[index];
                      apply(
                        [
                          {
                            kind: "creature.proportion",
                            target: character.id,
                            region: region.id,
                            scale,
                            propagate: "region",
                          },
                        ],
                        `Reshape ${region.name}`,
                      );
                    }}
                  />
                ))}
                <button
                  type="button"
                  onClick={() =>
                    act(() => setExplanation(controller.creature.explain(character.id, region.id)))
                  }
                >
                  Explain connected controls
                </button>
                {explanation !== undefined && (
                  <pre className="creature-evidence">{JSON.stringify(explanation, null, 2)}</pre>
                )}
                {creature.anchors
                  .filter((anchor) => anchor.region === region.id)
                  .map((anchor) => (
                    <details key={anchor.id}>
                      <summary>
                        {anchor.id}
                        <small>{anchor.purpose}</small>
                      </summary>
                      <p className="hint">
                        {anchor.chart ?? "Region frame"} · tolerance {anchor.tolerance} m
                      </p>
                      {number(
                        "Surface offset (m)",
                        ["anchors", creature.anchors.indexOf(anchor), "offset"],
                        anchor.offset,
                        -100,
                        100,
                      )}
                      {anchor.coordinates.map((value, index) => (
                        <NumberEdit
                          key={index}
                          label={`Anchor coordinate ${index + 1}`}
                          value={value}
                          commit={(next) =>
                            set(["anchors", creature.anchors.indexOf(anchor), "coordinates", index], next)
                          }
                        />
                      ))}
                    </details>
                  ))}
              </>
            )}
          </section>
          {region && (
            <section>
              <h3>Local form corrections</h3>
              <p className="hint">
                Bounded displacements use this region’s local coordinates and survive regeneration.
              </p>
              <button
                type="button"
                onClick={() =>
                  apply(
                    [
                      {
                        kind: "creature.sculpt",
                        target: character.id,
                        value: {
                          id: `sculpt-${crypto.randomUUID().slice(0, 8)}`,
                          region: region.id,
                          center: [0, 0, 0],
                          radius: Math.min(...region.extent),
                          displacement: [0, 0.01, 0],
                          strength: 1,
                          falloff: 2,
                          mirror: false,
                        },
                      },
                    ],
                    `Add local correction to ${region.name}`,
                  )
                }
              >
                Add local correction
              </button>
              {creature.sculpts
                .filter((stroke) => stroke.region === region.id)
                .map((stroke) => {
                  const index = creature.sculpts.indexOf(stroke);
                  return (
                    <details key={stroke.id} open>
                      <summary>{stroke.id}</summary>
                      <label className="row-control">
                        <span>Form action</span>
                        <select
                          value={stroke.mode ?? "push"}
                          onChange={(event) => set(["sculpts", index, "mode"], event.target.value)}
                        >
                          <option value="push">Push / crease</option>
                          <option value="flatten">Flatten toward a plane</option>
                          <option value="inflate">Inflate from the stroke</option>
                        </select>
                      </label>
                      {!stroke.support && (
                        <button
                          type="button"
                          onClick={() =>
                            set(["sculpts", index, "support"], {
                              radii: [stroke.radius, stroke.radius, stroke.radius],
                              rotation: [0, 0, 0],
                            })
                          }
                        >
                          Shape the influence
                        </button>
                      )}
                      {stroke.support &&
                        (["radii", "rotation"] as const).map((property) =>
                          stroke.support?.[property].map((value, axis) => (
                            <NumberEdit
                              key={`support-${property}-${axis}`}
                              label={`${property === "radii" ? "Influence size" : "Influence angle"} ${["x", "y", "z"][axis]} (${property === "radii" ? "m" : "rad"})`}
                              value={value}
                              commit={(next) => set(["sculpts", index, "support", property, axis], next)}
                            />
                          )),
                        )}
                      {stroke.path?.map((point, pointIndex) => (
                        <details key={pointIndex}>
                          <summary>Stroke point {pointIndex + 1}</summary>
                          {point.map((value, axis) => (
                            <NumberEdit
                              key={axis}
                              label={`${["x", "y", "z"][axis]} (m)`}
                              value={value}
                              commit={(next) => set(["sculpts", index, "path", pointIndex, axis], next)}
                            />
                          ))}
                        </details>
                      ))}
                      {!stroke.path && (
                        <button
                          type="button"
                          onClick={() =>
                            set(
                              ["sculpts", index, "path"],
                              [
                                stroke.center,
                                [stroke.center[0], stroke.center[1] + stroke.radius, stroke.center[2]],
                              ],
                            )
                          }
                        >
                          Extend into a curved stroke
                        </button>
                      )}
                      {!stroke.detail ? (
                        <button
                          type="button"
                          onClick={() =>
                            set(["sculpts", index, "detail"], { maxEdgeLength: 0.03, passes: 4 })
                          }
                        >
                          Preserve fine form
                        </button>
                      ) : (
                        <>
                          {number(
                            "Local edge size (m)",
                            ["sculpts", index, "detail", "maxEdgeLength"],
                            stroke.detail.maxEdgeLength,
                            0.002,
                            2,
                          )}
                          {number(
                            "Refinement passes",
                            ["sculpts", index, "detail", "passes"],
                            stroke.detail.passes,
                            1,
                            6,
                          )}
                        </>
                      )}
                      {number(
                        "Influence radius (m)",
                        ["sculpts", index, "radius"],
                        stroke.radius,
                        0.001,
                        200,
                      )}
                      {number("Strength", ["sculpts", index, "strength"], stroke.strength, -10, 10)}
                      {number("Edge falloff", ["sculpts", index, "falloff"], stroke.falloff, 0.01, 200)}
                      {(["center", "displacement"] as const).map((property) =>
                        stroke[property].map((value, axis) => (
                          <NumberEdit
                            key={`${property}-${axis}`}
                            label={`${property} ${["x", "y", "z"][axis]} (m)`}
                            value={value}
                            commit={(next) => set(["sculpts", index, property, axis], next)}
                          />
                        )),
                      )}
                      <label className="row-control">
                        <span>Mirror across local X</span>
                        <input
                          type="checkbox"
                          checked={stroke.mirror}
                          onChange={(event) => set(["sculpts", index, "mirror"], event.target.checked)}
                        />
                      </label>
                      <button
                        type="button"
                        onClick={() =>
                          apply(
                            [
                              {
                                kind: "creature.remove",
                                target: character.id,
                                domain: "sculpts",
                                id: stroke.id,
                              },
                            ],
                            "Remove local correction",
                          )
                        }
                      >
                        Remove correction
                      </button>
                    </details>
                  );
                })}
            </section>
          )}
          <section>
            <h3>Surface and coat</h3>
            {creature.appearance
              .filter((layer) => layer.region === region?.id)
              .map((layer) => {
                const index = creature.appearance.indexOf(layer);
                return (
                  <details key={layer.id} open>
                    <summary>
                      {layer.id}
                      <small>{layer.family}</small>
                    </summary>
                    {number("Roughness", ["appearance", index, "roughness"], layer.roughness, 0.02, 1)}
                    {number("Tissue scattering", ["appearance", index, "subsurface"], layer.subsurface, 0, 1)}
                    {number("Transmission", ["appearance", index, "transmission"], layer.transmission, 0, 1)}
                    {number(
                      "Directional sheen",
                      ["appearance", index, "anisotropy"],
                      layer.anisotropy,
                      -1,
                      1,
                    )}
                    {number("Detail scale", ["appearance", index, "scale"], layer.scale, 0.001, 200)}
                  </details>
                );
              })}
            {creature.grooms
              .filter((groom) => groom.region === region?.id)
              .map((groom) => {
                const index = creature.grooms.indexOf(groom);
                return (
                  <details key={groom.id} open>
                    <summary>
                      {groom.id}
                      <small>Groom</small>
                    </summary>
                    {number("Fiber length (m)", ["grooms", index, "length"], groom.length, 0.001, 200)}
                    {number("Density", ["grooms", index, "density"], groom.density, 0, 100000, 1)}
                    {number("Clumping", ["grooms", index, "clump"], groom.clump, 0, 1)}
                    {number("Curl", ["grooms", index, "curl"], groom.curl, 0, 20)}
                    {number("Taper", ["grooms", index, "taper"], groom.taper, 0, 1)}
                    {number("Card budget", ["grooms", index, "maxCards"], groom.maxCards, 0, 20000, 1)}
                    <label className="row-control">
                      <span>Coat geometry</span>
                      <select
                        aria-label={`${groom.id} geometry`}
                        value={groom.representation}
                        onChange={(event) => set(["grooms", index, "representation"], event.target.value)}
                      >
                        <option value="tufts">Volumetric tufts</option>
                        <option value="ribbons">Thin ribbons</option>
                      </select>
                    </label>
                    {groom.representation === "ribbons" &&
                      number(
                        "Ribbon thickness ratio",
                        ["grooms", index, "ribbonThickness"],
                        groom.ribbonThickness,
                        0.001,
                        1,
                      )}
                    <p className="hint">
                      {groom.guides.length} authored guides · {groom.lodFractions.length} detail levels
                    </p>
                    <button
                      type="button"
                      disabled={groom.guides.length >= 128}
                      onClick={() => {
                        const length = Math.hypot(...groom.direction);
                        const tip = (
                          length > 0
                            ? groom.direction.map((value) => (value * groom.length) / length)
                            : [0, groom.length, 0]
                        ) as [number, number, number];
                        set(
                          ["grooms", index, "guides"],
                          [
                            ...groom.guides,
                            { id: `guide-${crypto.randomUUID().slice(0, 8)}`, points: [[0, 0, 0], tip] },
                          ],
                        );
                      }}
                    >
                      Add direction guide
                    </button>
                    {groom.guides.map((guide, guideIndex) => (
                      <details key={guide.id}>
                        <summary>{guide.id}</summary>
                        <p className="hint">
                          Guide points are in region-local metres. Nearby fibers follow this curve.
                        </p>
                        {guide.points.map((point, pointIndex) =>
                          point.map((value, axis) => (
                            <NumberEdit
                              key={`${pointIndex}-${axis}`}
                              label={`Point ${pointIndex + 1} ${["x", "y", "z"][axis]}`}
                              value={value}
                              commit={(next) =>
                                set(["grooms", index, "guides", guideIndex, "points", pointIndex, axis], next)
                              }
                            />
                          )),
                        )}
                        <button
                          type="button"
                          disabled={guide.points.length >= 64}
                          onClick={() => {
                            const last = guide.points[guide.points.length - 1],
                              previous = guide.points[guide.points.length - 2];
                            set(
                              ["grooms", index, "guides", guideIndex, "points"],
                              [
                                ...guide.points,
                                last.map((value, axis) => value + (value - previous[axis]) * 0.5),
                              ],
                            );
                          }}
                        >
                          Extend guide
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            set(
                              ["grooms", index, "guides"],
                              groom.guides.filter((item) => item.id !== guide.id),
                            )
                          }
                        >
                          Remove guide
                        </button>
                      </details>
                    ))}
                  </details>
                );
              })}
            {!creature.appearance.some((layer) => layer.region === region?.id) &&
              !creature.grooms.some((layer) => layer.region === region?.id) && (
                <p className="hint">This region has no local appearance or coat layer.</p>
              )}
          </section>
          <section>
            <h3>Performance</h3>
            {scenario?.motion && (
              <button
                type="button"
                onClick={() => {
                  const motion = character.motions.find((item) => item.id === scenario.motion);
                  const joint = region?.jointIds.at(-1) ?? character.joints.at(-1)?.id;
                  if (!motion || !joint) return;
                  const start = Math.max(0, Math.min(inspection.time, motion.duration - 0.05));
                  const target = creatureJointFrames(
                    character.joints,
                    sampleMotion(character.joints, motion, start),
                  ).get(joint)?.position;
                  if (!target) return;
                  apply(
                    [
                      {
                        kind: "creature.contact",
                        target: character.id,
                        value: {
                          id: `contact-${crypto.randomUUID().slice(0, 8)}`,
                          motion: motion.id,
                          joint,
                          start,
                          end: Math.min(motion.duration, start + 0.25),
                          target,
                          space: "character",
                          weight: 1,
                          tolerance: 0.02,
                          blendIn: 0.08,
                          blendOut: 0.08,
                          ground: false,
                          offset: 0,
                        },
                      },
                    ],
                    "Plant selected joint at playhead",
                  );
                }}
              >
                Plant selected joint at playhead
              </button>
            )}
            {creature.contacts.map((contact, index) => (
              <details key={contact.id}>
                <summary>
                  {contact.id}
                  <small>Contact</small>
                </summary>
                <p className="hint">
                  {contact.joint} in {contact.motion} · {contact.space} space
                </p>
                <div className="creature-contact-track" aria-hidden="true">
                  <span
                    style={{
                      left: `${(contact.start / (character.motions.find((item) => item.id === contact.motion)?.duration ?? 1)) * 100}%`,
                      width: `${((contact.end - contact.start) / (character.motions.find((item) => item.id === contact.motion)?.duration ?? 1)) * 100}%`,
                    }}
                  />
                </div>
                {number("Plant starts (s)", ["contacts", index, "start"], contact.start, 0, contact.end)}
                {number("Plant ends (s)", ["contacts", index, "end"], contact.end, contact.start)}
                {number("Contact weight", ["contacts", index, "weight"], contact.weight, 0, 1)}
                {number("Plant blend-in (s)", ["contacts", index, "blendIn"], contact.blendIn, 0, 10)}
                {number("Plant blend-out (s)", ["contacts", index, "blendOut"], contact.blendOut, 0, 10)}
                <label className="row-control">
                  <span>Follow ground height</span>
                  <input
                    type="checkbox"
                    checked={contact.ground}
                    onChange={(event) => set(["contacts", index, "ground"], event.target.checked)}
                  />
                </label>
                {number("Ground offset (m)", ["contacts", index, "offset"], contact.offset, -100, 100)}
                {contact.target.map((value, axis) => (
                  <NumberEdit
                    key={axis}
                    label={`Plant target ${["x", "y", "z"][axis]} (m)`}
                    value={value}
                    commit={(next) => set(["contacts", index, "target", axis], next)}
                  />
                ))}
                <button
                  type="button"
                  onClick={() =>
                    apply(
                      [{ kind: "creature.remove", target: character.id, domain: "contacts", id: contact.id }],
                      "Remove contact interval",
                    )
                  }
                >
                  Remove contact
                </button>
                {number(
                  "Contact tolerance (m)",
                  ["contacts", index, "tolerance"],
                  contact.tolerance,
                  0.001,
                  200,
                )}
              </details>
            ))}
            {creature.ikChains.map((chain, index) => (
              <details key={chain.id}>
                <summary>
                  {chain.id}
                  <small>IK</small>
                </summary>
                {chain.target.map((value, axis) => (
                  <NumberEdit
                    key={axis}
                    label={`Target ${["x", "y", "z"][axis]} (m)`}
                    value={value}
                    commit={(next) => set(["ikChains", index, "target", axis], next)}
                  />
                ))}
                {number("IK weight", ["ikChains", index, "weight"], chain.weight, 0, 1)}
              </details>
            ))}
            {creature.secondaryChains.map((chain, index) => (
              <details key={chain.id}>
                <summary>
                  {chain.id}
                  <small>Secondary motion</small>
                </summary>
                {number("Stiffness", ["secondaryChains", index, "stiffness"], chain.stiffness, 0, 10000)}
                {number("Damping", ["secondaryChains", index, "damping"], chain.damping, 0, 1000)}
                {number(
                  "Angle limit (rad)",
                  ["secondaryChains", index, "maxAngle"],
                  chain.maxAngle,
                  0,
                  Math.PI,
                )}
              </details>
            ))}
            {creature.expressions.map((expression, index) => (
              <NumberEdit
                key={expression.id}
                label={expression.id}
                value={expression.weight}
                min={0}
                max={1}
                commit={(value) => set(["expressions", index, "weight"], value)}
              />
            ))}
          </section>
          <section>
            <h3>Review stage</h3>
            {scenario && (
              <>
                <label className="row-control">
                  <span>Scenario</span>
                  <select
                    aria-label="Creature review scenario"
                    value={scenario.id}
                    onChange={(event) => {
                      setScenarioId(event.target.value);
                      setReview(undefined);
                    }}
                  >
                    {creature.reviewScenarios.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name}
                      </option>
                    ))}
                  </select>
                </label>
                <p className="hint">
                  {scenario.duration} s at {scenario.sampleRate} samples/s · {scenario.cameras.length} views
                </p>
                <button
                  type="button"
                  onClick={() => act(() => setReview(controller.creature.review(character.id, scenario.id)))}
                >
                  Evaluate movement
                </button>
                <button
                  type="button"
                  onClick={() => act(() => controller.creature.playScenario(character.id, scenario.id))}
                >
                  Play review motion
                </button>
                {review !== undefined && (
                  <div className="creature-review-result">
                    <output>
                      {review.status === "passed"
                        ? "Movement checks passed"
                        : review.status === "failed"
                          ? "Movement needs attention"
                          : "Review unavailable"}
                    </output>
                    <p className="hint">
                      {review.sampleCount} motion samples. Appearance still needs visual review.
                    </p>
                    {review.metrics.maximumPlantedSlip !== null && (
                      <p>Maximum planted slip: {(review.metrics.maximumPlantedSlip * 100).toFixed(2)} cm</p>
                    )}
                    {review.diagnostics
                      .filter((item) => !item.passed)
                      .map((item, index) => (
                        <p key={`${item.source}-${index}`} className="hint">
                          <strong>{item.source}</strong>: {item.message}
                        </p>
                      ))}
                    <details>
                      <summary>Review evidence</summary>
                      <pre className="creature-evidence">{JSON.stringify(review, null, 2)}</pre>
                    </details>
                  </div>
                )}
              </>
            )}
            <div className="button-row">
              <button type="button" disabled={capturing} onClick={() => capture(true)}>
                Capture baseline
              </button>
              <button type="button" disabled={capturing || !baseline} onClick={() => capture(false)}>
                Capture comparison
              </button>
            </div>
            {capturing && <output>Capturing the review view…</output>}
            {baseline && <CaptureImage capture={baseline} label="Baseline" />}
            {comparison && <CaptureImage capture={comparison} label="Current" />}
            {matched && (
              <output className="hint">
                {matched.matched
                  ? "Matched review conditions. Inspect the visual change above."
                  : `Conditions differ: ${matched.mismatches.join(", ")}. Capture a new baseline before judging this change.`}
              </output>
            )}
          </section>
        </>
      )}
      <section>
        <h3>Candidate variations</h3>
        <label className="row-control">
          <span>Comparison poses</span>
          <select
            aria-label="Candidate comparison pose count"
            value={sequenceSamples}
            onChange={(event) => setSequenceSamples(Number(event.target.value))}
          >
            <option value={1}>Single pose</option>
            <option value={4}>Four poses</option>
            <option value={8}>Eight poses</option>
          </select>
        </label>
        <p className="hint">
          Propose an isolated edit, inspect its changes, then adopt it as one undoable step.
        </p>
        {region && (
          <button
            type="button"
            onClick={() =>
              candidateAction(() => {
                const id = `broader-${crypto.randomUUID().slice(0, 8)}`;
                controller.creature.candidates.propose({
                  id,
                  batch: {
                    expectedRevision: controller.authoring.getSnapshot().revision,
                    operations: [
                      {
                        kind: "creature.proportion",
                        target: character.id,
                        region: region.id,
                        scale: [1.05, 1, 1],
                        propagate: "region",
                      },
                    ],
                    label: `Explore broader ${region.name}`,
                  },
                });
                setInspectedCandidate(id);
              })
            }
          >
            Explore broader {region.name}
          </button>
        )}
        <label className="creature-block-label" htmlFor="creature-candidate-operations">
          Proposed operations
        </label>
        <textarea
          id="creature-candidate-operations"
          value={candidateOperations}
          spellCheck={false}
          onChange={(event) => setCandidateOperations(event.target.value)}
        />
        <button
          type="button"
          onClick={() =>
            candidateAction(() => {
              const id = `creature-${crypto.randomUUID().slice(0, 8)}`;
              controller.creature.candidates.propose({
                id,
                batch: {
                  expectedRevision: controller.authoring.getSnapshot().revision,
                  operations: JSON.parse(candidateOperations),
                  label: `Explore ${character.name}`,
                },
              });
              setInspectedCandidate(id);
            })
          }
        >
          Propose variation
        </button>
        {candidates.map((candidate) => (
          <div className="creature-candidate" key={candidate.id}>
            <button
              type="button"
              aria-pressed={inspectedCandidate === candidate.id}
              onClick={() => setInspectedCandidate(candidate.id)}
            >
              {candidate.id}
              <small>{candidate.status}</small>
            </button>
            <p className="hint">
              From revision {candidate.sourceRevision} · {candidate.diff.length} definitions
            </p>
            <div className="button-row">
              <button
                type="button"
                disabled={capturing || candidate.status !== "proposed"}
                onClick={() =>
                  act(async () => {
                    setCapturing(true);
                    try {
                      const result = await controller.creature.previewCandidate(candidate.id, {
                        subject: character.id,
                        camera: scenario
                          ? {
                              position: scenario.cameras[0].position,
                              target: scenario.cameras[0].target,
                              fov: 48,
                            }
                          : controller.getSnapshot().camera,
                        motion: scenario?.motion,
                        tick: 0,
                        ticks:
                          scenario && sequenceSamples > 1
                            ? Array.from({ length: sequenceSamples }, (_, index) =>
                                Math.round(
                                  (index * Math.min(600, scenario.duration * 60)) / (sequenceSamples - 1),
                                ),
                              )
                            : undefined,
                        channel: inspection.mode,
                        overlays: inspection.overlays,
                        hideGroom: inspection.hideGroom,
                        width: 320,
                        height: 240,
                      });
                      setInspectedCandidate(candidate.id);
                      setCandidatePreview(result);
                    } finally {
                      setCapturing(false);
                    }
                  })
                }
              >
                Preview candidate
              </button>
              <button
                type="button"
                disabled={candidate.status !== "proposed"}
                onClick={() => candidateAction(() => controller.creature.candidates.adopt(candidate.id))}
              >
                Adopt
              </button>
              <button
                type="button"
                disabled={candidate.status !== "proposed"}
                onClick={() => candidateAction(() => controller.creature.candidates.cancel(candidate.id))}
              >
                Reject
              </button>
              <button
                type="button"
                onClick={() => candidateAction(() => controller.creature.candidates.release(candidate.id))}
              >
                Release
              </button>
            </div>
          </div>
        ))}
        {candidatePreview && (
          <div className="creature-candidate-comparison">
            <h3>Before adoption: {candidatePreview.metadata.candidateId}</h3>
            <p className="hint">
              {candidatePreview.metadata.channel} ·{" "}
              {candidatePreview.metadata.settings.hideGroom ? "coat hidden" : "coat visible"} ·{" "}
              {candidatePreview.metadata.overlays.includes("rig") ? "skeleton shown" : "skeleton hidden"}
            </p>
            <div className="creature-contact-sheet">
              {candidatePreview.frames.map((frame) => (
                <div className="creature-contact-pair" key={frame.tick}>
                  <p>{(frame.tick / 60).toFixed(2)} s</p>
                  <CaptureImage capture={frame.baseline} label="Accepted" />
                  <CaptureImage capture={frame.candidate} label="Candidate" />
                </div>
              ))}
            </div>
            <p className="hint">
              Matched camera and timing at 320 × 240. Previewing does not change accepted source.
            </p>
            {candidatePreview.metadata.acceptedRevision !== controller.authoring.getSnapshot().revision && (
              <p className="hint">
                Source changed since this comparison. Preview again before judging the current work.
              </p>
            )}
            <details>
              <summary>Comparison evidence</summary>
              <pre className="creature-evidence">{JSON.stringify(candidatePreview.metadata, null, 2)}</pre>
            </details>
          </div>
        )}
        {selectedCandidate && (
          <pre className="creature-evidence">{JSON.stringify(selectedCandidate, null, 2)}</pre>
        )}
      </section>
    </div>
  );
}
