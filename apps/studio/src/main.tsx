import {
  importProject,
  type Operation,
  planDefinitionCreation,
  type RigTemplateKind,
  rigTemplate,
} from "@wrela/authoring";
import { creatureFixtureCatalog, referenceProject, shape } from "@wrela/examples";
import {
  type FieldNode,
  normalize,
  type Document as SourceDocument,
  sub,
  type Vec3,
  VIEW_MODES,
} from "@wrela/model";
import type React from "react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { AuthoringHandles, NumberControl, PoseEditor } from "./authoring-controls";
import { AuthoringWorkPanel } from "./authoring-work-panel";
import { downloadBlob, studio } from "./controller";
import { CreatureEncounterPanel } from "./creature-encounter-panel";
import { CreaturePanel } from "./creature-panel";
import { DomainAuthoringPanels } from "./domain-authoring-panels";
import { DomainMotionPreview } from "./domain-motion-preview";
import { RecoveryDrafts } from "./recovery-drafts";
import "./style.css";
declare const WRELA_PRODUCTION: boolean;
const symbols: Record<SourceDocument["kind"], string> = {
  world: "◈",
  terrain: "▱",
  character: "♧",
  object: "⬡",
  vegetation: "♠",
  material: "◐",
  lighting: "☼",
  environment: "◒",
  water: "≈",
  stage: "▣",
};
function Icon({ name }: { name: string }) {
  const paths: Record<string, React.ReactNode> = {
    save: (
      <>
        <path d="M5 3h12l3 3v15H4V3h1Z" />
        <path d="M8 3v6h8V3M8 21v-8h8v8" />
      </>
    ),
    undo: (
      <>
        <path d="m8 4-5 5 5 5M3 9h11a6 6 0 0 1 0 12" />
      </>
    ),
    redo: (
      <>
        <path d="m16 4 5 5-5 5M21 9H10a6 6 0 0 0 0 12" />
      </>
    ),
    camera: (
      <>
        <path d="M3 7h5l2-3h4l2 3h5v13H3Z" />
        <circle cx="12" cy="13" r="4" />
      </>
    ),
    search: (
      <>
        <circle cx="10" cy="10" r="6" />
        <path d="m15 15 6 6" />
      </>
    ),
    play: <path d="m7 4 14 8-14 8Z" />,
    pause: (
      <>
        <path d="M8 4v16M16 4v16" />
      </>
    ),
    export: (
      <>
        <path d="M4 15v6h16v-6M12 17V3m-5 5 5-5 5 5" />
      </>
    ),
    plus: <path d="M12 4v16M4 12h16" />,
    close: <path d="m5 5 14 14M19 5 5 19" />,
    home: (
      <>
        <path d="m3 10 9-7 9 7v11H3Z" />
        <path d="M9 21v-8h6v8" />
      </>
    ),
  };
  return (
    <svg
      width="17"
      height="17"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name] ?? paths.plus}
    </svg>
  );
}
function App() {
  const source = useSyncExternalStore(studio.authoring.subscribe, studio.authoring.getSnapshot),
    preview = useSyncExternalStore(studio.subscribe, studio.getSnapshot);
  const motionPreview = useRef(new DomainMotionPreview(studio));
  const canvas = useRef<HTMLCanvasElement>(null),
    importInput = useRef<HTMLInputElement>(null),
    overlayCanvas = useRef<HTMLCanvasElement>(null),
    runtimeInput = useRef<HTMLInputElement>(null);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [query, setQuery] = useState(""),
    [palette, setPalette] = useState(false),
    [agent, setAgent] = useState(false),
    [add, setAdd] = useState(false),
    [nodeId, setNodeId] = useState(""),
    [workspace, setWorkspace] = useState("Shape"),
    [commands, setCommands] = useState(""),
    [diagnostics, setDiagnostics] = useState(false),
    [handles, setHandles] = useState(false),
    [blendDuration, setBlendDuration] = useState(0.25),
    [rigKind, setRigKind] = useState<RigTemplateKind>("biped"),
    [motionId, setMotionId] = useState("");
  const selected =
    source.project.documents.find((d) => d.id === source.selection) ?? source.project.documents[0];
  const timelineMotion =
    selected.kind === "character"
      ? (selected.motions.find((motion) => motion.id === motionId) ?? selected.motions[0])
      : undefined;
  const timelineDuration = timelineMotion?.duration ?? 12;
  const docs = source.project.documents;
  const busy = preview.status === "starting";
  function act(fn: () => unknown | Promise<unknown>, message?: string) {
    setError("");
    try {
      Promise.resolve(fn())
        .then(() => {
          if (message) {
            setNotice(message);
            setTimeout(() => setNotice(""), 3000);
          }
        })
        .catch((e) => setError(e instanceof Error ? e.message : String(e)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  const edit = (operations: Operation[], label?: string, gesture?: string) =>
    act(() =>
      studio.apply({ expectedRevision: studio.authoring.getSnapshot().revision, operations, label, gesture }),
    );
  const set = (path: (string | number)[], value: unknown, gesture?: string) =>
    edit([{ kind: "document.set", target: selected.id, path, value }], `Edit ${path.join(" / ")}`, gesture);
  useEffect(() => {
    if (!palette && !agent) return;
    const prior = document.activeElement;
    const dialog = document.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) return;
    const focusable = () =>
      Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]',
        ),
      ).filter((element) => element.getClientRects().length > 0);
    focusable()[0]?.focus();
    const trap = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const controls = focusable();
      const first = controls[0],
        last = controls.at(-1);
      if (!first || !last) return;
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) {
        event.preventDefault();
        last.focus();
      } else if (
        !event.shiftKey &&
        (document.activeElement === last || !dialog.contains(document.activeElement))
      ) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", trap);
    return () => {
      document.removeEventListener("keydown", trap);
      if (prior instanceof HTMLElement && prior.isConnected) prior.focus();
    };
  }, [palette, agent]);
  useEffect(() => {
    if (canvas.current) void studio.start(canvas.current, overlayCanvas.current ?? undefined);
    return () => studio.dispose();
  }, []);
  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      const input =
        event.target instanceof HTMLInputElement ||
        event.target instanceof HTMLTextAreaElement ||
        event.target instanceof HTMLSelectElement;
      if (studio.getSnapshot().encounter) {
        if (event.key === "Escape") studio.creature.encounter.stop();
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        act(() => studio.save(), "Project saved");
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z" && !input) {
        event.preventDefault();
        if (event.shiftKey) studio.authoring.redo();
        else studio.authoring.undo();
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPalette((p) => !p);
      }
      if (event.key === "Escape") {
        setPalette(false);
        setAgent(false);
        setAdd(false);
      }
      if (event.code === "Space" && !input) {
        event.preventDefault();
        studio.patch({ playing: !studio.getSnapshot().playing });
      }
      if (event.key.toLowerCase() === "f" && !input) studio.patch({ stage: preview.stage });
    };
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  }, [preview.stage]);
  useEffect(() => {
    motionPreview.current.cancel();
    studio.patch({ reviewSunElevation: undefined, reviewSunAzimuth: undefined });
    setNodeId("");
    setWorkspace(selected.kind === "character" && selected.creature ? "Creature" : "Shape");
    setMotionId("");
  }, [source.selection]);
  useEffect(() => {
    if ("serviceWorker" in navigator && typeof WRELA_PRODUCTION !== "undefined" && WRELA_PRODUCTION)
      void navigator.serviceWorker.register("/sw.js").catch(() => {});
  }, []);
  const number = (
    label: string,
    path: (string | number)[],
    value: number,
    min: number,
    max: number,
    step = 0.01,
  ) => (
    <NumberControl
      onGestureEnd={(id) => studio.authoring.endGesture(id)}
      key={path.join(".")}
      label={label}
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(v, g) => set(path, v, g)}
    />
  );
  const vector = (label: string, path: (string | number)[], value: Vec3, min = -100, max = 100) => (
    <div className="vector-control">
      <span>{label}</span>
      <div>
        {value.map((v, i) => (
          <label key={i}>
            <small>{"XYZ"[i]}</small>
            <input
              aria-label={`${label} ${"XYZ"[i]}`}
              type="number"
              step="0.05"
              min={min}
              max={max}
              value={Number(v.toFixed(3))}
              onChange={(e) => {
                if (e.target.value === "") return;
                const next = [...value];
                next[i] = Number(e.target.value);
                set(path, next);
              }}
            />
          </label>
        ))}
      </div>
    </div>
  );
  const color = (label: string, path: (string | number)[], value: Vec3) => (
    <label className="row-control">
      <span>{label}</span>
      <input
        aria-label={label}
        type="color"
        value={`#${value
          .map((v) =>
            Math.round(v ** (1 / 2.2) * 255)
              .toString(16)
              .padStart(2, "0"),
          )
          .join("")}`}
        onChange={(e) =>
          set(
            path,
            [1, 3, 5].map((i) => (Number.parseInt(e.target.value.slice(i, i + 2), 16) / 255) ** 2.2),
          )
        }
      />
    </label>
  );
  const select = (
    label: string,
    path: (string | number)[],
    value: string,
    options: { id: string; name: string }[],
  ) => (
    <label className="row-control">
      <span>{label}</span>
      <select aria-label={label} value={value} onChange={(e) => set(path, e.target.value)}>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.name}
          </option>
        ))}
      </select>
    </label>
  );
  const surfaceMaterial =
    "material" in selected ? (
      <section>
        <h3>Surface</h3>
        <label className="row-control">
          <span>Material</span>
          <select
            aria-label="Assigned material"
            value={selected.material}
            onChange={(e) =>
              edit([{ kind: "material.assign", target: selected.id, material: e.target.value }])
            }
          >
            {docs
              .filter((d) => d.kind === "material")
              .map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
          </select>
        </label>
        <div className="button-row">
          <button type="button" onClick={() => studio.authoring.select(selected.material)}>
            Edit shared
          </button>
          <button
            type="button"
            onClick={() =>
              edit([
                {
                  kind: "material.makeLocal",
                  target: selected.id,
                  newId: `material-${crypto.randomUUID().slice(0, 8)}`,
                },
              ])
            }
          >
            Make local
          </button>
        </div>
      </section>
    ) : null;
  const field = "field" in selected ? selected.field : null;
  const selectedNode =
    field?.nodes.find((n) => n.id === nodeId) ?? field?.nodes.find((n) => !n.children.length);
  const fieldInspector =
    field && selectedNode ? (
      <>
        <section>
          <h3>
            Shape composition{" "}
            <button
              type="button"
              className="tiny"
              onClick={() => {
                const id = `shape-${crypto.randomUUID().slice(0, 6)}`;
                edit([
                  {
                    kind: "field.add",
                    target: selected.id,
                    node: shape(id, "New sphere", [0, 1, 0], [0.35, 0.35, 0.35], "sphere"),
                  },
                ]);
                setNodeId(id);
              }}
              aria-label="Add shape"
            >
              <Icon name="plus" />
            </button>
          </h3>
          <div className="node-list">
            {field.nodes.map((n) => (
              <button
                type="button"
                key={n.id}
                className={n.id === selectedNode.id ? "selected" : ""}
                onClick={() => setNodeId(n.id)}
              >
                <span>{n.children.length ? "◇" : "○"}</span>
                {n.name}
                <small>{n.kind}</small>
              </button>
            ))}
          </div>
        </section>
        <section>
          <h3>{selectedNode.name}</h3>
          <label className="row-control">
            <span>Name</span>
            <input
              aria-label="Shape name"
              value={selectedNode.name}
              onChange={(e) =>
                edit([
                  {
                    kind: "field.update",
                    target: selected.id,
                    node: selectedNode.id,
                    changes: { name: e.target.value },
                  },
                ])
              }
            />
          </label>
          <label className="row-control">
            <span>Material override</span>
            <select
              aria-label="Shape material"
              value={selectedNode.material ?? ""}
              onChange={(e) =>
                edit([
                  {
                    kind: "field.update",
                    target: selected.id,
                    node: selectedNode.id,
                    changes: { material: e.target.value || undefined },
                  },
                ])
              }
            >
              <option value="">Use subject material</option>
              {docs
                .filter((d) => d.kind === "material")
                .map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
            </select>
          </label>
          {selectedNode.children.length > 0 && (
            <details>
              <summary>Composition inputs</summary>
              {field.nodes
                .filter((n) => n.id !== selectedNode.id && n.id !== field.root)
                .map((n) => (
                  <label className="row-control" key={n.id}>
                    <span>{n.name}</span>
                    <input
                      type="checkbox"
                      checked={selectedNode.children.includes(n.id)}
                      onChange={(e) =>
                        edit([
                          {
                            kind: "field.update",
                            target: selected.id,
                            node: selectedNode.id,
                            changes: {
                              children: e.target.checked
                                ? [...selectedNode.children, n.id]
                                : selectedNode.children.filter((id) => id !== n.id),
                            },
                          },
                        ])
                      }
                    />
                  </label>
                ))}
            </details>
          )}
          <label className="row-control">
            <span>Operation</span>
            <select
              aria-label="Shape type"
              value={selectedNode.kind}
              onChange={(e) =>
                edit([
                  {
                    kind: "field.update",
                    target: selected.id,
                    node: selectedNode.id,
                    changes: { kind: e.target.value as FieldNode["kind"] },
                  },
                ])
              }
            >
              {[
                "sphere",
                "ellipsoid",
                "rock",
                "box",
                "capsule",
                "torus",
                "union",
                "smoothUnion",
                "subtract",
                "intersect",
              ].map((k) => (
                <option key={k}>{k}</option>
              ))}
            </select>
          </label>
          {vector(
            "Position",
            ["field", "nodes", field.nodes.indexOf(selectedNode), "position"],
            selectedNode.position,
          )}
          {vector(
            "Size",
            ["field", "nodes", field.nodes.indexOf(selectedNode), "size"],
            selectedNode.size,
            0.01,
            100,
          )}
          {vector(
            "Rotation",
            ["field", "nodes", field.nodes.indexOf(selectedNode), "rotation"],
            selectedNode.rotation,
            -6.28,
            6.28,
          )}
          {number(
            "Radius",
            ["field", "nodes", field.nodes.indexOf(selectedNode), "radius"],
            selectedNode.radius,
            0.01,
            5,
          )}
          {selectedNode.children.length > 0 &&
            number(
              "Blend",
              ["field", "nodes", field.nodes.indexOf(selectedNode), "blend"],
              selectedNode.blend,
              0,
              2,
            )}
          <details>
            <summary>Extraction bounds & detail</summary>
            {vector("Minimum bounds", ["field", "bounds", "min"], field.bounds.min)}
            {vector("Maximum bounds", ["field", "bounds", "max"], field.bounds.max)}
            {number("Resolution", ["field", "resolution"], field.resolution, 12, 80, 1)}
          </details>
          <button
            type="button"
            className="danger subtle"
            disabled={selectedNode.id === field.root}
            onClick={() => edit([{ kind: "field.remove", target: selected.id, node: selectedNode.id }])}
          >
            Remove shape
          </button>
        </section>
      </>
    ) : null;
  function create(kind: SourceDocument["kind"]) {
    act(() => {
      const { document, dependencies } = planDefinitionCreation(
        studio.authoring.getSnapshot().project,
        kind,
        referenceProject().documents,
      );
      studio.apply({
        expectedRevision: studio.authoring.getSnapshot().revision,
        label: `Create ${kind}`,
        operations: [...dependencies, document].map((definition) => ({
          kind: "document.create",
          document: definition,
        })),
      });
      studio.authoring.select(document.id);
      studio.patch({ stage: "studio" });
      setAdd(false);
    });
  }
  const domainInspector = () => {
    switch (selected.kind) {
      case "object":
        return (
          <>
            {fieldInspector}
            {surfaceMaterial}
            <section>
              <h3>Collision</h3>
              {select(
                "Primitive",
                ["collision"],
                selected.collision,
                ["none", "box", "sphere"].map((id) => ({ id, name: id })),
              )}
            </section>
          </>
        );
      case "character":
        return (
          <>
            <div className="workspace-tabs">
              {["Shape", "Creature", "Rig", "Pose", "Motion", "Physics"].map((w) => (
                <button
                  type="button"
                  key={w}
                  className={workspace === w ? "active" : ""}
                  onClick={() => setWorkspace(w)}
                >
                  {w}
                </button>
              ))}
            </div>
            {workspace === "Creature" && (
              <CreaturePanel key={selected.id} character={selected} controller={studio} act={act} />
            )}
            {workspace === "Shape" && (
              <>
                {fieldInspector}
                {surfaceMaterial}
              </>
            )}
            {workspace === "Rig" && (
              <section>
                <h3>
                  Skeleton{" "}
                  <button
                    type="button"
                    className="tiny"
                    aria-label="Add joint"
                    onClick={() =>
                      edit([
                        {
                          kind: "character.addJoint",
                          target: selected.id,
                          joint: {
                            id: `joint-${crypto.randomUUID().slice(0, 6)}`,
                            name: "New joint",
                            parent: selected.joints[0].id,
                            position: [0, 1, 0],
                            rotation: [0, 0, 0],
                            radius: 0.4,
                            minimum: -1,
                            maximum: 1,
                          },
                        },
                      ])
                    }
                  >
                    <Icon name="plus" />
                  </button>
                </h3>
                <details>
                  <summary>Rig templates</summary>
                  <label className="row-control">
                    <span>Template</span>
                    <select
                      aria-label="Rig template"
                      value={rigKind}
                      onChange={(e) => setRigKind(e.target.value as RigTemplateKind)}
                    >
                      <option value="biped">Biped</option>
                      <option value="quadruped">Quadruped</option>
                      <option value="chain">Flexible chain</option>
                    </select>
                  </label>
                  <p className="hint">
                    Replaces the skeleton and clears motion keys. Undo restores the previous rig and
                    animation.
                  </p>
                  <button
                    type="button"
                    onClick={() =>
                      edit(
                        [
                          {
                            kind: "document.set",
                            target: selected.id,
                            path: ["joints"],
                            value: rigTemplate(rigKind, selected.field.bounds),
                          },
                          { kind: "document.set", target: selected.id, path: ["motions"], value: [] },
                        ],
                        `Apply ${rigKind} rig`,
                      )
                    }
                  >
                    Replace rig
                  </button>
                </details>
                <p className="hint">
                  Influence envelopes bind the regenerated surface to stable joint identities.
                </p>
                {selected.joints.map((j, i) => (
                  <details key={j.id} open={i === 0 || nodeId === j.id}>
                    <summary>
                      {j.name}
                      <small>{j.parent ? "Joint" : "Root"}</small>
                    </summary>
                    <label className="row-control">
                      <span>Name</span>
                      <input
                        aria-label={`Joint name ${i}`}
                        value={j.name}
                        onChange={(e) => set(["joints", i, "name"], e.target.value)}
                      />
                    </label>
                    <label className="row-control">
                      <span>Parent</span>
                      <select
                        aria-label={`Joint parent ${i}`}
                        value={j.parent ?? ""}
                        onChange={(e) => set(["joints", i, "parent"], e.target.value || null)}
                      >
                        <option value="">Root</option>
                        {selected.joints
                          .filter((candidate) => candidate.id !== j.id)
                          .map((candidate) => (
                            <option key={candidate.id} value={candidate.id}>
                              {candidate.name}
                            </option>
                          ))}
                      </select>
                    </label>
                    {vector("Rest position", ["joints", i, "position"], j.position)}
                    {vector("Rest rotation", ["joints", i, "rotation"], j.rotation, -3.14, 3.14)}
                    {number("Influence", ["joints", i, "radius"], j.radius, 0.01, 3)}
                    {number("Minimum", ["joints", i, "minimum"], j.minimum, -3.14, 3.14)}
                    {number("Maximum", ["joints", i, "maximum"], j.maximum, -3.14, 3.14)}
                  </details>
                ))}
                <button
                  type="button"
                  onClick={() => studio.patch({ mode: preview.mode === "binding" ? "beauty" : "binding" })}
                >
                  Inspect binding
                </button>
              </section>
            )}
            {workspace === "Pose" && (
              <PoseEditor studio={studio} key={selected.id} character={selected} edit={edit} act={act} />
            )}
            {workspace === "Motion" && (
              <section>
                <h3>
                  Motion clips{" "}
                  <button
                    type="button"
                    aria-label="Create motion"
                    className="tiny"
                    onClick={() =>
                      set(
                        ["motions"],
                        [
                          ...selected.motions,
                          {
                            id: `motion-${crypto.randomUUID().slice(0, 6)}`,
                            name: "New motion",
                            duration: 3,
                            loop: true,
                            keys: [],
                          },
                        ],
                      )
                    }
                  >
                    <Icon name="plus" />
                  </button>
                </h3>
                {selected.motions.map((m, i) => (
                  <details key={m.id} open>
                    <summary>
                      {m.name}
                      <small>{m.keys.length} keys</small>
                    </summary>
                    {number("Duration", ["motions", i, "duration"], m.duration, 0.1, 120, 0.1)}
                    <NumberControl
                      onGestureEnd={(id) => studio.authoring.endGesture(id)}
                      label="Blend duration"
                      value={blendDuration}
                      min={0}
                      max={5}
                      step={0.05}
                      onChange={setBlendDuration}
                    />
                    <button
                      type="button"
                      onClick={() =>
                        act(() => {
                          setMotionId(m.id);
                          studio.host?.playMotion(selected.id, m.id, blendDuration);
                          studio.patch({ playing: true });
                        })
                      }
                    >
                      Blend to {m.name}
                    </button>
                    <label className="row-control">
                      <span>Loop</span>
                      <input
                        type="checkbox"
                        checked={m.loop}
                        onChange={(e) => set(["motions", i, "loop"], e.target.checked)}
                      />
                    </label>
                    <button
                      type="button"
                      onClick={() =>
                        act(async () => {
                          await studio.seek(0);
                          setMotionId(m.id);
                          studio.host?.playMotion(selected.id, m.id, 0);
                          studio.patch({ playing: true });
                        })
                      }
                    >
                      Preview {m.name}
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        edit([
                          {
                            kind: "character.addKey",
                            target: selected.id,
                            motion: m.id,
                            joint: selected.joints[0].id,
                            time: Math.min(m.duration, Number(preview.time.toFixed(2))),
                            rotation: selected.joints[0].rotation,
                            translation: [0, 0, 0],
                          },
                        ])
                      }
                    >
                      Key root at playhead
                    </button>
                    {m.keys.map((k, ki) => (
                      <details key={`${k.joint}-${ki}`}>
                        <summary>
                          {selected.joints.find((j) => j.id === k.joint)?.name} at {k.time.toFixed(2)} s
                        </summary>
                        {number("Key time", ["motions", i, "keys", ki, "time"], k.time, 0, m.duration, 0.05)}
                        {vector(
                          "Key rotation",
                          ["motions", i, "keys", ki, "rotation"],
                          k.rotation,
                          -3.14,
                          3.14,
                        )}
                        {vector(
                          "Key translation",
                          ["motions", i, "keys", ki, "translation"],
                          k.translation,
                          -10,
                          10,
                        )}
                        <button
                          type="button"
                          className="danger subtle"
                          onClick={() =>
                            set(
                              ["motions", i, "keys"],
                              m.keys.filter((_, n) => n !== ki),
                            )
                          }
                        >
                          Remove key
                        </button>
                      </details>
                    ))}
                  </details>
                ))}
                <p className="hint">
                  Poses use radians. Scrubbing analytic motion seeks directly; dynamic physics replays fixed
                  steps.
                </p>
              </section>
            )}
            {workspace === "Physics" && (
              <section>
                <h3>Transform ownership</h3>
                {select("Mode", ["physics", "mode"], selected.physics.mode, [
                  { id: "kinematic", name: "Animated / kinematic" },
                  { id: "dynamic", name: "Dynamic body" },
                  { id: "motor", name: "Motor driven" },
                ])}
                {number("Mass (kg)", ["physics", "mass"], selected.physics.mass, 0.1, 30, 0.1)}
                {number("Friction", ["physics", "friction"], selected.physics.friction, 0, 3)}
                {number("Restitution", ["physics", "restitution"], selected.physics.restitution, 0, 1)}
                <h3>Collision primitives</h3>
                {selected.physics.colliders?.map((c, i) => (
                  <details key={c.id} open={i === 0}>
                    <summary>
                      {c.shape}
                      <small>{c.id}</small>
                    </summary>
                    {vector("Collider offset", ["physics", "colliders", i, "position"], c.position, -10, 10)}
                    {vector(
                      "Collider rotation",
                      ["physics", "colliders", i, "rotation"],
                      c.rotation,
                      -3.14,
                      3.14,
                    )}
                    {c.shape === "box"
                      ? vector("Half extents", ["physics", "colliders", i, "size"], c.size, 0.01, 10)
                      : number("Collider radius", ["physics", "colliders", i, "radius"], c.radius, 0.01, 5)}
                    {c.shape === "capsule" &&
                      number("Half height", ["physics", "colliders", i, "halfHeight"], c.halfHeight, 0, 5)}
                    <button
                      type="button"
                      className="subtle danger"
                      onClick={() => {
                        const colliders = selected.physics.colliders?.filter((_, index) => index !== i);
                        set(["physics"], {
                          ...selected.physics,
                          colliders: colliders?.length ? colliders : undefined,
                        });
                      }}
                    >
                      Remove collider
                    </button>
                  </details>
                ))}
                {!selected.physics.colliders && (
                  <p className="hint">An automatic capsule currently encloses the generated surface.</p>
                )}
                <div className="button-row">
                  {(["sphere", "box", "capsule"] as const).map((shape) => (
                    <button
                      type="button"
                      key={shape}
                      onClick={() => {
                        const collider = {
                          id: `collider-${crypto.randomUUID().slice(0, 6)}`,
                          position: [0, 0, 0],
                          rotation: [0, 0, 0],
                          shape,
                          ...(shape === "box"
                            ? { size: [0.4, 0.8, 0.35] }
                            : shape === "sphere"
                              ? { radius: 0.4 }
                              : { radius: 0.35, halfHeight: 0.6 }),
                        };
                        set(["physics"], {
                          ...selected.physics,
                          colliders: [...(selected.physics.colliders ?? []), collider],
                        });
                      }}
                    >
                      + {shape}
                    </button>
                  ))}
                </div>

                <button
                  type="button"
                  onClick={() =>
                    act(async () => {
                      const p = studio.host?.characterPosition(selected.id);
                      if (!p) return;
                      await studio.host?.moveCharacter(selected.id, [p[0] + 3, p[1], p[2]]);
                      studio.patch({ playing: true });
                    })
                  }
                >
                  Move forward 3 m
                </button>
                <p className="hint">
                  The selected mode owns the body transform. Test contact on the world stage.
                </p>
              </section>
            )}
          </>
        );
      case "material":
        return (
          <>
            <section>
              <h3>Surface appearance</h3>
              {color("Base color", ["color"], selected.color)}
              {color("Variation color", ["secondary"], selected.secondary)}
              {number("Roughness", ["roughness"], selected.roughness, 0.04, 1)}
              {number("Metallic", ["metallic"], selected.metallic, 0, 1)}
              {select(
                "Pattern",
                ["pattern"],
                selected.pattern,
                ["solid", "noise", "stripes", "marble", "weave"].map((id) => ({ id, name: id })),
              )}
              {number("Pattern scale", ["scale"], selected.scale, 0.01, 30)}
              {number("Normal variation", ["normalStrength"], selected.normalStrength, 0, 1)}
            </section>
            <section>
              <h3>Shared definition</h3>
              <p className="hint">
                Edits affect every subject using this material. Make a local variant from a subject’s Surface
                panel for independent changes.
              </p>
            </section>
          </>
        );
      case "vegetation":
        return (
          <>
            <section>
              <h3>Growth</h3>
              {number("Seed", ["seed"], selected.seed, 0, 10000, 1)}
              {number("Height (m)", ["height"], selected.height, 0.5, 20, 0.1)}
              {number("Crown radius", ["radius"], selected.radius, 0.1, 8, 0.1)}
              {number("Branches", ["branches"], selected.branches, 3, 32, 1)}
              {number("Variation", ["variation"], selected.variation, 0, 1)}
            </section>
            <section>
              <h3>Wind response</h3>
              {number("Flexibility", ["windResponse"], selected.windResponse, 0, 2)}
              <p className="hint">
                Preview wind is independent of this authored response. Motion remains inside the compiled
                displacement envelope.
              </p>
            </section>
            {surfaceMaterial}
          </>
        );
      case "environment":
        return (
          <>
            <section>
              <h3>Sun & atmosphere</h3>
              {number("Sun elevation", ["sunElevation"], selected.sunElevation, -0.2, 1.57)}
              {number("Sun azimuth", ["sunAzimuth"], selected.sunAzimuth, -3.14, 3.14)}
              {number("Turbidity", ["turbidity"], selected.turbidity, 1, 10, 0.1)}
              {number("Fog density", ["fogDensity"], selected.fogDensity, 0, 0.05, 0.001)}
              {color("Zenith", ["skyColor"], selected.skyColor)}
              {color("Horizon", ["horizonColor"], selected.horizonColor)}
              {color("Ground bounce", ["groundColor"], selected.groundColor)}
            </section>
            <section>
              <h3>Wind</h3>
              {vector("Velocity", ["wind"], selected.wind, -10, 10)}
            </section>
          </>
        );
      case "lighting":
        return (
          <section>
            <h3>Lighting rig</h3>
            <div className="button-row">
              {(["point", "directional"] as const).map((type) => (
                <button
                  type="button"
                  key={type}
                  disabled={selected.lights.length >= 8}
                  onClick={() =>
                    set(
                      ["lights"],
                      [
                        ...selected.lights,
                        {
                          id: `light-${crypto.randomUUID().slice(0, 8)}`,
                          type,
                          position: [2, 4, 2],
                          color: [1, 1, 1],
                          intensity: type === "point" ? 8 : 1,
                        },
                      ],
                    )
                  }
                >
                  Add {type === "point" ? "point light" : "sun contribution"}
                </button>
              ))}
            </div>
            {number("Ambient", ["ambient"], selected.ambient, 0, 3)}
            {selected.lights.map((l, i) => (
              <details key={l.id} open>
                <summary>{l.type === "directional" ? "Sun contribution" : "Point light"}</summary>
                {number("Intensity", ["lights", i, "intensity"], l.intensity, 0, 20, 0.1)}
                {color("Light color", ["lights", i, "color"], l.color)}
                {l.type === "point" && vector("Position", ["lights", i, "position"], l.position)}
                <button
                  type="button"
                  className="subtle danger"
                  onClick={() =>
                    set(
                      ["lights"],
                      selected.lights.filter((light) => light.id !== l.id),
                    )
                  }
                >
                  Remove light
                </button>
              </details>
            ))}
            <p className="hint">
              Sun direction follows the sky’s solar state. Exposure is a separate preview control.
            </p>
          </section>
        );
      case "water":
        return (
          <>
            <section>
              <h3>Surface</h3>
              {number("Water level", ["level"], selected.level, -10, 10, 0.1)}
              {color("Water color", ["color"], selected.color)}
              {number("Roughness", ["roughness"], selected.roughness, 0.02, 1)}
            </section>
            <section>
              <h3>Wave components</h3>
              <button
                type="button"
                disabled={selected.waves.length >= 8}
                onClick={() =>
                  set(
                    ["waves"],
                    [...selected.waves, { amplitude: 0.1, wavelength: 6, speed: 1, direction: 0, phase: 0 }],
                  )
                }
              >
                Add wave component
              </button>
              {selected.waves.map((w, i) => (
                <details key={i} open={i === 0}>
                  <summary>
                    Wave {i + 1}
                    <small>{w.wavelength.toFixed(1)} m</small>
                  </summary>
                  {number("Amplitude", ["waves", i, "amplitude"], w.amplitude, 0, 3)}
                  {number("Wavelength", ["waves", i, "wavelength"], w.wavelength, 0.5, 100, 0.1)}
                  {number("Speed", ["waves", i, "speed"], w.speed, -10, 10, 0.1)}
                  {number("Direction", ["waves", i, "direction"], w.direction, -3.14, 3.14)}
                  {number("Phase", ["waves", i, "phase"], w.phase, -10, 10, 0.1)}
                  <button
                    type="button"
                    className="subtle danger"
                    onClick={() =>
                      set(
                        ["waves"],
                        selected.waves.filter((_, index) => index !== i),
                      )
                    }
                  >
                    Remove wave
                  </button>
                </details>
              ))}
              <p className="hint">
                Height, normal, and velocity queries use the same wave components as the visible surface.
              </p>
            </section>
          </>
        );
      case "terrain":
        return (
          <>
            <section>
              <h3>Terrain rules</h3>
              {number("Seed", ["seed"], selected.seed, 0, 10000, 1)}
              {number("Relief (m)", ["amplitude"], selected.amplitude, 0, 50, 0.1)}
              {number("Feature frequency", ["frequency"], selected.frequency, 0.001, 0.1, 0.001)}
              {number("Octaves", ["octaves"], selected.octaves, 1, 6, 1)}
              {number("Base height", ["baseHeight"], selected.baseHeight, -20, 20, 0.1)}
            </section>
            <section>
              <h3>
                Local interventions{" "}
                <button
                  type="button"
                  className="tiny"
                  aria-label="Add terrain intervention"
                  onClick={() =>
                    edit([
                      {
                        kind: "terrain.intervene",
                        target: selected.id,
                        intervention: {
                          id: `edit-${crypto.randomUUID().slice(0, 6)}`,
                          kind: "raise",
                          center: [preview.camera.target[0], preview.camera.target[2]],
                          radius: 10,
                          strength: 4,
                          targetHeight: 0,
                        },
                      },
                    ])
                  }
                >
                  <Icon name="plus" />
                </button>
              </h3>
              {selected.interventions.map((v, i) => (
                <details key={v.id} open={i === 1}>
                  <summary>
                    {v.kind}
                    <small>{v.radius} m</small>
                  </summary>
                  {select(
                    "Operation",
                    ["interventions", i, "kind"],
                    v.kind,
                    ["raise", "lower", "flatten", "valley", "clearing", "river"].map((id) => ({
                      id,
                      name: id,
                    })),
                  )}
                  {number("Radius", ["interventions", i, "radius"], v.radius, 1, 200, 1)}
                  {number("Strength", ["interventions", i, "strength"], v.strength, -20, 20, 0.1)}
                  {number(
                    "Target height",
                    ["interventions", i, "targetHeight"],
                    v.targetHeight,
                    -20,
                    20,
                    0.1,
                  )}
                  {number("Center X", ["interventions", i, "center", 0], v.center[0], -500, 500, 1)}
                  {number("Center Z", ["interventions", i, "center", 1], v.center[1], -500, 500, 1)}
                  <button
                    type="button"
                    className="subtle danger"
                    onClick={() =>
                      set(
                        ["interventions"],
                        selected.interventions.filter((edit) => edit.id !== v.id),
                      )
                    }
                  >
                    Remove intervention
                  </button>
                </details>
              ))}
            </section>
            {surfaceMaterial}
          </>
        );
      case "world":
        return (
          <>
            <section>
              <h3>World composition</h3>
              {(["terrain", "environment", "lighting"] as const).map((kind) => (
                <div key={kind}>
                  {select(
                    kind[0].toUpperCase() + kind.slice(1),
                    [kind],
                    selected[kind],
                    docs.filter((d) => d.kind === kind),
                  )}
                  <button
                    type="button"
                    className="subtle"
                    onClick={() => studio.authoring.select(selected[kind])}
                  >
                    Edit {kind}
                  </button>
                </div>
              ))}
              <label className="row-control">
                <span>Water</span>
                <select
                  aria-label="World water"
                  value={selected.water ?? ""}
                  onChange={(e) => set(["water"], e.target.value || undefined)}
                >
                  <option value="">None</option>
                  {docs
                    .filter((d) => d.kind === "water")
                    .map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.name}
                      </option>
                    ))}
                </select>
              </label>
              {docs
                .filter((d) => d.kind === "water" && d.id !== selected.water)
                .map((water) => (
                  <label className="row-control" key={water.id}>
                    <span>{water.name}</span>
                    <input
                      type="checkbox"
                      aria-label={`Include ${water.name}`}
                      checked={selected.waters?.includes(water.id) ?? false}
                      disabled={
                        !selected.waters?.includes(water.id) &&
                        new Set([selected.water, ...(selected.waters ?? [])].filter(Boolean)).size >= 8
                      }
                      onChange={(event) =>
                        set(
                          ["waters"],
                          event.target.checked
                            ? [...(selected.waters ?? []), water.id]
                            : selected.waters?.filter((id) => id !== water.id),
                        )
                      }
                    />
                  </label>
                ))}
            </section>
            <section>
              <h3>Procedural populations</h3>
              {selected.populations.map((p, i) => (
                <details key={p.id} open>
                  <summary>
                    {docs.find((d) => d.id === p.definition)?.name}
                    <small>Rule</small>
                  </summary>
                  {select(
                    "Population definition",
                    ["populations", i, "definition"],
                    p.definition,
                    docs.filter((d) => d.kind === "vegetation" || d.kind === "object"),
                  )}
                  {number("Maximum height", ["populations", i, "maxHeight"], p.maxHeight, -20, 200, 0.1)}
                  <button
                    type="button"
                    className="subtle danger"
                    onClick={() =>
                      set(
                        ["populations"],
                        selected.populations.filter((rule) => rule.id !== p.id),
                      )
                    }
                  >
                    Remove population
                  </button>
                  {number("Spacing (m)", ["populations", i, "spacing"], p.spacing, 2, 50, 1)}
                  {number("Density", ["populations", i, "density"], p.density, 0, 1)}
                  {number("Seed", ["populations", i, "seed"], p.seed, 0, 10000, 1)}
                  {number("Minimum height", ["populations", i, "minHeight"], p.minHeight, -20, 50, 0.1)}
                  {number("Maximum slope", ["populations", i, "maxSlope"], p.maxSlope, 0, 1)}
                </details>
              ))}
              <button
                type="button"
                onClick={() =>
                  edit([
                    {
                      kind: "world.placeForest",
                      target: selected.id,
                      rule: `forest-${crypto.randomUUID().slice(0, 6)}`,
                      definition: docs.find((d) => d.kind === "vegetation")?.id ?? "",
                      spacing: 15,
                      density: 0.5,
                      seed: 73,
                    },
                  ])
                }
                disabled={!docs.some((d) => d.kind === "vegetation")}
              >
                Place a forest
              </button>
            </section>
            <section>
              <h3>Placed instances</h3>
              {selected.instances.map((instance, i) => (
                <details key={instance.id}>
                  <summary>{docs.find((d) => d.id === instance.definition)?.name ?? instance.id}</summary>
                  {select(
                    "Instance definition",
                    ["instances", i, "definition"],
                    instance.definition,
                    docs.filter((d) => ["object", "vegetation", "character"].includes(d.kind)),
                  )}
                  {vector(
                    "Instance position",
                    ["instances", i, "position"],
                    instance.position,
                    -1000000,
                    1000000,
                  )}
                  {vector("Instance rotation", ["instances", i, "rotation"], instance.rotation, -6.29, 6.29)}
                  {number("Instance scale", ["instances", i, "scale"], instance.scale, 0.01, 100)}
                  <label className="row-control">
                    <input
                      type="checkbox"
                      checked={!!instance.grounding}
                      onChange={(event) =>
                        set(
                          ["instances"],
                          selected.instances.map((item, index) => {
                            if (index !== i) return item;
                            if (event.target.checked) return { ...item, grounding: { offset: 0 } };
                            const { grounding: _grounding, ...ungrounded } = item;
                            return ungrounded;
                          }),
                        )
                      }
                    />
                    Follow terrain
                  </label>
                  {instance.grounding &&
                    number(
                      "Height above terrain",
                      ["instances", i, "grounding", "offset"],
                      instance.grounding.offset,
                      -20,
                      20,
                    )}
                  <button
                    type="button"
                    className="subtle danger"
                    onClick={() =>
                      set(
                        ["instances"],
                        selected.instances.filter((item) => item.id !== instance.id),
                      )
                    }
                  >
                    Remove instance
                  </button>
                </details>
              ))}
              <button
                type="button"
                disabled={!docs.some((d) => ["object", "vegetation", "character"].includes(d.kind))}
                onClick={() =>
                  set(
                    ["instances"],
                    [
                      ...selected.instances,
                      {
                        id: `instance-${crypto.randomUUID().slice(0, 8)}`,
                        definition: docs.find((d) => ["object", "vegetation", "character"].includes(d.kind))
                          ?.id,
                        position: [...preview.camera.target],
                        rotation: [0, 0, 0],
                        scale: 1,
                      },
                    ],
                  )
                }
              >
                Place an instance
              </button>
            </section>
            <section>
              <h3>Travel</h3>
              <p className="hint">
                Prepare a new region, then move the camera. Physical ground remains explicit and bounded.
              </p>
              <div className="button-row">
                <button
                  type="button"
                  onClick={() =>
                    act(() => studio.travel(preview.camera.target[0] + 128, preview.camera.target[2]))
                  }
                >
                  East 128 m
                </button>
                <button type="button" onClick={() => act(() => studio.travel(0, 0))}>
                  Return home
                </button>
              </div>
            </section>
          </>
        );
      case "stage":
        return (
          <section>
            <h3>Reference stage</h3>
            {select(
              "Stage environment",
              ["environment"],
              selected.environment,
              docs.filter((d) => d.kind === "environment"),
            )}
            {select(
              "Stage lighting",
              ["lighting"],
              selected.lighting,
              docs.filter((d) => d.kind === "lighting"),
            )}
            <details open>
              <summary>Reference subjects</summary>
              {docs
                .filter((d) => ["object", "character", "vegetation"].includes(d.kind))
                .map((d) => (
                  <label className="row-control" key={d.id}>
                    <span>{d.name}</span>
                    <input
                      type="checkbox"
                      aria-label={`Stage subject ${d.name}`}
                      checked={selected.subjects.includes(d.id)}
                      onChange={(e) =>
                        set(
                          ["subjects"],
                          e.target.checked
                            ? [...selected.subjects, d.id]
                            : selected.subjects.filter((id) => id !== d.id),
                        )
                      }
                    />
                  </label>
                ))}
            </details>
            {number("Exposure", ["exposure"], selected.exposure, 0.1, 8)}
            <label className="row-control">
              <span>Ground plane</span>
              <input
                type="checkbox"
                checked={selected.ground}
                onChange={(e) => set(["ground"], e.target.checked)}
              />
            </label>
            {vector("Camera", ["camera", "position"], selected.camera.position)}
            {vector("Look at", ["camera", "target"], selected.camera.target)}
          </section>
        );
    }
  };
  return (
    <div className="studio-shell">
      <header className="app-header">
        <div className="brand">
          <svg viewBox="0 0 32 32" aria-hidden="true">
            <path d="M3 9 9 24 16 9 23 24 29 9" />
          </svg>
          <strong>
            Wrela<span>Studio</span>
          </strong>
        </div>
        <div className="project-title">
          {source.project.name}
          <span
            className={source.savedRevision === source.revision ? "saved" : "unsaved"}
            title={source.savedRevision === source.revision ? "Saved" : "Unsaved changes"}
          />
        </div>
        <button type="button" className="search-button" onClick={() => setPalette(true)}>
          <Icon name="search" />
          <span>Find a command</span>
          <kbd>⌘ K</kbd>
        </button>
        <div className="header-actions">
          <button
            type="button"
            title="Undo (⌘Z)"
            aria-label="Undo"
            disabled={!source.canUndo}
            onClick={() => studio.authoring.undo()}
          >
            <Icon name="undo" />
          </button>
          <button
            type="button"
            title="Redo (⌘⇧Z)"
            aria-label="Redo"
            disabled={!source.canRedo}
            onClick={() => studio.authoring.redo()}
          >
            <Icon name="redo" />
          </button>
          <span className="divider" />
          <RecoveryDrafts
            drafts={preview.recoveryDrafts}
            restore={(writer) => act(() => studio.restoreDraft(writer), "Recovery draft opened")}
          />
          <button type="button" aria-label="Play Winter valley" onClick={() => act(() => studio.playGame())}>
            <Icon name="play" />
            <span>Play</span>
          </button>
          <button
            type="button"
            aria-label="Save project"
            onClick={() => act(() => studio.save(), "Project saved")}
          >
            <Icon name="save" />
            <span>Save</span>
          </button>
          <button
            type="button"
            className="primary"
            onClick={() =>
              act(
                async () => downloadBlob(await studio.exportPlayer(), `${source.project.id}.html`),
                "Standalone player exported",
              )
            }
          >
            <Icon name="export" />
            <span>Export</span>
          </button>
        </div>
      </header>
      <aside className="project-panel">
        <div className="panel-heading">
          <h2>Project</h2>
          <button type="button" aria-label="Create definition" className="tiny" onClick={() => setAdd(!add)}>
            <Icon name="plus" />
          </button>
        </div>
        <div className="project-search">
          <Icon name="search" />
          <input
            aria-label="Filter definitions"
            placeholder="Filter definitions…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <nav aria-label="Project definitions">
          {[
            ["World", ["world", "terrain"]],
            ["Subjects", ["character", "object", "vegetation"]],
            ["Look & atmosphere", ["material", "lighting", "environment", "water"]],
            ["Stages", ["stage"]],
          ].map(([title, kinds]) => (
            <div key={title as string} className="document-group">
              <h3>{title as string}</h3>
              {docs
                .filter(
                  (d) =>
                    (kinds as string[]).includes(d.kind) &&
                    d.name.toLowerCase().includes(query.toLowerCase()),
                )
                .map((d) => (
                  <button
                    type="button"
                    key={d.id}
                    data-document={d.id}
                    className={`document-item ${selected.id === d.id ? "selected" : ""}`}
                    onClick={() => studio.authoring.select(d.id)}
                  >
                    <span className={`kind-icon ${d.kind}`}>{symbols[d.kind]}</span>
                    <span>{d.name}</span>
                    {selected.id === d.id && <span className="selection-dot" />}
                  </button>
                ))}
            </div>
          ))}
        </nav>
        <div className="project-footer">
          <label className="creature-block-label" htmlFor="creature-study">
            Open creature study
          </label>
          <select
            id="creature-study"
            aria-label="Open creature study"
            value=""
            disabled={busy}
            onChange={(event) => {
              const fixture = creatureFixtureCatalog.find((item) => item.id === event.target.value);
              if (fixture)
                act(
                  () => studio.creature.openFixture(fixture.id),
                  `${fixture.name} opened; previous project preserved in recovery`,
                );
            }}
          >
            <option value="">Choose a creature…</option>
            {creatureFixtureCatalog.map((fixture) => (
              <option key={fixture.id} value={fixture.id}>
                {fixture.name}
              </option>
            ))}
          </select>
          {preview.returnProjectName && (
            <button type="button" onClick={() => act(() => studio.creature.returnToProject())}>
              Return to {preview.returnProjectName}
            </button>
          )}
          <button type="button" onClick={() => setAgent(true)}>
            <span className="agent-mark">⌘</span>Agent operations <span>↗</span>
          </button>
          <p>Local project · version 1</p>
        </div>
      </aside>
      <main className="stage-panel">
        <div className="viewport-toolbar">
          <div className="breadcrumb">
            <span>{symbols[selected.kind]}</span>
            <strong>{selected.name}</strong>
            <span className="quiet">/ {selected.kind}</span>
          </div>
          <div className="toolbar-controls">
            {preview.stage === "studio" && "field" in selected && (
              <button
                type="button"
                aria-label="Toggle authoring handles"
                className={handles ? "active" : ""}
                onClick={() => {
                  setHandles(!handles);
                  studio.patch({ playing: false });
                }}
              >
                Handles
              </button>
            )}
            <select
              aria-label="Preview stage"
              value={preview.stage}
              onChange={(e) => studio.patch({ stage: e.target.value as "world" | "studio" })}
            >
              <option value="world">World stage</option>
              <option value="studio">Neutral studio</option>
            </select>
            <select
              aria-label="View channel"
              value={preview.mode}
              onChange={(e) => studio.patch({ mode: e.target.value as any })}
            >
              {VIEW_MODES.map((m) => (
                <option key={m} value={m}>
                  {m === "beauty"
                    ? "Shaded"
                    : m === "lod"
                      ? "Level of detail"
                      : m[0].toUpperCase() + m.slice(1)}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="viewport" onContextMenu={(e) => e.preventDefault()}>
          <canvas
            id="viewport"
            ref={canvas}
            aria-label="Interactive 3D viewport. Drag to orbit; shift drag to pan; scroll to zoom."
            tabIndex={0}
            onPointerDown={(e) => {
              const el = e.currentTarget;
              el.setPointerCapture(e.pointerId);
              el.dataset.drag = "true";
              el.dataset.movement = "0";
              el.dataset.x = String(e.clientX);
              el.dataset.y = String(e.clientY);
            }}
            onPointerMove={(e) => {
              const el = e.currentTarget;
              if (el.dataset.drag !== "true") return;
              const dx = e.clientX - Number(el.dataset.x),
                dy = e.clientY - Number(el.dataset.y);
              el.dataset.x = String(e.clientX);
              el.dataset.y = String(e.clientY);
              el.dataset.movement = String(Number(el.dataset.movement) + Math.abs(dx) + Math.abs(dy));
              if (e.shiftKey || e.buttons === 2) studio.pan(dx, dy);
              else studio.orbit(dx, dy);
            }}
            onPointerUp={(e) => {
              e.currentTarget.dataset.drag = "false";
              if (Number(e.currentTarget.dataset.movement) < 4) {
                const r = e.currentTarget.getBoundingClientRect();
                const hit = studio.pick(e.clientX - r.left, e.clientY - r.top, r.width, r.height);
                if (hit?.nodeId) setTimeout(() => setNodeId(hit.nodeId ?? ""), 0);
              }
            }}
            onWheel={(e) => studio.zoom(e.deltaY)}
          />
          <canvas ref={overlayCanvas} className="diagnostic-overlay" aria-hidden="true" tabIndex={-1} />
          {preview.encounter && <CreatureEncounterPanel controller={studio} encounter={preview.encounter} />}
          {handles && preview.stage === "studio" && "field" in selected && canvas.current && (
            <AuthoringHandles
              studio={studio}
              document={selected}
              workspace={workspace}
              width={canvas.current.clientWidth}
              height={canvas.current.clientHeight}
              selectedNode={nodeId}
              onSelect={setNodeId}
              edit={edit}
            />
          )}
          {(busy || preview.status === "failed") && (
            <div className={`viewport-message ${preview.status === "failed" ? "failure" : ""}`}>
              <div className="small-mark">W</div>
              <h2>{preview.status === "failed" ? "Preview needs attention" : "Preparing your world"}</h2>
              <p>{preview.message}</p>
              {preview.status === "failed" && (
                <button type="button" onClick={() => studio.patch({ quality: preview.quality })}>
                  Retry preview
                </button>
              )}
            </div>
          )}
          <div className="viewport-top">
            <span className={`status-pill ${preview.status}`}>
              <i />
              {preview.status === "current"
                ? "Live preview"
                : preview.status === "compiling"
                  ? "Updating preview"
                  : preview.status === "failed"
                    ? "Preview needs attention"
                    : "Starting"}
            </span>
            <span>{preview.quality === "interactive" ? "Interactive quality" : "Review quality"}</span>
          </div>
          <div className="viewport-bottom">
            <span>
              Drag to orbit <b>·</b> Shift drag to pan <b>·</b> Scroll to zoom
            </span>
            <div>
              <button
                type="button"
                title="Reset view (F)"
                aria-label="Reset view"
                onClick={() => studio.patch({ stage: preview.stage })}
              >
                <Icon name="home" />
              </button>
              <button
                type="button"
                title="Capture image"
                aria-label="Capture image"
                onClick={() =>
                  act(async () => {
                    const result = await studio.capture();
                    downloadBlob(result.blob, "wrela-capture.png");
                    downloadBlob(
                      new Blob([JSON.stringify(result.metadata, null, 2)], { type: "application/json" }),
                      "wrela-capture.json",
                    );
                  }, "Capture saved")
                }
              >
                <Icon name="camera" />
              </button>
            </div>
          </div>
          <div className="axis-widget" aria-hidden="true">
            <span>Y</span>
            <i />
            <b>X</b>
            <em>Z</em>
          </div>
        </div>
        <div className="timeline">
          <div className="transport">
            <button
              type="button"
              aria-label={preview.playing ? "Pause playback" : "Play preview"}
              className={preview.playing ? "active" : ""}
              onClick={() => studio.patch({ playing: !preview.playing })}
            >
              <Icon name={preview.playing ? "pause" : "play"} />
            </button>
            <button type="button" aria-label="Rewind" onClick={() => act(() => studio.seek(0))}>
              ↤
            </button>
            <output>
              {preview.time.toFixed(2)}
              <small>s</small>
            </output>
            <span>{selected.kind === "character" ? "Motion timeline" : "Stage clock"}</span>
            <output className="loop-indicator">{preview.seeking ? preview.message : "60 Hz"}</output>
          </div>
          <div className="timeline-track">
            <div className="time-ruler">
              {[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].map((n) => (
                <span key={n}>{Number(((n * timelineDuration) / 12).toFixed(2))}s</span>
              ))}
            </div>
            <input
              aria-label="Timeline"
              type="range"
              min="0"
              max={timelineDuration}
              step="0.0166667"
              value={
                preview.playing ? preview.time % timelineDuration : Math.min(preview.time, timelineDuration)
              }
              onChange={(e) => act(() => studio.seek(Number(e.target.value)))}
            />
            <div className="timeline-lane">
              <span>
                {selected.kind === "character"
                  ? "Pose & motion"
                  : selected.kind === "water"
                    ? "Analytic waves"
                    : "Environment"}
              </span>
              <div className="clip-bar">
                {timelineMotion?.name ??
                  (selected.kind === "character" ? "No motion selected" : "Continuous evaluation")}
                {timelineMotion &&
                  [...new Set(timelineMotion.keys.map((key) => key.time))].map((time) => (
                    <i
                      key={time}
                      title={`Key at ${time.toFixed(2)} seconds`}
                      style={{ left: `${Math.min(99, (time / timelineDuration) * 100)}%` }}
                    />
                  ))}
              </div>
            </div>
          </div>
        </div>
      </main>
      <aside className="inspector">
        <div className="panel-heading">
          <h2>Inspector</h2>
          <span className="type-badge">{selected.kind}</span>
        </div>
        <div className="subject-heading">
          <span className={`subject-icon ${selected.kind}`}>{symbols[selected.kind]}</span>
          <div>
            <input
              aria-label="Definition name"
              value={selected.name}
              onChange={(e) => edit([{ kind: "document.rename", target: selected.id, name: e.target.value }])}
            />
            <small>{selected.id}</small>
          </div>
        </div>
        <div className="inspector-scroll">
          <DomainAuthoringPanels
            key={selected.id}
            document={selected}
            project={source.project}
            onChange={(path, value, label) =>
              edit(
                [{ kind: "document.set", target: selected.id, path, value }],
                label ?? `Edit ${path.join(" / ")}`,
              )
            }
            onApply={edit}
            onSeek={(seconds) => act(() => studio.seek(seconds))}
            onSeekMotion={(motion, seconds) =>
              act(async () => {
                setMotionId(motion);
                await motionPreview.current.seek(selected.id, motion, seconds);
              })
            }
            onTransition={(from, to, at, elapsed, playing) =>
              act(async () => {
                setMotionId(to);
                await motionPreview.current.transition(selected.id, from, to, at, elapsed, playing);
              })
            }
            onPlay={(motion) =>
              act(async () => {
                if (motion && selected.kind === "character") {
                  setMotionId(motion);
                  await motionPreview.current.seek(selected.id, motion, 0, true);
                } else {
                  await studio.seek(0);
                  studio.patch({ playing: true });
                }
              })
            }
            onReview={(scenarioId, cameraId, mode) =>
              act(async () => {
                if (selected.kind !== "character") return;
                await studio.creature.playScenario(selected.id, scenarioId);
                const camera = selected.creature?.reviewScenarios
                  .find((s) => s.id === scenarioId)
                  ?.cameras.find((c) => c.id === cameraId);
                studio.patch({
                  mode: mode === "lit" ? "beauty" : "silhouette",
                  ...(camera ? { camera: { ...camera, fov: 48 } } : {}),
                });
              })
            }
            onPreview={(options) =>
              act(async () => {
                const camera = studio.getSnapshot().camera;
                if (options.distance !== undefined) {
                  const direction = normalize(sub(camera.position, camera.target));
                  studio.patch({
                    camera: {
                      ...camera,
                      position: camera.target.map((v, i) => v + direction[i] * options.distance!) as Vec3,
                    },
                  });
                }
                if (options.mode) studio.patch({ mode: options.mode === "lit" ? "beauty" : "silhouette" });
                if (options.sunElevation !== undefined)
                  studio.patch({ reviewSunElevation: options.sunElevation });
                if (options.sunAzimuth !== undefined) studio.patch({ reviewSunAzimuth: options.sunAzimuth });
                if (options.time !== undefined) await studio.seek(options.time);
              })
            }
          />
          {domainInspector()}
          {studio.host?.world && (
            <section>
              <h3>Runtime changes</h3>
              {preview.pickedInstance && (
                <>
                  <p className="hint">Selected occurrence: {preview.pickedInstance}</p>
                  <div className="button-row">
                    <button
                      type="button"
                      onClick={() =>
                        act(
                          () => studio.removeInstance(preview.pickedInstance ?? ""),
                          "Occurrence hidden in runtime",
                        )
                      }
                    >
                      Hide occurrence
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        act(
                          () => studio.removeInstance(preview.pickedInstance ?? "", false),
                          "Occurrence restored",
                        )
                      }
                    >
                      Restore occurrence
                    </button>
                  </div>
                </>
              )}
              <div className="button-row">
                <button
                  type="button"
                  onClick={() => act(() => studio.saveRuntime(), "Runtime changes saved")}
                >
                  Save runtime
                </button>
                <button type="button" onClick={() => act(() => studio.restoreRuntime(), "Runtime restored")}>
                  Restore runtime
                </button>
              </div>
              <div className="button-row">
                <button
                  type="button"
                  onClick={() =>
                    downloadBlob(
                      new Blob([JSON.stringify(studio.host?.saveRuntime(), null, 2)], {
                        type: "application/json",
                      }),
                      "world-save.json",
                    )
                  }
                >
                  Export runtime save
                </button>
                <button type="button" onClick={() => runtimeInput.current?.click()}>
                  Import runtime save
                </button>
                <button
                  type="button"
                  onClick={() => act(() => studio.resetRuntime(), "Runtime changes reset")}
                >
                  Reset runtime
                </button>
              </div>
              <p className="hint">
                Changes to live occurrences are stored separately from authored rules and exported snapshots.
              </p>
            </section>
          )}
          <section className="preview-settings">
            <h3>Preview conditions</h3>
            <label className="row-control">
              <span>Reference stage</span>
              <select
                aria-label="Reference stage"
                value={preview.stageId ?? ""}
                onChange={(e) => studio.patch({ stage: "studio", stageId: e.target.value })}
              >
                <option value="">Default studio</option>
                {docs
                  .filter((d) => d.kind === "stage")
                  .map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
              </select>
            </label>
            <NumberControl
              onGestureEnd={(id) => studio.authoring.endGesture(id)}
              label="Exposure"
              value={preview.exposure}
              min={0.1}
              max={4}
              step={0.05}
              onChange={(v) => studio.patch({ exposure: v })}
            />
            <NumberControl
              onGestureEnd={(id) => studio.authoring.endGesture(id)}
              label="Wind multiplier"
              value={preview.wind}
              min={0}
              max={4}
              step={0.1}
              onChange={(v) => studio.patch({ wind: v })}
            />
            <label className="row-control">
              <span>Quality</span>
              <select
                aria-label="Preview quality"
                value={preview.quality}
                onChange={(e) => studio.patch({ quality: e.target.value as any })}
              >
                <option value="interactive">Interactive</option>
                <option value="review">Review</option>
                <option value="export">Export</option>
              </select>
            </label>
            {(["rig", "colliders"] as const).map((overlay) => (
              <label className="row-control" key={overlay}>
                <span>{overlay === "rig" ? "Rig overlay" : "Collider overlay"}</span>
                <input
                  type="checkbox"
                  aria-label={`${overlay} overlay`}
                  checked={preview.overlays.includes(overlay)}
                  onChange={(e) =>
                    studio.patch({
                      overlays: e.target.checked
                        ? [...preview.overlays, overlay]
                        : preview.overlays.filter((item) => item !== overlay),
                    })
                  }
                />
              </label>
            ))}
            <p className="hint">
              Preview conditions do not change the subject. Diagnostic overlays are included in captures.
            </p>
          </section>
        </div>
      </aside>
      <footer className="status-bar">
        <button type="button" onClick={() => setDiagnostics(!diagnostics)}>
          <span className={preview.diagnostics.length ? "warning-dot" : "health-dot"} />
          {preview.diagnostics.length ? `${preview.diagnostics.length} diagnostics` : "Diagnostics"}
          <span className="quiet">⌃</span>
        </button>
        <span>
          {source.savedRevision === source.revision ? "All changes saved" : "Unsaved source"}
          <b>·</b>Revision {source.revision}
          <b>·</b>
          {preview.status === "failed"
            ? "Preview stale"
            : preview.seeking
              ? preview.message
              : preview.revision === source.revision
                ? "Preview current"
                : "Preview updating"}
        </span>
        <span className="performance">
          {preview.measurements
            ? `${Math.round(preview.measurements.triangles / 1000)}k triangles · ${preview.measurements.cpuMs.toFixed(1)} ms CPU · ${(preview.measurements.gpuBytes / 1048576).toFixed(1)} MB GPU`
            : "WebGPU initializing"}
        </span>
      </footer>
      {(error || notice) && (
        <div role={error ? "alert" : "status"} className={`toast ${error ? "error" : ""}`}>
          <span>{error || notice}</span>
          <button
            type="button"
            aria-label="Dismiss message"
            onClick={() => {
              setError("");
              setNotice("");
            }}
          >
            <Icon name="close" />
          </button>
        </div>
      )}
      {diagnostics && (
        <div className="diagnostics-drawer">
          <div className="panel-heading">
            <h2>Diagnostics & measurements</h2>
            <button type="button" aria-label="Close diagnostics" onClick={() => setDiagnostics(false)}>
              <Icon name="close" />
            </button>
          </div>
          {preview.diagnostics.length ? (
            preview.diagnostics.map((d, i) => (
              <p key={i} className="diagnostic-error">
                {d.document && `${d.document}: `}
                {d.message}
              </p>
            ))
          ) : (
            <p>No diagnostics for the current preview.</p>
          )}
          <pre>
            {JSON.stringify({ renderer: preview.measurements, world: studio.host?.world?.metrics }, null, 2)}
          </pre>
        </div>
      )}
      {add && (
        <div className="popover create-menu">
          <h3>Create definition</h3>
          {(
            [
              "object",
              "character",
              "vegetation",
              "material",
              "lighting",
              "environment",
              "water",
              "terrain",
              "world",
              "stage",
            ] as const
          ).map((kind) => (
            <button type="button" key={kind} onClick={() => create(kind)}>
              <span>{symbols[kind]}</span>
              {kind[0].toUpperCase() + kind.slice(1)}
            </button>
          ))}
        </div>
      )}
      {palette && (
        <div className="modal-backdrop" onClick={() => setPalette(false)}>
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Command palette"
            className="command-dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <h2>Commands</h2>
            {[
              ["Save project", "⌘ S", () => act(() => studio.save(), "Project saved")],
              ["Reopen saved project", "", () => act(() => studio.reopen(), "Saved project reopened")],
              ["Import project JSON", "", () => importInput.current?.click()],
              [
                "Export project JSON",
                "",
                () =>
                  downloadBlob(
                    new Blob([studio.authoring.export()], { type: "application/json" }),
                    `${source.project.id}.json`,
                  ),
              ],
              [
                "Export standalone player",
                "",
                () => act(async () => downloadBlob(await studio.exportPlayer(), `${source.project.id}.html`)),
              ],
              ["Agent operations", "", () => setAgent(true)],
              ["Undo", "⌘ Z", () => studio.authoring.undo()],
              ["Redo", "⌘ ⇧ Z", () => studio.authoring.redo()],
            ].map(([label, shortcut, fn]) => (
              <button
                type="button"
                key={label as string}
                onClick={() => {
                  (fn as () => void)();
                  setPalette(false);
                }}
              >
                <span>{label as string}</span>
                <kbd>{shortcut as string}</kbd>
              </button>
            ))}
          </div>
        </div>
      )}
      {agent && (
        <div className="modal-backdrop" onClick={() => setAgent(false)}>
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Agent operations"
            className="agent-dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="panel-heading">
              <h2>Agent operations</h2>
              <button type="button" aria-label="Close agent operations" onClick={() => setAgent(false)}>
                <Icon name="close" />
              </button>
            </div>
            <p>Humans and agents use the same validated operations. A batch commits as one undo step.</p>
            <AuthoringWorkPanel operations={commands} />
            <div className="api-line">
              window.wrela.discover() <span>Revision {source.revision}</span>
            </div>
            <label htmlFor="agent-commands">Operations JSON</label>
            <textarea
              id="agent-commands"
              spellCheck={false}
              value={commands}
              onChange={(e) => setCommands(e.target.value)}
              placeholder={
                '[{"kind":"terrain.widenValley","target":"valley-terrain","intervention":"river-valley","width":80}]'
              }
            />
            <div className="button-row">
              <button
                type="button"
                onClick={() =>
                  setCommands(
                    JSON.stringify(
                      [
                        {
                          kind: "terrain.widenValley",
                          target: "valley-terrain",
                          intervention: "river-valley",
                          width: 80,
                        },
                        {
                          kind: "world.placeForest",
                          target: "winter-valley",
                          rule: "conifer-line",
                          definition: docs.find((d) => d.kind === "vegetation")?.id ?? "",
                          spacing: 10,
                          density: 0.65,
                          seed: 42,
                        },
                      ],
                      null,
                      2,
                    ),
                  )
                }
              >
                Valley & forest example
              </button>
              <button
                type="button"
                className="primary"
                onClick={() =>
                  act(
                    () =>
                      studio.apply({
                        expectedRevision: source.revision,
                        operations: JSON.parse(commands),
                        label: "Agent batch",
                      }),
                    "Agent batch applied",
                  )
                }
              >
                Apply batch
              </button>
            </div>
            <details>
              <summary>Available operations</summary>
              <pre>{JSON.stringify(studio.authoring.discover().operations, null, 2)}</pre>
            </details>
          </div>
        </div>
      )}
      <input
        type="file"
        accept="application/json,.json"
        ref={runtimeInput}
        hidden
        aria-label="Import runtime save"
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = "";
          if (file)
            act(async () => {
              if (file.size > 8 * 1024 * 1024) throw Error("Runtime save exceeds 8 MiB");
              await studio.loadRuntime(JSON.parse(await file.text()));
            }, "Runtime imported");
        }}
      />
      <input
        hidden
        ref={importInput}
        aria-label="Import project file"
        type="file"
        accept=".json,application/json"
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file)
            act(async () => {
              if (file.size > 8 * 1024 * 1024) throw Error("Project exceeds 8 MiB");
              studio.authoring.replace(importProject(await file.text()));
            }, "Project imported");
          e.target.value = "";
        }}
      />
    </div>
  );
}
const rootElement = document.getElementById("root");
if (!rootElement) throw Error("Studio root is missing");
const root = createRoot(rootElement);
root.render(<App />);
const hot = (import.meta as ImportMeta & { hot?: { dispose(callback: () => void): void } }).hot;
if (hot)
  hot.dispose(() => {
    root.unmount();
  });
