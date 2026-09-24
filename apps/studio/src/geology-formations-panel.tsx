import { terrainHeight } from "@wrela/compiler";
import type { TerrainDefinition } from "@wrela/model";

import { useState } from "react";
import type { DomainPanelProps } from "./domain-authoring";
import { NumberEdit, nextId } from "./geology-controls";
import { geologyPublication } from "./geology-tools";

export function GeologyFormationsPanel({
  document,
  project,
  onChange,
  onApply,
}: DomainPanelProps<TerrainDefinition>) {
  const geology = document.geology;
  const [error, setError] = useState("");
  if (!geology) return null;
  const set = (path: (string | number)[], value: unknown) =>
    onChange(["geology", ...path], value, "Edit rock formation");
  const number = (
    label: string,
    value: number,
    min: number,
    max: number,
    path: (string | number)[],
    step: number | "any" = "any",
  ) => (
    <NumberEdit
      key={path.join(".")}
      {...{ label, value, min, max, step }}
      onChange={(value) => set(path, value)}
    />
  );
  return (
    <details>
      <summary>Caves and overhangs</summary>
      <p>
        Finite rock formations with carved openings and mesh collision. Publish to update all worlds using
        this terrain. Terrain beneath remains intact.
      </p>
      {geology.formations.map((formation, index) => (
        <fieldset key={formation.id}>
          <legend>{formation.id}</legend>
          <label>
            Formation{" "}
            <select
              aria-label={`${formation.id} type`}
              value={formation.kind}
              onChange={(event) => set(["formations", index, "kind"], event.target.value)}
            >
              <option value="cave">Cave</option>
              <option value="overhang">Overhang</option>
            </select>
          </label>
          <label>
            Rock material{" "}
            <select
              aria-label={`${formation.id} material`}
              value={formation.material ?? ""}
              onChange={(event) =>
                set(["formations", index], { ...formation, material: event.target.value || undefined })
              }
            >
              <option value="">Terrain material</option>
              {project.documents
                .filter((item) => item.kind === "material")
                .map((material) => (
                  <option key={material.id} value={material.id}>
                    {material.name}
                  </option>
                ))}
            </select>
          </label>
          {["X", "Y", "Z"].map((axis, i) =>
            number(
              `Position ${axis}`,
              formation.position[i],
              i === 1 ? -200 : -1_000_000,
              i === 1 ? 500 : 1_000_000,
              ["formations", index, "position", i],
            ),
          )}
          {["Width", "Height", "Depth"].map((axis, i) =>
            number(axis, formation.size[i], 4, 100, ["formations", index, "size", i]),
          )}
          <NumberEdit
            label="Heading (degrees)"
            value={((formation.heading ?? 0) * 180) / Math.PI}
            min={-180}
            max={180}
            onChange={(value) =>
              set(["formations", index], { ...formation, heading: (value * Math.PI) / 180 })
            }
          />
          <button
            type="button"
            onClick={() =>
              set(["formations", index], {
                ...formation,
                rock: formation.rock ? undefined : { seed: 81, layers: 5, fracture: 0.65 },
              })
            }
          >
            {formation.rock ? "Use plain exterior" : "Sculpt stratified rock"}
          </button>
          {formation.rock && (
            <>
              {number("Rock seed", formation.rock.seed, 0, 65_535, ["formations", index, "rock", "seed"], 1)}
              {number("Rock layers", formation.rock.layers, 2, 8, ["formations", index, "rock", "layers"], 1)}
              {number("Fracture strength", formation.rock.fracture, 0, 1, [
                "formations",
                index,
                "rock",
                "fracture",
              ])}
              <NumberEdit
                label="Bedding dip (degrees)"
                value={((formation.rock.bedding?.dip ?? 0.19) * 180) / Math.PI}
                min={-45.8}
                max={45.8}
                onChange={(value) =>
                  set(["formations", index, "rock"], {
                    ...formation.rock,
                    bedding: {
                      strike: formation.rock?.bedding?.strike ?? -0.28,
                      dip: (value * Math.PI) / 180,
                    },
                  })
                }
              />
              <NumberEdit
                label="Bedding direction (degrees)"
                value={((formation.rock.bedding?.strike ?? -0.28) * 180) / Math.PI}
                min={-180}
                max={180}
                onChange={(value) =>
                  set(["formations", index, "rock"], {
                    ...formation.rock,
                    bedding: { dip: formation.rock?.bedding?.dip ?? 0.19, strike: (value * Math.PI) / 180 },
                  })
                }
              />
              <NumberEdit
                label="Joint span"
                value={formation.rock.fractureScale ?? formation.size[0] * 0.24}
                min={0.1}
                max={40}
                onChange={(value) =>
                  set(["formations", index, "rock"], { ...formation.rock, fractureScale: value })
                }
              />
            </>
          )}
          {number("Opening ratio", formation.opening, 0.2, 0.8, ["formations", index, "opening"])}
          {number("Mesh resolution", formation.resolution, 24, 64, ["formations", index, "resolution"], 1)}
          <button
            type="button"
            onClick={() =>
              set(
                ["formations", index, "position", 1],
                Math.max(
                  -200,
                  Math.min(500, terrainHeight(document, formation.position[0], formation.position[2])),
                ),
              )
            }
          >
            Rest on terrain
          </button>
          <button
            type="button"
            onClick={() =>
              set(
                ["formations"],
                geology.formations.filter((_, i) => i !== index),
              )
            }
          >
            Remove formation
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={geology.formations.length >= 8}
        onClick={() =>
          set(
            ["formations"],
            [
              ...geology.formations,
              {
                id: nextId("formation", geology.formations),
                kind: "cave",
                position: [0, Math.max(-200, Math.min(500, terrainHeight(document, 0, 0))), 0],
                size: [12, 8, 10],
                opening: 0.65,
                resolution: 48,
                heading: 0,
                rock: { seed: 81, layers: 5, fracture: 0.65 },
              },
            ],
          )
        }
      >
        Add formation
      </button>
      <button
        type="button"
        onClick={() => {
          try {
            setError("");
            onApply(geologyPublication(project, document), "Publish geological formations");
          } catch (cause) {
            setError(cause instanceof Error ? cause.message : String(cause));
          }
        }}
      >
        Publish formations to worlds
      </button>
      {error && <p role="alert">{error}</p>}
    </details>
  );
}
