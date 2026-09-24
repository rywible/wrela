import {
  addWorldLayout,
  appendWorldPathPoint,
  duplicateAssemblyPlacement,
  removeWorldLayoutElement,
  type WorldLayoutCollection,
  type WorldLayoutKind,
} from "@wrela/authoring";
import {
  emptyWorldComposition,
  validateWorldComposition,
  type WorldComposition,
  type WorldDefinition,
} from "@wrela/model";

import { useState } from "react";
import type { DomainPanelProps } from "./domain-authoring";
import { WorldSpatialPanel } from "./world-spatial-panel";

type Path = (string | number)[];
const labels: Record<string, string> = {
  placements: "Assembly placements",
  assemblies: "Reusable assemblies",
  paths: "Roads and paths",
  rooms: "Rooms",
  biomes: "Biome transitions",
  spaces: "Landmarks and encounters",
  streaming: "Streaming regions",
  overrides: "Placement exceptions",
  yaw: "Rotation (radians)",
  maxGrade: "Maximum traversal grade",
  moduleWidth: "Wall module width",
  wallDefinition: "Wall object",
  clearPopulation: "Keep population clear",
  preloadDistance: "Preload distance",
  cornerRadius: "Corner rounding (meters)",
  grounding: "Follow terrain",
  offset: "Height above terrain",
};
/** Structured controls keep document edits on the shared validated, undoable transaction path. */
export function WorldAuthoringPanel({ document, project, onChange }: DomainPanelProps<WorldDefinition>) {
  const [definition, setDefinition] = useState("");
  const definitions = project.documents.filter((item) =>
    ["object", "character", "vegetation"].includes(item.kind),
  );
  const selected = definition || definitions[0]?.id;
  const composition = document.composition ?? emptyWorldComposition();
  const save = (value: WorldComposition, label = "Edit world composition") =>
    onChange(["composition"], value, label);
  const change = (path: Path, value: unknown) => {
    const next = structuredClone(composition);
    let cursor: unknown = next;
    for (const key of path.slice(0, -1)) cursor = (cursor as Record<string | number, unknown>)[key];
    (cursor as Record<string | number, unknown>)[path[path.length - 1]] = value;
    save(next);
  };
  const remove = (path: Path) => {
    if (path.length === 2) {
      const collection = path[0] as WorldLayoutCollection;
      const item = composition[collection]?.[Number(path[1])];
      if (item) save(removeWorldLayoutElement(composition, collection, item.id), "Remove layout element");
      return;
    }
    const next = structuredClone(composition);
    let cursor: unknown = next;
    for (const key of path.slice(0, -1)) cursor = (cursor as Record<string | number, unknown>)[key];
    if (Array.isArray(cursor)) cursor.splice(Number(path[path.length - 1]), 1);
    save(next, "Remove layout element");
  };
  function control(value: unknown, path: Path, key: string): React.ReactNode {
    const label = labels[key] ?? key.replace(/([A-Z])/g, " $1");
    if (typeof value === "boolean")
      return (
        <label className="row-control" key={path.join(".")}>
          <input type="checkbox" checked={value} onChange={(event) => change(path, event.target.checked)} />{" "}
          {label}
        </label>
      );
    if (typeof value === "number")
      return (
        <label className="row-control" key={path.join(".")}>
          {label}{" "}
          <input
            aria-label={label}
            type="number"
            step="any"
            value={value}
            onChange={(event) => {
              if (Number.isFinite(event.target.valueAsNumber)) change(path, event.target.valueAsNumber);
            }}
          />
        </label>
      );
    if (typeof value === "string") {
      const options =
        key === "definition" || key === "wallDefinition"
          ? definitions.map((item) => ({ value: item.id, label: item.name }))
          : key === "assembly"
            ? composition.assemblies.map((item) => ({ value: item.id, label: item.name }))
            : key === "doorSide"
              ? ["north", "east", "south", "west"].map((side) => ({ value: side, label: side }))
              : key === "kind"
                ? (value === "road" || value === "path" ? ["road", "path"] : ["landmark", "encounter"]).map(
                    (kind) => ({ value: kind, label: kind }),
                  )
                : undefined;
      return (
        <label className="row-control" key={path.join(".")} htmlFor={`world-${path.join("-")}`}>
          {label}{" "}
          {options ? (
            <select
              id={`world-${path.join("-")}`}
              value={value}
              onChange={(event) => change(path, event.target.value)}
            >
              {options.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          ) : (
            <input
              id={`world-${path.join("-")}`}
              value={value}
              onChange={(event) => change(path, event.target.value)}
            />
          )}
        </label>
      );
    }
    if (Array.isArray(value)) {
      if (value.length && value.every((item) => typeof item === "number"))
        return (
          <fieldset style={{ minWidth: 0, border: 0, padding: "0.25rem 0" }} key={path.join(".")}>
            <legend>{label}</legend>
            {value.map((item, index) => control(item, [...path, index], ["x", "y", "z"][index]))}
          </fieldset>
        );
      return (
        <fieldset style={{ minWidth: 0, border: 0, padding: "0.25rem 0" }} key={path.join(".")}>
          <legend>{label}</legend>
          {value.map((item, index) => (
            <div key={`${path.join(".")}-${index}`}>
              {control(item, [...path, index], `${key} ${index + 1}`)}
              <button type="button" onClick={() => remove([...path, index])}>
                Remove
              </button>
            </div>
          ))}
          {["members", "points", "populations"].includes(key) && (
            <button
              type="button"
              onClick={() => {
                if (key === "points") {
                  const route = composition.paths[Number(path[1])];
                  if (route) save(appendWorldPathPoint(composition, route.id), "Extend route");
                  return;
                }
                const memberId = `part-${Date.now().toString(36)}`;
                const item =
                  key === "members"
                    ? selected && {
                        id: memberId,
                        definition: selected,
                        position: [0, 0, 0],
                        yaw: 0,
                        scale: 1,
                      }
                    : document.populations.find((rule) => !value.includes(rule.id))?.id;
                if (item) change(path, [...value, item]);
              }}
            >
              Add {key === "members" ? "member" : key === "points" ? "point" : "population"}
            </button>
          )}
        </fieldset>
      );
    }
    if (value && typeof value === "object")
      return (
        <fieldset style={{ minWidth: 0, border: 0, padding: "0.25rem 0" }} key={path.join(".")}>
          <legend>{label}</legend>
          {Object.entries(value)
            .filter(([name]) => name !== "cornerRadius")
            .map(([name, item]) => control(item, [...path, name], name))}
        </fieldset>
      );
    return null;
  }
  const diagnostics = validateWorldComposition(
    composition,
    document.populations.map((rule) => rule.id),
  );
  return (
    <section aria-label="World composition">
      <h3>World composition</h3>
      <p>
        Compose reusable parts, traversal routes, plant communities, and playable spaces. Distances are in
        meters.
      </p>
      <label className="row-control">
        Object for new layouts{" "}
        <select value={selected ?? ""} onChange={(event) => setDefinition(event.target.value)}>
          {definitions.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
      <div>
        {(
          ["assembly", "path", "room", "biome", "landmark", "encounter", "streaming"] as WorldLayoutKind[]
        ).map((kind) => (
          <button
            key={kind}
            type="button"
            disabled={kind === "room" && !selected}
            onClick={() => save(addWorldLayout(document, kind, selected), `Add ${kind}`)}
          >
            Add {kind}
          </button>
        ))}
      </div>
      {diagnostics.map((issue, index) => (
        <output key={`${issue.code}-${index}`}>{issue.message}</output>
      ))}
      <WorldSpatialPanel world={document} project={project} composition={composition} onChange={save} />
      {Object.entries(composition)
        .filter((entry) => Array.isArray(entry[1]))
        .map(([key, items]) => (
          <details key={key} open={key === "overrides"}>
            <summary>
              {labels[key]} ({Array.isArray(items) ? items.length : 0})
            </summary>
            {control(items, [key], key)}
          </details>
        ))}
      {composition.paths.map((path, index) => (
        <fieldset key={path.id}>
          <legend>{path.id} route</legend>
          <label className="row-control">
            Corner rounding (meters)
            <input
              type="number"
              min="0"
              max="100"
              step="0.5"
              value={path.cornerRadius ?? 0}
              onChange={(event) => {
                if (Number.isFinite(event.target.valueAsNumber))
                  change(["paths", index, "cornerRadius"], event.target.valueAsNumber);
              }}
            />
          </label>
          <label className="row-control" key={path.id}>
            Surface module for {path.id}{" "}
            <select
              value={path.definition ?? ""}
              onChange={(event) => change(["paths", index, "definition"], event.target.value || undefined)}
            >
              <option value="">Terrain only</option>
              {definitions.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
          </label>
        </fieldset>
      ))}
      {(["placements", "spaces"] as const).flatMap((collection) =>
        composition[collection].map((group, index) => (
          <label className="row-control" key={`${collection}-${group.id}`}>
            <input
              type="checkbox"
              checked={!!group.grounding}
              onChange={(event) =>
                change([collection, index, "grounding"], event.target.checked ? { offset: 0 } : undefined)
              }
            />
            Ground {group.id} as a group
          </label>
        )),
      )}
      {composition.placements.map((placement) => (
        <button
          type="button"
          key={placement.id}
          onClick={() => save(duplicateAssemblyPlacement(composition, placement.id), "Duplicate assembly")}
        >
          Duplicate {placement.id}
        </button>
      ))}
      <button
        type="button"
        onClick={() =>
          save(
            {
              ...composition,
              overrides: [
                ...composition.overrides,
                {
                  id: `placement-${Date.now().toString(36)}`,
                  removed: false,
                  position: [0, 0, 0],
                  yaw: 0,
                  scale: 1,
                },
              ],
            },
            "Add placement exception",
          )
        }
      >
        Add placement exception
      </button>
      <p>
        Exceptions use the generated placement identity. Runtime saved changes take precedence. Rooms repeat
        the selected wall module; encounters place actors but do not define combat behavior.
      </p>
    </section>
  );
}
