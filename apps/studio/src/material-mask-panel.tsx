import type { SurfaceMask } from "@wrela/model";

import { MaterialNumberControl } from "./material-controls";

export function MaterialMaskPanel({
  mask,
  index,
  change,
}: {
  mask: SurfaceMask;
  index: number;
  change: (key: keyof SurfaceMask, value: unknown) => void;
}) {
  const label = (name: string) => `Layer ${index + 1} ${name}`;
  const hasNoise = mask.kind === "noise" || mask.kind === "combined";
  const hasHeight = mask.kind === "height" || mask.kind === "combined";
  return (
    <>
      <label className="row-control">
        Mask
        <select value={mask.kind} onChange={(event) => change("kind", event.currentTarget.value)}>
          {["uniform", "noise", "slope", "height", "combined"].map((kind) => (
            <option key={kind}>{kind}</option>
          ))}
        </select>
      </label>
      {hasNoise && (
        <MaterialNumberControl
          label={label("mask scale (per metre)")}
          value={mask.scale}
          min={0.01}
          max={100}
          change={(value) => change("scale", value)}
        />
      )}
      {mask.kind !== "uniform" && (
        <MaterialNumberControl
          label={label("softness")}
          value={mask.softness}
          min={0.001}
          change={(value) => change("softness", value)}
        />
      )}
      {(hasNoise || mask.kind === "slope") && (
        <MaterialNumberControl
          label={label("threshold")}
          value={mask.threshold}
          change={(value) => change("threshold", value)}
        />
      )}
      {mask.kind === "combined" && (
        <>
          <p className="hint">
            Noise is restricted to the selected slope and height band. Inversion applies to the combined
            result.
          </p>
          <MaterialNumberControl
            label={label("slope influence")}
            value={mask.slopeInfluence ?? 1}
            change={(value) => change("slopeInfluence", value)}
          />
          <MaterialNumberControl
            label={label("slope threshold")}
            value={mask.slopeThreshold ?? 0.5}
            change={(value) => change("slopeThreshold", value)}
          />
          <MaterialNumberControl
            label={label("slope softness")}
            value={mask.slopeSoftness ?? 0.15}
            min={0.001}
            change={(value) => change("slopeSoftness", value)}
          />
          <MaterialNumberControl
            label={label("height influence")}
            value={mask.heightInfluence ?? 1}
            change={(value) => change("heightInfluence", value)}
          />
        </>
      )}
      {hasHeight && (
        <>
          <MaterialNumberControl
            label={label("minimum height")}
            value={mask.minimumHeight}
            min={-1e6}
            max={mask.maximumHeight}
            change={(value) => change("minimumHeight", value)}
          />
          <MaterialNumberControl
            label={label("maximum height")}
            value={mask.maximumHeight}
            min={mask.minimumHeight}
            max={1e6}
            change={(value) => change("maximumHeight", value)}
          />
        </>
      )}
      <label className="row-control">
        <input
          type="checkbox"
          checked={mask.invert}
          onChange={(event) => change("invert", event.currentTarget.checked)}
        />
        Invert mask
      </label>
    </>
  );
}
