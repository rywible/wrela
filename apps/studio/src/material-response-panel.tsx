import { type SurfaceAppearance, surfaceResponseDefaults } from "@wrela/model";

import { MaterialNumberControl, materialColorFromHex, materialColorToHex } from "./material-controls";

export function MaterialResponsePanel({
  appearance,
  explicitCreatureResponse,
  change,
}: {
  appearance: SurfaceAppearance;
  explicitCreatureResponse: boolean;
  change: (value: SurfaceAppearance["response"]) => void;
}) {
  if (appearance.family === "glass") return null;
  const response = { ...surfaceResponseDefaults(appearance.family), ...appearance.response };
  const update = (key: keyof typeof response, value: unknown) =>
    change({ ...appearance.response, [key]: value });
  const transmissive = appearance.family === "skin" || appearance.family === "foliage";
  return (
    <fieldset disabled={explicitCreatureResponse}>
      <legend>Substance response</legend>
      {explicitCreatureResponse && (
        <p className="hint">
          The authored creature response controls this material. Edit its creature optics to change scattering
          and sheen.
        </p>
      )}
      {transmissive && (
        <>
          {appearance.family === "skin" && (
            <MaterialNumberControl
              label="Subsurface wrap"
              value={response.subsurface}
              change={(value) => update("subsurface", value)}
            />
          )}
          <MaterialNumberControl
            label="Optical thickness (metres)"
            value={response.thickness}
            max={1}
            step={0.0001}
            change={(value) => update("thickness", value)}
          />
          <label className="row-control">
            Scatter tint
            <input
              type="color"
              value={materialColorToHex(response.scatterColor)}
              onChange={(event) => update("scatterColor", materialColorFromHex(event.currentTarget.value))}
            />
          </label>
        </>
      )}
      {appearance.family === "fabric" && (
        <>
          <MaterialNumberControl
            label="Fiber sheen"
            value={response.sheen}
            change={(value) => update("sheen", value)}
          />
          <MaterialNumberControl
            label="Fiber anisotropy"
            value={response.anisotropy}
            min={-0.95}
            max={0.95}
            change={(value) => update("anisotropy", value)}
          />
        </>
      )}
      <MaterialNumberControl
        label="Clearcoat"
        value={response.clearcoat}
        change={(value) => update("clearcoat", value)}
      />
      {response.clearcoat > 0 && (
        <MaterialNumberControl
          label="Clearcoat roughness"
          value={response.clearcoatRoughness}
          min={0.04}
          change={(value) => update("clearcoatRoughness", value)}
        />
      )}
      {appearance.response && (
        <button type="button" onClick={() => change(undefined)}>
          Reset substance response
        </button>
      )}
    </fieldset>
  );
}
