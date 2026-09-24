import { reviewGeology } from "@wrela/compiler";
import { defaultTerrainGeology, type TerrainDefinition } from "@wrela/model";

import { useEffect, useState } from "react";
import type { DomainPanelProps } from "./domain-authoring";
import { NumberEdit, nextId } from "./geology-controls";
import { GeologyCorridorPanel } from "./geology-corridor-panel";
import { GeologyFormationsPanel } from "./geology-formations-panel";
import { GeologyProfilePanel } from "./geology-profile-panel";

export function GeologyAuthoringPanel({
  document,
  project,
  onChange,
  onApply,
}: DomainPanelProps<TerrainDefinition>) {
  const geology = document.geology;
  const [review, setReview] = useState<ReturnType<typeof reviewGeology>>();
  useEffect(() => setReview(undefined), [document]);
  if (!geology)
    return (
      <section>
        <h3>Geology</h3>
        <button
          type="button"
          onClick={() => onChange(["geology"], defaultTerrainGeology(), "Enable geology authoring")}
        >
          Enable geology authoring
        </button>
      </section>
    );
  const set = (path: (string | number)[], value: unknown) =>
    onChange(["geology", ...path], value, "Edit geology");
  const number = (
    label: string,
    value: number,
    min: number,
    max: number,
    path: (string | number)[],
    step: number | "any" = "any",
  ) => (
    <NumberEdit key={path.join(".")} {...{ label, value, min, max, step }} onChange={(v) => set(path, v)} />
  );
  return (
    <section className="domain-authoring">
      <h3>Landforms and geology</h3>
      <p>
        Distances are metres. Local terrain interventions apply after geology, preserving authored flattening
        and clearings.
      </p>
      <details open>
        <summary>Landforms</summary>
        {geology.landforms.map((feature, index) => (
          <fieldset key={feature.id}>
            <legend>{feature.id}</legend>
            <label>
              Shape{" "}
              <select
                value={feature.kind}
                aria-label={`${feature.id} shape`}
                onChange={(event) =>
                  set([], {
                    ...geology,
                    landforms: geology.landforms.map((item, i) =>
                      i === index ? { ...item, kind: event.target.value, profile: undefined } : item,
                    ),
                    bankWetness:
                      geology.bankWetness?.drainageId === feature.id && event.target.value !== "drainage"
                        ? undefined
                        : geology.bankWetness,
                  })
                }
              >
                <option value="ridge">Ridge</option>
                <option value="drainage">Drainage channel</option>
                <option value="cliff">Cliff</option>
              </select>
            </label>
            {number("Width", feature.width, 0.5, 500, ["landforms", index, "width"])}
            {number("Height / depth", feature.height, 0, 100, ["landforms", index, "height"])}
            {number("Falloff", feature.falloff, 0.25, 8, ["landforms", index, "falloff"])}
            <GeologyProfilePanel feature={feature} onChange={(value) => set(["landforms", index], value)} />
            {feature.points.map((point, pointIndex) => (
              <div key={`${feature.id}-${pointIndex}`}>
                {number(`Point ${pointIndex + 1} X`, point[0], -1_000_000, 1_000_000, [
                  "landforms",
                  index,
                  "points",
                  pointIndex,
                  0,
                ])}
                {number(`Point ${pointIndex + 1} Z`, point[1], -1_000_000, 1_000_000, [
                  "landforms",
                  index,
                  "points",
                  pointIndex,
                  1,
                ])}
                <button
                  type="button"
                  disabled={feature.points.length <= 2}
                  onClick={() =>
                    set(
                      ["landforms", index, "points"],
                      feature.points.filter((_, p) => p !== pointIndex),
                    )
                  }
                >
                  Remove point {pointIndex + 1}
                </button>
              </div>
            ))}
            <button
              type="button"
              disabled={feature.points.length >= 32}
              onClick={() => {
                const last = feature.points[feature.points.length - 1];
                set(
                  ["landforms", index, "points"],
                  [...feature.points, [Math.min(1_000_000, last[0] + 10), last[1]]],
                );
              }}
            >
              Extend path
            </button>
            <button
              type="button"
              onClick={() =>
                set([], {
                  ...geology,
                  landforms: geology.landforms.filter((_, i) => i !== index),
                  bankWetness:
                    geology.bankWetness?.drainageId === feature.id ? undefined : geology.bankWetness,
                })
              }
            >
              Remove landform
            </button>
          </fieldset>
        ))}
        <button
          type="button"
          disabled={geology.landforms.length >= 24}
          onClick={() =>
            set(
              ["landforms"],
              [
                ...geology.landforms,
                {
                  id: nextId("landform", geology.landforms),
                  kind: "ridge",
                  points: [
                    [-10, 0],
                    [10, 0],
                  ],
                  width: 8,
                  height: 4,
                  falloff: 1,
                },
              ],
            )
          }
        >
          Add landform
        </button>
      </details>
      <details>
        <summary>Erosion and strata</summary>
        <p>
          Talus relaxation softens steep local slopes. Strata terrace the surface; these are deterministic
          sculpting filters.
        </p>
        {number("Erosion strength", geology.erosion.strength, 0, 1, ["erosion", "strength"])}
        {number("Erosion radius", geology.erosion.radius, 0.25, 20, ["erosion", "radius"])}
        {number("Talus angle", geology.erosion.talusAngle, 5, 80, ["erosion", "talusAngle"])}
        {number("Strata thickness", geology.strata.thickness, 0.25, 20, ["strata", "thickness"])}
        {number("Strata strength", geology.strata.strength, 0, 1, ["strata", "strength"])}
        <label>
          Wet drainage banks{" "}
          <select
            aria-label="Wet drainage banks"
            value={geology.bankWetness?.drainageId ?? ""}
            onChange={(event) =>
              set([], {
                ...geology,
                bankWetness: event.target.value
                  ? {
                      waterHalfWidth: 1.8,
                      fadeWidth: 3.5,
                      darkening: 0.3,
                      ...geology.bankWetness,
                      drainageId: event.target.value,
                    }
                  : undefined,
              })
            }
          >
            <option value="">None</option>
            {geology.landforms
              .filter((item) => item.kind === "drainage")
              .map((item) => (
                <option key={item.id} value={item.id}>
                  {item.id}
                </option>
              ))}
          </select>
        </label>
        {geology.bankWetness && (
          <>
            {number("Wet bank water half width", geology.bankWetness.waterHalfWidth, 0.1, 250, [
              "bankWetness",
              "waterHalfWidth",
            ])}
            {number("Wet bank fade width", geology.bankWetness.fadeWidth, 0.1, 50, [
              "bankWetness",
              "fadeWidth",
            ])}
            {number("Wet bank darkening", geology.bankWetness.darkening, 0, 0.8, [
              "bankWetness",
              "darkening",
            ])}
          </>
        )}
      </details>
      <GeologyFormationsPanel {...{ document, project, onChange, onApply }} />
      <GeologyCorridorPanel document={document} onChange={onChange} />
      <details>
        <summary>Traversal, collision and sightline review</summary>
        {geology.review.route.map((point, index) => (
          <div key={`route-${index}`}>
            {number(`Route ${index + 1} X`, point[0], -1_000_000, 1_000_000, ["review", "route", index, 0])}
            {number(`Route ${index + 1} Z`, point[1], -1_000_000, 1_000_000, ["review", "route", index, 1])}
            <button
              type="button"
              disabled={geology.review.route.length <= 2}
              onClick={() =>
                set(
                  ["review", "route"],
                  geology.review.route.filter((_, i) => i !== index),
                )
              }
            >
              Remove waypoint {index + 1}
            </button>
          </div>
        ))}
        <button
          type="button"
          disabled={geology.review.route.length >= 32}
          onClick={() => {
            const last = geology.review.route[geology.review.route.length - 1];
            set(["review", "route"], [...geology.review.route, [Math.min(1_000_000, last[0] + 10), last[1]]]);
          }}
        >
          Add route waypoint
        </button>
        {number("Maximum walkable slope", geology.review.maxSlope, 0, 80, ["review", "maxSlope"])}
        {number("Eye height", geology.review.eyeHeight, 0.1, 5, ["review", "eyeHeight"])}
        {number("Body clearance", geology.review.clearance, 0.1, 10, ["review", "clearance"])}
        <NumberEdit
          label="Body radius"
          value={geology.review.bodyRadius ?? 0.35}
          min={0}
          max={2}
          onChange={(value) => set(["review"], { ...geology.review, bodyRadius: value })}
        />
        <button type="button" onClick={() => setReview(reviewGeology(document))}>
          Review route
        </button>
        {review && (
          <output>
            <p>
              Maximum slope {review.maxSlope.toFixed(1)}°. {review.steepSamples} steep samples;{" "}
              {review.collisionSamples} obstructed samples. Endpoint sightline{" "}
              {review.sightlineClear ? "clear" : "blocked"}.
            </p>
            <p>
              Sample spacing {review.sampleSpacing.toFixed(2)} m. Uses authored formations, including
              unpublished edits. Body radius {review.bodyRadius.toFixed(2)} m. Approximate footprint checks
              exclude other world objects; playtest after publishing.
            </p>
          </output>
        )}
      </details>
    </section>
  );
}
