import { MaterialEmissionPanel } from "./material-emission-panel";
import {
  createSurfaceAppearance,
  createSurfaceLayer,
  type MaterialDefinition,
  type SurfaceAppearance,
} from "@wrela/model";

import { SURFACE_REVIEW_PRESETS, surfaceReviewWarnings } from "@wrela/runtime";
import type { DomainPanelProps } from "./domain-authoring";

import {
  materialColorToHex as hex,
  MaterialNumberControl as NumberControl,
  materialColorFromHex as rgb,
} from "./material-controls";
import { MaterialMaskPanel } from "./material-mask-panel";
import { MaterialReliefPanel } from "./material-relief-panel";
import { MaterialResponsePanel } from "./material-response-panel";

export function MaterialAuthoringPanel({
  document,
  onChange,
  onPreview,
}: DomainPanelProps<MaterialDefinition>) {
  const appearance = document.appearance;
  if (!appearance)
    return (
      <section>
        <MaterialEmissionPanel document={document} onChange={onChange} />
        <h3>Substance and coatings</h3>
        <p>Author substance, layered coatings, and surface history.</p>
        <button
          type="button"
          onClick={() => onChange(["appearance"], createSurfaceAppearance(), "Enable surface appearance")}
        >
          Enable surface authoring
        </button>
      </section>
    );
  const change = (key: keyof SurfaceAppearance, value: unknown) =>
    onChange(["appearance", key], value, `Edit surface ${key}`);
  const layerChange = (index: number, path: string[], value: unknown) =>
    onChange(["appearance", "layers", index, ...path], value, "Edit surface layer");
  return (
    <section aria-label="Surface appearance authoring">
      <MaterialEmissionPanel document={document} onChange={onChange} />
      <h3>Substance and coatings</h3>
      <label className="row-control">
        Substance
        <select value={appearance.family} onChange={(event) => change("family", event.currentTarget.value)}>
          {["generic", "skin", "foliage", "fabric", "metal", "glass"].map((family) => (
            <option key={family}>{family}</option>
          ))}
        </select>
      </label>
      <label className="row-control">
        Coordinate space
        <select
          value={document.domain ?? "local"}
          onChange={(event) => onChange(["domain"], event.currentTarget.value)}
        >
          <option value="local">Attached to surface</option>
          <option value="world">World metres</option>
        </select>
      </label>
      <NumberControl
        label="Pattern scale"
        value={document.scale}
        min={0.01}
        max={100}
        change={(value) => onChange(["scale"], value)}
      />
      <MaterialResponsePanel
        appearance={appearance}
        explicitCreatureResponse={Boolean(document.creature)}
        change={(value) => change("response", value)}
      />
      <h4>Surface detail</h4>
      <MaterialReliefPanel
        appearance={appearance}
        change={(value) => onChange(["appearance"], value, "Edit physical surface relief")}
      />
      <label className="row-control">
        Detail structure
        <select
          value={appearance.detail.kind}
          onChange={(event) =>
            onChange(["appearance", "detail", "kind"], event.currentTarget.value, "Edit surface detail")
          }
        >
          <option value="none">None</option>
          <option value="wood">Wood grain</option>
          <option value="birch-bark">Birch bark</option>
          <option value="bark">Bark plates</option>
          <option value="mineral">Mineral bedding</option>
          <option value="soil">Soil patches</option>
        </select>
      </label>
      {appearance.detail.kind !== "none" && (
        <>
          <NumberControl
            label="Detail scale (per metre)"
            value={appearance.detail.scale}
            min={0.01}
            max={100}
            change={(value) => onChange(["appearance", "detail", "scale"], value, "Edit surface detail")}
          />
          <NumberControl
            label="Detail strength"
            value={appearance.detail.strength}
            change={(value) => onChange(["appearance", "detail", "strength"], value, "Edit surface detail")}
          />
        </>
      )}
      {(["weathering", "wetness", "dirt", "damage", "transmission"] as const).map((key) => (
        <NumberControl key={key} label={key} value={appearance[key]} change={(value) => change(key, value)} />
      ))}
      <NumberControl
        label="History scale"
        value={appearance.historyScale}
        min={0.01}
        max={100}
        change={(value) => change("historyScale", value)}
      />
      {appearance.family === "glass" && (
        <NumberControl
          label="Index of refraction"
          value={appearance.indexOfRefraction}
          min={1}
          max={2.5}
          change={(value) => change("indexOfRefraction", value)}
        />
      )}
      {(["dirtColor", "damageColor"] as const).map((key) => (
        <label className="row-control" key={key}>
          {key === "dirtColor" ? "Dirt color" : "Exposed damage color"}
          <input
            type="color"
            value={hex(appearance[key])}
            onChange={(event) => change(key, rgb(event.currentTarget.value))}
          />
        </label>
      ))}
      <h4>Ordered coatings</h4>
      <p className="hint">
        Later layers cover earlier layers. Surface history is applied last. Relief is in metres.
      </p>
      {appearance.layers.map((layer, index) => (
        <fieldset key={layer.id}>
          <legend>
            {index + 1}. {layer.name}
          </legend>
          <label className="row-control">
            Name
            <input
              value={layer.name}
              onChange={(event) => {
                if (event.currentTarget.value) layerChange(index, ["name"], event.currentTarget.value);
              }}
            />
          </label>
          <label className="row-control">
            <input
              type="checkbox"
              checked={layer.enabled}
              onChange={(event) => layerChange(index, ["enabled"], event.currentTarget.checked)}
            />
            Enabled
          </label>
          <label className="row-control">
            Color
            <input
              type="color"
              value={hex(layer.color)}
              onChange={(event) => layerChange(index, ["color"], rgb(event.currentTarget.value))}
            />
          </label>
          {(["roughness", "metallic", "coverage"] as const).map((key) => (
            <NumberControl
              key={key}
              label={`Layer ${index + 1} ${key}`}
              value={layer[key]}
              min={key === "roughness" ? 0.04 : 0}
              change={(value) => layerChange(index, [key], value)}
            />
          ))}
          <NumberControl
            label={`Layer ${index + 1} relief`}
            value={layer.relief}
            min={-0.02}
            max={0.02}
            step={0.001}
            change={(value) => layerChange(index, ["relief"], value)}
          />
          <MaterialMaskPanel
            mask={layer.mask}
            index={index}
            change={(key, value) => layerChange(index, ["mask", key], value)}
          />
          <button
            type="button"
            disabled={index === 0}
            onClick={() => {
              const layers = [...appearance.layers];
              [layers[index - 1], layers[index]] = [layers[index], layers[index - 1]];
              change("layers", layers);
            }}
          >
            Move earlier
          </button>
          <button
            type="button"
            disabled={index === appearance.layers.length - 1}
            onClick={() => {
              const layers = [...appearance.layers];
              [layers[index + 1], layers[index]] = [layers[index], layers[index + 1]];
              change("layers", layers);
            }}
          >
            Move later
          </button>
          <button
            type="button"
            onClick={() =>
              change(
                "layers",
                appearance.layers.filter((_, item) => item !== index),
              )
            }
          >
            Remove layer
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={appearance.layers.length >= 4}
        onClick={() => {
          let id = 1;
          while (appearance.layers.some((layer) => layer.id === `surface-${id}`)) id++;
          change("layers", [...appearance.layers, createSurfaceLayer(`surface-${id}`)]);
        }}
      >
        Add coating
      </button>
      <h4>Review</h4>
      <p className="hint">
        Compare grazing relief, low-sun transmission, and gameplay distance. Sun angle is adjusted in the
        current review stage.
      </p>
      {SURFACE_REVIEW_PRESETS.map((preset) => (
        <button
          key={preset.id}
          type="button"
          disabled={!onPreview}
          onClick={() => {
            const [x, y, z] = preset.sunDirection;
            const length = Math.hypot(x, y, z);
            onPreview?.({
              distance: preset.distance,
              sunElevation: Math.asin(y / length),
              sunAzimuth: Math.atan2(x, z),
              mode: "lit",
            });
          }}
        >
          {preset.name}
        </button>
      ))}
      {surfaceReviewWarnings(appearance).map((warning) => (
        <p className="hint" key={warning}>
          {warning}
        </p>
      ))}
    </section>
  );
}
