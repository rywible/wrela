import { createGeologyCorridor, createTerrainSampler } from "@wrela/compiler";
import type { TerrainDefinition } from "@wrela/model";

import type { DomainPanelProps } from "./domain-authoring";
import { NumberEdit, nextId } from "./geology-controls";

/** Elevation profiles remain explicit editable source, including after re-sculpting landforms. */
export function GeologyCorridorPanel({
  document,
  onChange,
}: Pick<DomainPanelProps<TerrainDefinition>, "document" | "onChange">) {
  const geology = document.geology;
  if (!geology) return null;
  const corridors = geology.corridors ?? [];
  const set = (path: (string | number)[], value: unknown) =>
    path.length
      ? onChange(["geology", "corridors", ...path], value, "Edit traversal corridor")
      : onChange(["geology"], { ...geology, corridors: value }, "Edit traversal corridor");
  return (
    <details>
      <summary>Protected traversal corridors</summary>
      <p>
        Preserve an exact width and elevation through future geological edits. The initial profile limits
        grades to the review route's slope threshold; elevations remain editable. Local terrain interventions
        apply last.
      </p>
      {corridors.map((corridor, index) => (
        <fieldset key={corridor.id}>
          <legend>{corridor.id}</legend>
          <NumberEdit
            label="Corridor half width"
            value={corridor.halfWidth}
            min={0.2}
            max={50}
            onChange={(value) => set([index, "halfWidth"], value)}
          />
          <NumberEdit
            label="Shoulder width"
            value={corridor.shoulder}
            min={0.2}
            max={100}
            onChange={(value) => set([index, "shoulder"], value)}
          />
          {corridor.points.map((point, pointIndex) => (
            <div key={pointIndex}>
              {(["X", "Elevation", "Z"] as const).map((axis, axisIndex) => (
                <NumberEdit
                  key={axis}
                  label={`Corridor point ${pointIndex + 1} ${axis}`}
                  value={point[axisIndex]}
                  min={axisIndex === 1 ? -5_000 : -1_000_000}
                  max={axisIndex === 1 ? 5_000 : 1_000_000}
                  onChange={(value) => set([index, "points", pointIndex, axisIndex], value)}
                />
              ))}
            </div>
          ))}
          <button
            type="button"
            onClick={() =>
              onChange(
                ["geology", "review", "route"],
                corridor.points.map(([x, , z]) => [x, z]),
                "Review corridor route",
              )
            }
          >
            Use as review route
          </button>
          <button
            type="button"
            onClick={() =>
              set(
                [],
                corridors.filter((_, i) => i !== index),
              )
            }
          >
            Remove corridor
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={corridors.length >= 16}
        onClick={() => {
          const corridor = createGeologyCorridor(
            document,
            createTerrainSampler(document).height,
            nextId("corridor", corridors),
          );
          set([], [...corridors, corridor]);
        }}
      >
        Protect current review route
      </button>
    </details>
  );
}
