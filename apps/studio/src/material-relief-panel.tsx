import { type SurfaceAppearance, type SurfaceRelief, surfaceReliefSchema } from "@wrela/model";

import { MaterialNumberControl } from "./material-controls";

/** Replace the optional parent recipe, including when no relief object exists yet. */
export function MaterialReliefPanel({
  appearance,
  change,
}: {
  appearance: SurfaceAppearance;
  change: (appearance: SurfaceAppearance) => void;
}) {
  const relief = appearance.relief;
  const update = (patch: Partial<SurfaceRelief>) => {
    if (!relief) return;
    const parsed = surfaceReliefSchema.safeParse({ ...relief, ...patch });
    if (parsed.success) change({ ...appearance, relief: parsed.data });
  };
  const select = (kind: SurfaceRelief["kind"] | "none") => {
    if (kind === "none") {
      const next = { ...appearance };
      delete next.relief;
      change(next);
    } else
      change({
        ...appearance,
        relief: surfaceReliefSchema.parse(
          relief
            ? { ...relief, kind }
            : {
                kind,
                amplitude: kind === "bark" ? 0.022 : 0.012,
                scale: kind === "bark" ? 0.07 : 0.1,
                targetEdgeLength: 0.025,
                seed: 17,
                direction: [0, 1, 0],
              },
        ),
      });
  };
  return (
    <fieldset>
      <legend>Physical surface relief</legend>
      <p>Carve bark grooves or stone chips into the surface. These change its silhouette and cast shadows.</p>
      <label className="row-control">
        Relief structure
        <select
          value={relief?.kind ?? "none"}
          onChange={(event) => select(event.currentTarget.value as SurfaceRelief["kind"] | "none")}
        >
          <option value="none">Smooth geometry</option>
          <option value="bark">Bark grooves</option>
          <option value="stone">Stone chips</option>
        </select>
      </label>
      {relief && (
        <>
          <MaterialNumberControl
            label="Maximum depth (mm)"
            value={relief.amplitude * 1000}
            min={0}
            max={80}
            step={1}
            change={(value) => update({ amplitude: value / 1000 })}
          />
          <MaterialNumberControl
            label={relief.kind === "bark" ? "Groove spacing (mm)" : "Chip spacing (mm)"}
            value={relief.scale * 1000}
            min={5}
            max={2000}
            step={1}
            change={(value) => update({ scale: value / 1000 })}
          />
          <MaterialNumberControl
            label="Close-up detail spacing (mm)"
            value={relief.targetEdgeLength * 1000}
            min={2}
            max={500}
            step={1}
            change={(value) => update({ targetEdgeLength: value / 1000 })}
          />
          <p>
            Smaller detail spacing costs more geometry. Distant views use a simpler surface when the depth
            falls below the screen error budget. Detail is capped when the geometry budget is reached.
          </p>
          <MaterialNumberControl
            label="Relief variation seed"
            value={relief.seed}
            min={-2147483648}
            max={2147483647}
            step={1}
            change={(value) => update({ seed: Math.round(value) })}
          />
          {relief.kind === "bark" && (
            <label className="row-control">
              Grain direction
              <select
                value={
                  relief.direction[0] === 0 && relief.direction[2] === 0
                    ? "y"
                    : relief.direction[1] === 0 && relief.direction[2] === 0
                      ? "x"
                      : relief.direction[0] === 0 && relief.direction[1] === 0
                        ? "z"
                        : "custom"
                }
                onChange={(event) => {
                  const axis = event.currentTarget.value;
                  if (axis !== "custom")
                    update({ direction: axis === "x" ? [1, 0, 0] : axis === "z" ? [0, 0, 1] : [0, 1, 0] });
                }}
              >
                <option value="y">Along height</option>
                <option value="x">Along width</option>
                <option value="z">Along depth</option>
                <option value="custom" disabled>
                  Authored custom direction
                </option>
              </select>
            </label>
          )}
          {relief.targetEdgeLength > relief.scale / 2 && (
            <p className="hint">
              Detail spacing is larger than half the groove or chip spacing. Reduce it to represent those
              shapes in close-up views.
            </p>
          )}
        </>
      )}
    </fieldset>
  );
}
