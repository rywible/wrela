import {
  alpineConiferSchema,
  type BotanicalGrowthState,
  botanicalDevelopmentSchema,
  botanicalSpeciesTraits,
  contentKey,
  type VegetationDefinition,
} from "@wrela/model";

import { useEffect, useRef, useState } from "react";
import { CompilerClient } from "./compiler-client";
import type { DomainPanelProps } from "./domain-authoring";

/** All controls use the existing source transaction path, including undo, save, and worker recompilation. */
export function VegetationGrowthPanel({
  document,
  onChange,
}: Pick<DomainPanelProps<VegetationDefinition>, "document" | "onChange">) {
  const botanical = document.botanical;
  const development = botanical?.development;
  const compiler = useRef<CompilerClient | null>(null);
  const [inspection, inspect] = useState<{ key: string; state?: BotanicalGrowthState; error?: string }>({
    key: "",
  });
  const growthKey = development ? contentKey([document.seed, development]) : "";
  useEffect(() => {
    const client = new CompilerClient();
    compiler.current = client;
    return () => {
      client.dispose();
      compiler.current = null;
    };
  }, []);
  useEffect(() => {
    const client = compiler.current;
    if (!development || !client) return;
    let active = true;
    // Coalesce slider input before bounded worker inspection; never simulate in React rendering.
    const timer = setTimeout(() => {
      void client.inspectVegetation(document).then(
        (state) => {
          if (active) inspect({ key: growthKey, state });
        },
        (error) => {
          if (active) inspect({ key: growthKey, error: String(error) });
        },
      );
    }, 60);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [document, development, growthKey]);
  const state = inspection.key === growthKey ? inspection.state : undefined;
  const [selected, select] = useState("");
  if (!botanical || !["pine", "birch"].includes(botanical.species)) return null;
  const set = (path: (string | number)[], value: unknown, label: string) =>
    onChange(["botanical", "development", ...path], value, label);
  if (!development)
    return (
      <details>
        <summary>Experimental ecological growth</summary>
        <p className="hint">
          Research model for shoots, light, resources, and pruning history. Its tree proportions are not
          visually qualified. Enabling it replaces the shaped appearance.
        </p>
        <button
          type="button"
          onClick={() =>
            onChange(
              ["botanical"],
              {
                ...botanical,
                ...(botanical.species === "pine"
                  ? { conifer: botanical.conifer ?? alpineConiferSchema.parse({}) }
                  : {}),
                development: botanicalDevelopmentSchema.parse({
                  species: botanical.species === "pine" ? "lodgepole-pine" : "paper-birch",
                }),
              },
              "Enable developmental growth",
            )
          }
        >
          Try experimental growth
        </button>
      </details>
    );
  const traits = botanicalSpeciesTraits[development.species];
  const organ = state?.shoots.find((shoot) => shoot.id === selected);
  const prune = () => {
    if (!state || !organ || state.step >= 64) return;
    const id = `prune-${state.step + 1}-${organ.id}`;
    onChange(
      ["botanical", "development"],
      {
        ...development,
        steps: state.step + 1,
        events: [...development.events, { id, kind: "prune", step: state.step + 1, organ: organ.id }],
      },
      "Prune and grow tree",
    );
  };
  return (
    <details open>
      <summary>Developmental growth</summary>
      <p>{traits.commonName}</p>
      <label className="row-control">
        <span>Growth step</span>
        <input
          aria-label="Growth step"
          type="range"
          min={0}
          max={64}
          step={1}
          value={development.steps}
          onChange={(event) => set(["steps"], Number(event.target.value), "Scrub tree development")}
        />
        <output>{development.steps}</output>
      </label>
      <p className="hint">
        Steps are developmental intervals, not calibrated years. Existing organ identities persist through
        growth.{" "}
        {state
          ? `${state.shoots.length.toLocaleString()} shoots; ${state.buds.filter((b) => b.state === "active").length.toLocaleString()} active buds.`
          : inspection.key === growthKey && inspection.error
            ? inspection.error
            : "Preparing growth history…"}
      </p>
      {(["light", "water", "fertility"] as const).map((key) => (
        <label className="row-control" key={key}>
          <span>
            {key === "fertility"
              ? "Resource availability"
              : key === "water"
                ? "Water availability"
                : "Seasonal light"}
          </span>
          <input
            type="range"
            aria-label={key}
            min={0}
            max={1}
            step={0.05}
            value={development.environment[key]}
            onChange={(event) =>
              set(["environment", key], Number(event.target.value), "Change growth environment")
            }
          />
        </label>
      ))}
      <button
        type="button"
        onClick={() =>
          set(
            ["environment", "neighbors"],
            development.environment.neighbors.length
              ? []
              : [
                  {
                    id: "review-neighbor",
                    center: [document.radius, document.height * 0.65, 0],
                    radius: document.radius,
                    height: document.height,
                    opacity: 0.9,
                  },
                ],
            "Change neighboring canopy",
          )
        }
      >
        {development.environment.neighbors.length ? "Remove shading neighbor" : "Add shading neighbor"}
      </button>
      <label className="row-control">
        <span>Inspect shoot</span>
        <select
          aria-label="Inspect growth shoot"
          value={selected}
          onChange={(event) => select(event.target.value)}
        >
          <option value="">Choose a shoot</option>
          {state?.shoots.map((shoot) => (
            <option key={shoot.id} value={shoot.id}>
              {shoot.id} · order {shoot.order} · {shoot.state}
            </option>
          ))}
        </select>
      </label>
      {organ && (
        <p className="hint">
          Born at step {organ.birth}. Radius {(organ.radius * 1000).toFixed(1)} model mm.{" "}
          {Math.round(organ.foliage.retained * 100)}% of its foliage cohort retained.{" "}
          {state?.buds.find((b) => b.parent === organ.id)?.reason ?? "Supporting wood"}.
        </p>
      )}
      <button
        type="button"
        disabled={
          !state || !organ || organ.state !== "living" || state.step >= 64 || development.events.length >= 256
        }
        onClick={prune}
      >
        Prune selected shoot and advance
      </button>
      <p className="hint">
        {development.events.length} recorded events. Resource reserve: {state?.reserve.toFixed(2) ?? "…"}{" "}
        model units. {state?.limited ? "Growth reached the bounded organ capacity." : ""}
      </p>
    </details>
  );
}
