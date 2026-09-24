import {
  expeditionLantern,
  hangingBell,
  materializeWorkProposal,
  type ToolkitEntry,
  type VisualRepair,
  type WorkSession,
} from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { useEffect, useState } from "react";
import { studio } from "./controller";

export function HeroAuthoringPanel({
  work,
  busy,
  run,
}: {
  work: WorkSession;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
}) {
  const [family, setFamily] = useState("lantern"),
    [stage, setStage] = useState<"blockout" | "construction" | "surface" | "production">("blockout"),
    [age, setAge] = useState(0.5);
  const [base, setBase] = useState(""),
    [part, setPart] = useState(""),
    [amount, setAmount] = useState(0.6),
    [repair, setRepair] = useState("grain"),
    [reason, setReason] = useState("");
  const [recipes, setRecipes] = useState<ToolkitEntry[]>([]);
  useEffect(() => {
    let active = true;
    void studio.work.store
      .toolkit("creation")
      .then((rows) => {
        if (active) setRecipes(rows);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [work]);
  const project = base ? materializeWorkProposal(work, base) : work.baseline;
  const objects = project.documents.filter((d) => d.kind === "object" && d.assembly),
    object = objects.at(-1);
  return (
    <details>
      <summary>Create and refine a hero asset</summary>
      <p>
        Build editable construction from a starting design, then branch alternatives and keep the decisions
        you like.
      </p>
      <label>
        Starting construction{" "}
        <select value={family} disabled={busy} onChange={(e) => setFamily(e.target.value)}>
          <option value="lantern">Expedition lantern</option>
          <option value="bell">Hanging bell</option>
        </select>
      </label>
      <label>
        Review stage{" "}
        <select value={stage} disabled={busy} onChange={(e) => setStage(e.target.value as typeof stage)}>
          {["blockout", "construction", "surface", "production"].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
      </label>
      <label>
        Wear {age.toFixed(2)}
        <input
          type="range"
          min="0"
          max="1"
          step=".05"
          value={age}
          disabled={busy}
          onChange={(e) => setAge(Number(e.target.value))}
        />
      </label>
      <button
        type="button"
        disabled={busy || work.proposals.length > 0}
        onClick={() =>
          void run(() => {
            const design = family === "lantern" ? expeditionLantern() : hangingBell();
            design.id = `hero-${crypto.randomUUID().slice(0, 8)}`;
            design.stage = stage;
            design.age = age;
            return studio.work.fast.hero(work.id, {
              expectedKey: contentKey(work),
              id: `creation-${crypto.randomUUID().slice(0, 8)}`,
              designs: [design],
            });
          })
        }
      >
        Construct and review
      </button>
      <p>Use a new work session for a new construction. Existing alternatives remain available below.</p>
      {recipes.length > 0 && (
        <div>
          <strong>Saved construction</strong>
          {recipes.map((recipe) => (
            <p key={`${recipe.id}@${recipe.revision}`}>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(() => studio.work.fast.reuseCreation(work.id, recipe.id, recipe.revision))
                }
              >
                {recipe.title}
              </button>{" "}
              · revision {recipe.revision} · {recipe.status}
            </p>
          ))}
        </div>
      )}
      <label>
        Refine from{" "}
        <select
          value={base}
          disabled={busy}
          onChange={(e) => {
            setBase(e.target.value);
            setPart("");
          }}
        >
          <option value="">Original source</option>
          {work.proposals.map((p) => (
            <option key={p.id} value={p.id}>
              {p.id}
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        disabled={busy || !base}
        onClick={() => void run(() => studio.work.fast.advanceHero(work.id, base, stage, age))}
      >
        Branch construction at selected stage
      </button>
      {object?.kind === "object" && object.assembly && (
        <>
          <label>
            Part{" "}
            <select value={part} disabled={busy} onChange={(e) => setPart(e.target.value)}>
              <option value="">Choose a part</option>
              {object.assembly.parts.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Repair{" "}
            <select value={repair} disabled={busy} onChange={(e) => setRepair(e.target.value)}>
              <option value="grain">Vary timber origin</option>
              <option value="wear">Adjust corner wear</option>
            </select>
          </label>
          <label>
            Amount {amount.toFixed(2)}
            <input
              type="range"
              min="0"
              max="1"
              step=".05"
              value={amount}
              onChange={(e) => setAmount(Number(e.target.value))}
            />
          </label>
          <label>
            What should improve?
            <input
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Describe the visual issue"
            />
          </label>
          <button
            type="button"
            disabled={busy || !part}
            onClick={() =>
              void run(() =>
                studio.work.fast.iterate(work.id, {
                  expectedKey: contentKey(work),
                  proposal: `repair-${crypto.randomUUID().slice(0, 8)}`,
                  target: object.id,
                  baseProposal: base || undefined,
                  repair: {
                    kind: repair === "grain" ? "timber.grain-origin" : "assembly.edge-wear",
                    target: object.id,
                    parts: [part],
                    amount,
                  } as VisualRepair,
                  critique: reason,
                  pins: [{ target: object.id, path: ["name"], reason: "Keep asset identity" }],
                }),
              )
            }
          >
            Branch and review repair
          </button>
        </>
      )}
    </details>
  );
}
