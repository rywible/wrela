import {
  createAssemblyPart,
  createDoorwayAssembly,
  detachAssemblyMate,
  mateAssemblySockets,
  snapAssemblyPartToGrid,
  snapAssemblySockets,
} from "@wrela/authoring";
import {
  type AssemblyDefinition,
  type AssemblyPart,
  assemblySchema,
  type ObjectDefinition,
} from "@wrela/model";

import { useState } from "react";
import { NumberControl, VectorControl } from "./assembly-controls";
import { AssemblyJointPanel } from "./assembly-joint-panel";
import type { DomainPanelProps } from "./domain-authoring";

export function AssemblyAuthoringPanel({ document, project, onChange }: DomainPanelProps<ObjectDefinition>) {
  const [selected, setSelected] = useState(""),
    [error, setError] = useState(""),
    [target, setTarget] = useState(""),
    [movingSocket, setMovingSocket] = useState(""),
    [targetSocket, setTargetSocket] = useState(""),
    [sourceText, setSourceText] = useState("");
  const assembly = document.assembly;
  function save(value: AssemblyDefinition, label = "Edit assembly") {
    const parsed = assemblySchema.safeParse(value);
    if (!parsed.success) {
      setError(parsed.error.issues[0]?.message ?? "Invalid assembly");
      return;
    }
    setError("");
    onChange(["assembly"], parsed.data, label);
  }
  const part = assembly?.parts.find((p) => p.id === selected) ?? assembly?.parts[0];
  function edit(value: Partial<AssemblyPart>) {
    if (assembly && part)
      save({ ...assembly, parts: assembly.parts.map((p) => (p.id === part.id ? { ...p, ...value } : p)) });
  }
  if (!assembly)
    return (
      <section>
        <h3>Parts and assemblies</h3>
        <p>Author dimensioned profiles, swept beams, repeated modules, sockets and articulated machinery.</p>
        <button
          type="button"
          onClick={() =>
            save({ parts: [createAssemblyPart()], grid: 0.05, clearances: [] }, "Create assembly")
          }
        >
          Create assembly
        </button>
        <button type="button" onClick={() => save(createDoorwayAssembly(), "Create dimensioned doorway")}>
          Create 1m doorway
        </button>
      </section>
    );
  return (
    <section>
      <h3>Parts and assemblies</h3>
      <button type="button" onClick={() => onChange(["assembly"], undefined, "Restore field geometry")}>
        Use original field geometry
      </button>
      <p>
        Dimensions in metres; rotations and hinge values in radians. Assembly geometry replaces this object's
        field realization.
      </p>
      {error && <p role="alert">{error}</p>}
      <NumberControl
        label="Snap grid"
        value={assembly.grid}
        min={0.001}
        change={(grid) => save({ ...assembly, grid })}
      />
      <label className="row-control">
        Part
        <select value={part?.id ?? ""} onChange={(event) => setSelected(event.target.value)}>
          {assembly.parts.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        onClick={() => {
          let id = `part-${assembly.parts.length + 1}`;
          while (assembly.parts.some((p) => p.id === id)) id += "-new";
          save({ ...assembly, parts: [...assembly.parts, createAssemblyPart(id)] }, "Add assembly part");
          setSelected(id);
        }}
      >
        Add part
      </button>
      {part && (
        <>
          <label className="row-control">
            Name
            <input
              value={part.name}
              onChange={(event) => {
                if (event.target.value) edit({ name: event.target.value });
              }}
            />
          </label>
          <label className="row-control">
            Parent
            <select
              value={part.parent ?? ""}
              onChange={(event) => edit({ parent: event.target.value || undefined })}
            >
              <option value="">Assembly root</option>
              {assembly.parts
                .filter((p) => p.id !== part.id && p.repeat.count === 1)
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </select>
          </label>
          <label className="row-control">
            Profile
            <select
              value={part.profile.kind}
              onChange={(event) =>
                edit({
                  profile:
                    event.target.value === "circle"
                      ? { kind: "circle", radius: 0.1, segments: 16 }
                      : event.target.value === "polygon"
                        ? {
                            kind: "polygon",
                            points: [
                              [-0.1, -0.1],
                              [0.1, -0.1],
                              [0, 0.15],
                            ],
                          }
                        : { kind: "rectangle", width: 0.2, height: 0.2 },
                })
              }
            >
              <option value="rectangle">Rectangle</option>
              <option value="circle">Circle</option>
              <option value="polygon">Convex polygon</option>
            </select>
          </label>
          {part.profile.kind === "rectangle" && (
            <>
              <NumberControl
                label="Width"
                value={part.profile.width}
                min={0.001}
                change={(width) => {
                  if (part.profile.kind === "rectangle") edit({ profile: { ...part.profile, width } });
                }}
              />
              <NumberControl
                label="Height"
                value={part.profile.height}
                min={0.001}
                change={(height) => {
                  if (part.profile.kind === "rectangle") edit({ profile: { ...part.profile, height } });
                }}
              />
            </>
          )}
          {part.profile.kind === "circle" && (
            <>
              <NumberControl
                label="Radius"
                value={part.profile.radius}
                min={0.001}
                change={(radius) => {
                  if (part.profile.kind === "circle") edit({ profile: { ...part.profile, radius } });
                }}
              />
              <NumberControl
                label="Circle sides"
                value={part.profile.segments}
                min={6}
                max={64}
                step={1}
                change={(segments) => {
                  if (part.profile.kind === "circle")
                    edit({ profile: { ...part.profile, segments: Math.round(segments) } });
                }}
              />
            </>
          )}
          {part.profile.kind === "polygon" && (
            <fieldset>
              <legend>Convex profile vertices</legend>
              {part.profile.points.map((point, i) => (
                <fieldset key={i}>
                  <legend>Vertex {i + 1}</legend>
                  {([0, 1] as const).map((axis) => (
                    <NumberControl
                      key={axis}
                      label={axis === 0 ? "X" : "Y"}
                      value={point[axis]}
                      change={(value) => {
                        if (part.profile.kind !== "polygon") return;
                        const points = part.profile.points.map((p) => [...p] as [number, number]);
                        points[i][axis] = value;
                        edit({ profile: { ...part.profile, points } });
                      }}
                    />
                  ))}
                  <button
                    type="button"
                    disabled={part.profile.kind === "polygon" && part.profile.points.length <= 3}
                    onClick={() => {
                      if (part.profile.kind === "polygon")
                        edit({
                          profile: { ...part.profile, points: part.profile.points.filter((_, n) => n !== i) },
                        });
                    }}
                  >
                    Remove vertex
                  </button>
                </fieldset>
              ))}
              <button
                type="button"
                disabled={part.profile.points.length >= 64}
                onClick={() => {
                  if (part.profile.kind !== "polygon") return;
                  const points = part.profile.points;
                  const a = points[points.length - 1],
                    b = points[0];
                  const dx = b[0] - a[0],
                    dy = b[1] - a[1];
                  edit({
                    profile: {
                      ...part.profile,
                      points: [...points, [(a[0] + b[0]) / 2 + dy * 0.045, (a[1] + b[1]) / 2 - dx * 0.045]],
                    },
                  });
                }}
              >
                Add profile corner
              </button>
            </fieldset>
          )}
          <label className="row-control">
            Material
            <select
              value={part.material ?? ""}
              onChange={(event) => edit({ material: event.target.value || undefined })}
            >
              <option value="">Object material ({document.material})</option>
              {project.documents
                .filter((d) => d.kind === "material")
                .map((material) => (
                  <option key={material.id} value={material.id}>
                    {material.name}
                  </option>
                ))}
            </select>
          </label>
          <label className="row-control">
            <input
              type="checkbox"
              checked={part.collision !== false}
              onChange={(event) => edit({ collision: event.target.checked })}
            />
            Collides with characters and physics objects
          </label>
          <NumberControl
            label="Profile corner bevel"
            value={part.bevel}
            min={0}
            change={(bevel) => edit({ bevel })}
          />
          <NumberControl
            label="End edge bevel"
            value={part.endBevel ?? 0}
            min={0}
            change={(endBevel) => edit({ endBevel })}
          />
          <VectorControl label="Position" value={part.position} change={(position) => edit({ position })} />
          <VectorControl label="Rotation" value={part.rotation} change={(rotation) => edit({ rotation })} />
          {part.path.map((point, i) => (
            <VectorControl
              key={i}
              label={`Sweep point ${i + 1}`}
              value={point}
              change={(value) => edit({ path: part.path.map((p, n) => (n === i ? value : p)) })}
            />
          ))}
          <button
            type="button"
            onClick={() => {
              const end = part.path[part.path.length - 1];
              edit({ path: [...part.path, [end[0], end[1], end[2] + 1]] });
            }}
          >
            Extend sweep
          </button>
          {part.path.length > 2 && (
            <button type="button" onClick={() => edit({ path: part.path.slice(0, -1) })}>
              Remove final point
            </button>
          )}
          <NumberControl
            label="Module count"
            value={part.repeat.count}
            min={1}
            max={128}
            step={1}
            change={(count) => edit({ repeat: { ...part.repeat, count: Math.round(count) } })}
          />
          <VectorControl
            label="Module spacing"
            value={part.repeat.offset}
            change={(offset) => edit({ repeat: { ...part.repeat, offset } })}
          />
          <button
            type="button"
            disabled={!!part.mate}
            onClick={() => save(snapAssemblyPartToGrid(assembly, part.id), "Snap part to grid")}
          >
            Snap to grid
          </button>
          <AssemblyJointPanel part={part} parts={assembly.parts} change={(joint) => edit({ joint })} />
          <NumberControl
            label="Wear"
            value={part.wear.amount}
            min={0}
            max={1}
            change={(amount) => edit({ wear: { ...part.wear, amount } })}
          />
          <NumberControl
            label="Wear frequency"
            value={part.wear.scale}
            min={0.01}
            max={100}
            change={(scale) => edit({ wear: { ...part.wear, scale } })}
          />
          <NumberControl
            label="Wear seed"
            value={part.wear.seed}
            step={1}
            change={(seed) => edit({ wear: { ...part.wear, seed: Math.round(seed) } })}
          />
          <details>
            <summary>Sockets</summary>
            <p>
              Socket frames define position and orientation. Anchors follow sweep endpoints and profile
              dimensions.
            </p>
            {part.sockets.map((socket, i) => (
              <fieldset key={socket.id}>
                <legend>{socket.id}</legend>
                <VectorControl
                  label="Offset"
                  value={socket.position}
                  change={(position) =>
                    edit({ sockets: part.sockets.map((s, n) => (n === i ? { ...s, position } : s)) })
                  }
                />
                <VectorControl
                  label="Orientation"
                  value={socket.rotation ?? [0, 0, 0]}
                  change={(rotation) =>
                    edit({ sockets: part.sockets.map((s, n) => (n === i ? { ...s, rotation } : s)) })
                  }
                />
                <label className="row-control">
                  Anchor
                  <select
                    value={socket.anchor ? String(socket.anchor.point) : "local"}
                    onChange={(event) =>
                      edit({
                        sockets: part.sockets.map((s, n) =>
                          n === i
                            ? {
                                ...s,
                                anchor:
                                  event.target.value === "local"
                                    ? undefined
                                    : { point: event.target.value as "start" | "end", profileOffset: [0, 0] },
                                position: [0, 0, 0],
                              }
                            : s,
                        ),
                      })
                    }
                  >
                    <option value="local">Local point</option>
                    <option value="start">Sweep start</option>
                    <option value="end">Sweep end</option>
                  </select>
                </label>
                {socket.anchor && (
                  <>
                    <NumberControl
                      label="Profile X (-1 to 1)"
                      value={socket.anchor.profileOffset[0]}
                      min={-1}
                      max={1}
                      change={(value) =>
                        edit({
                          sockets: part.sockets.map((s, n) =>
                            n === i && s.anchor
                              ? {
                                  ...s,
                                  anchor: { ...s.anchor, profileOffset: [value, s.anchor.profileOffset[1]] },
                                }
                              : s,
                          ),
                        })
                      }
                    />
                    <NumberControl
                      label="Profile Y (-1 to 1)"
                      value={socket.anchor.profileOffset[1]}
                      min={-1}
                      max={1}
                      change={(value) =>
                        edit({
                          sockets: part.sockets.map((s, n) =>
                            n === i && s.anchor
                              ? {
                                  ...s,
                                  anchor: { ...s.anchor, profileOffset: [s.anchor.profileOffset[0], value] },
                                }
                              : s,
                          ),
                        })
                      }
                    />
                  </>
                )}
                <button
                  type="button"
                  onClick={() => edit({ sockets: part.sockets.filter((_, n) => n !== i) })}
                >
                  Remove socket
                </button>
              </fieldset>
            ))}
            <button
              type="button"
              disabled={part.sockets.length >= 32}
              onClick={() => {
                let id = `socket-${part.sockets.length + 1}`;
                while (part.sockets.some((socket) => socket.id === id)) id += "-new";
                edit({ sockets: [...part.sockets, { id, position: [0, 0, 0] }] });
              }}
            >
              Add socket
            </button>
            {part.mate && (
              <p>
                Fixed to {part.mate.part}/{part.mate.socket}. Position and orientation follow that frame.
              </p>
            )}
            {part.mate && (
              <button
                type="button"
                onClick={() => save(detachAssemblyMate(assembly, part.id), "Detach socket mate")}
              >
                Detach mate, preserve pose
              </button>
            )}
            <label className="row-control">
              Moving socket
              <select value={movingSocket} onChange={(event) => setMovingSocket(event.target.value)}>
                <option value="">Select socket</option>
                {part.sockets.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.id}
                  </option>
                ))}
              </select>
            </label>
            <label className="row-control">
              Target part
              <select
                value={target}
                onChange={(event) => {
                  setTarget(event.target.value);
                  setTargetSocket("");
                }}
              >
                <option value="">Select part</option>
                {assembly.parts
                  .filter((p) => p.id !== part.id)
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </select>
            </label>
            <label className="row-control">
              Target socket
              <select value={targetSocket} onChange={(event) => setTargetSocket(event.target.value)}>
                <option value="">Select socket</option>
                {assembly.parts
                  .find((p) => p.id === target)
                  ?.sockets.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.id}
                    </option>
                  ))}
              </select>
            </label>
            <button
              type="button"
              disabled={!target || !movingSocket || !targetSocket || !!part.mate}
              onClick={() => {
                try {
                  save(
                    snapAssemblySockets(assembly, part.id, movingSocket, target, targetSocket),
                    "Snap sockets",
                  );
                } catch (error) {
                  setError(String(error));
                }
              }}
            >
              Snap positions once
            </button>
            <button
              type="button"
              disabled={!target || !movingSocket || !targetSocket || !!part.joint}
              onClick={() => {
                try {
                  save(
                    mateAssemblySockets(assembly, part.id, movingSocket, target, targetSocket),
                    "Create persistent socket mate",
                  );
                } catch (error) {
                  setError(String(error));
                }
              }}
            >
              Mate socket frames
            </button>
          </details>
          {assembly.parts.length > 1 && (
            <button
              type="button"
              disabled={assembly.parts.some(
                (other) => other.parent === part.id || other.mate?.part === part.id,
              )}
              onClick={() =>
                save(
                  {
                    ...assembly,
                    parts: assembly.parts.filter((p) => p.id !== part.id),
                  },
                  "Remove assembly part",
                )
              }
            >
              Remove part
            </button>
          )}
          {assembly.parts.some((other) => other.parent === part.id || other.mate?.part === part.id) && (
            <p>Detach connected parts before removing this part.</p>
          )}
        </>
      )}
      <details>
        <summary>Clearance volumes</summary>
        <p>Conservative bounds checks flag parts which may obstruct these reserved spaces.</p>
        {assembly.clearances.map((clearance, i) => (
          <fieldset key={clearance.id}>
            <legend>{clearance.name}</legend>
            {(["min", "max"] as const).map((bound) => (
              <VectorControl
                key={bound}
                label={bound}
                value={clearance[bound]}
                change={(value) =>
                  save({
                    ...assembly,
                    clearances: assembly.clearances.map((c, n) => (n === i ? { ...c, [bound]: value } : c)),
                  })
                }
              />
            ))}
            <button
              type="button"
              onClick={() => save({ ...assembly, clearances: assembly.clearances.filter((_, n) => n !== i) })}
            >
              Remove clearance
            </button>
          </fieldset>
        ))}
        <button
          type="button"
          onClick={() =>
            save({
              ...assembly,
              clearances: [
                ...assembly.clearances,
                {
                  id: `clearance-${assembly.clearances.length + 1}`,
                  name: "Traversal clearance",
                  min: [-0.5, 0, -0.5],
                  max: [0.5, 2.1, 0.5],
                },
              ],
            })
          }
        >
          Add clearance
        </button>
      </details>
      <details>
        <summary>Complete assembly source</summary>
        <p>
          Edit polygon points, material bindings and all source fields. Applying validates the entire
          assembly.
        </p>
        <button type="button" onClick={() => setSourceText(JSON.stringify(assembly, null, 2))}>
          Load current source
        </button>
        <textarea
          aria-label="Assembly source"
          rows={16}
          value={sourceText}
          onChange={(event) => setSourceText(event.target.value)}
        />
        <button
          type="button"
          onClick={() => {
            try {
              save(JSON.parse(sourceText), "Apply assembly source");
            } catch (error) {
              setError(String(error));
            }
          }}
        >
          Apply source
        </button>
      </details>
    </section>
  );
}
