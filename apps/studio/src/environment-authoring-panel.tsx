import { reviewRiverChannel } from "@wrela/compiler";
import type {
  Cloudscape,
  EnvironmentDefinition,
  EnvironmentGrade,
  EnvironmentState,
  LightingRigDefinition,
  StageDefinition,
  WaterDefinition,
} from "@wrela/model";

import type { ReactNode } from "react";
import type { DomainPanelProps } from "./domain-authoring";

type Supported = EnvironmentDefinition | LightingRigDefinition | WaterDefinition | StageDefinition;
type Path = (string | number)[];
const identityGrade = (): EnvironmentGrade => ({ exposureCompensation: 0, tint: [1, 1, 1] });
const labels: Record<string, string> = {
  sunElevation: "Sun elevation (radians)",
  sunAzimuth: "Sun azimuth (radians)",
  turbidity: "Atmospheric haze",
  fogDensity: "Fog density",
  cloudCover: "Cloud cover",
  development: "Cloud vertical growth",
  storminess: "Storm depth",
  highCloudCover: "High cloud cover",
  background: "Background cloud coverage",
  base: "Cloud base (m)",
  erosion: "Edge breakup",
  startHour: "Starting local hour",
  latitude: "Latitude (degrees)",
  declination: "Seasonal solar tilt (degrees)",
  northOffset: "North rotation (radians)",
  moonIntensity: "Moonlight intensity",
  nightExposure: "Night adaptation (stops)",
  origin: "Front origin (X/Z metres)",
  direction: "Front direction (X/Z)",
  strength: "Front cover change",
  ambient: "Ambient intensity",
  sunIntensity: "Sun intensity",
  wetness: "Surface wetness",
  exposureCompensation: "Exposure compensation (stops)",
  tint: "Light tint (RGB)",
  wind: "Wind velocity (m/s)",
  velocity: "Water current (X/Z m/s)",
  radius: "Zone radius (m)",
  blendDistance: "Zone blend distance (m)",
  fogMultiplier: "Fog multiplier",
  shoreWidth: "Shore response width (m)",
  foam: "Foam coverage",
  windSpeed: "Wind speed (m/s)",
  amplitude: "Wave amplitude (m)",
  wavelength: "Dominant wavelength (m)",
  significantHeight: "Significant wave height (m)",
  peakPeriod: "Peak wave period (seconds)",
  period: "Period (seconds)",
  choppiness: "Crest sharpness",
  spread: "Directional spread",
  absorption: "Light absorption per metre (RGB)",
  scattering: "Light scattering per metre (RGB)",
  anisotropy: "Forward scattering",
  foamLifetime: "Foam residence (seconds)",
  dryingSeconds: "Bank drying time (seconds)",
  bedDetail: "Bed relief (m)",
  width: "Channel width (m)",
  depth: "Channel depth (m)",
  halfExtents: "Room half extents (m)",
  yaw: "Room rotation (radians)",
  referenceWindSpeed: "Reference wind speed (m/s)",
  waveGain: "Wave amplitude gain per m/s",
  roughnessGain: "Roughness gain per m/s",
};

/** All changes use Studio's document transaction path, including sequence edits. */
export function EnvironmentAuthoringPanel({
  document,
  onChange,
  onApply,
  onSeek,
  onPlay,
  onPreview,
}: DomainPanelProps<Supported>) {
  const edit = (path: Path, value: unknown) => onChange(path, value, "Edit environment authoring");
  const field = (value: unknown, path: Path, name: string): ReactNode => {
    const label =
      name === "direction" && typeof value === "number" ? "Direction (radians)" : (labels[name] ?? name);
    if (value && typeof value === "object" && "development" in value && "storminess" in value) {
      const cloud = value as Cloudscape;
      const forms = cloud.formations ?? [];
      return (
        <fieldset key={path.join(".")}>
          <legend>{label}</legend>
          {Object.entries(cloud)
            .filter(
              ([key]) =>
                ![
                  "formations",
                  "background",
                  "highCloudYaw",
                  "highCloudHeight",
                  "midCloudCover",
                  "midCloudHeight",
                  "midCloudYaw",
                ].includes(key),
            )
            .map(([key, item]) => field(item, [...path, key], key))}
          {field(cloud.background ?? 1, [...path, "background"], "background")}
          {field(
            cloud.highCloudYaw ?? 0.4324078,
            [...path, "highCloudYaw"],
            "Upper cloud rotation (radians)",
          )}
          {field(cloud.highCloudHeight ?? 9800, [...path, "highCloudHeight"], "High cloud altitude (m)")}
          {field(cloud.midCloudCover ?? 0, [...path, "midCloudCover"], "Middle cloud cover")}
          {field(cloud.midCloudHeight ?? 6800, [...path, "midCloudHeight"], "Middle cloud altitude (m)")}
          {field(cloud.midCloudYaw ?? -0.35, [...path, "midCloudYaw"], "Middle cloud rotation (radians)")}
          {forms.map((form, index) => (
            <fieldset key={form.id}>
              <legend>{form.id}</legend>
              <label className="row-control">
                <span>Cloud form</span>
                <select
                  aria-label={`${form.id} cloud form`}
                  value={form.kind}
                  onChange={(e) => edit([...path, "formations", index, "kind"], e.target.value)}
                >
                  <option value="tower">Rising tower</option>
                  <option value="bank">Shallow bank</option>
                  <option value="wisp">Dissipating wisp</option>
                </select>
              </label>
              {field(form.center, [...path, "formations", index, "center"], "Position (X/Z metres)")}
              {field(form.base, [...path, "formations", index, "base"], "base")}
              {field(
                form.size,
                [...path, "formations", index, "size"],
                "Half width / height / half depth (m)",
              )}
              {field(form.yaw, [...path, "formations", index, "yaw"], "Cloud rotation (radians)")}
              {field(form.density, [...path, "formations", index, "density"], "Density")}
              {field(form.erosion, [...path, "formations", index, "erosion"], "erosion")}
              {field(
                form.maturity ?? 0.45,
                [...path, "formations", index, "maturity"],
                "Maturity (growing to dissolving)",
              )}
              {field(form.shear ?? 0, [...path, "formations", index, "shear"], "Growth lean")}
              {field(form.seed, [...path, "formations", index, "seed"], "Variation seed")}
              <button
                type="button"
                onClick={() =>
                  edit(
                    [...path, "formations"],
                    forms.filter((_, i) => i !== index),
                  )
                }
              >
                Remove {form.id}
              </button>
            </fieldset>
          ))}
          {(["tower", "bank", "wisp"] as const).map((kind) => (
            <button
              type="button"
              key={kind}
              disabled={forms.length >= 4}
              onClick={() => {
                let suffix = 1;
                while (forms.some((f) => f.id === `${kind}-${suffix}`)) suffix++;
                edit(
                  [...path, "formations"],
                  [
                    ...forms,
                    {
                      id: `${kind}-${suffix}`,
                      kind,
                      center: [0, 8000],
                      base: kind === "wisp" ? 3000 : 1200,
                      size:
                        kind === "tower"
                          ? [2200, 3800, 1900]
                          : kind === "bank"
                            ? [3800, 1400, 1800]
                            : [2600, 900, 1000],
                      yaw: 0,
                      density: kind === "wisp" ? 0.6 : 1,
                      erosion: kind === "wisp" ? 0.8 : 0.3,
                      seed: suffix * 17,
                    },
                  ],
                );
              }}
            >
              Add {kind}
            </button>
          ))}
        </fieldset>
      );
    }
    if (typeof value === "number")
      return (
        <label className="row-control" key={path.join(".")}>
          <span>{label}</span>
          <input
            aria-label={label}
            type="number"
            step="any"
            value={value}
            onChange={(event) => {
              if (Number.isFinite(event.target.valueAsNumber)) edit(path, event.target.valueAsNumber);
            }}
          />
        </label>
      );
    if (Array.isArray(value))
      return (
        <fieldset key={path.join(".")}>
          <legend>{label}</legend>
          {value.map((item, index) =>
            field(
              item,
              [...path, index],
              name === "tint"
                ? ["Red", "Green", "Blue"][index]
                : ["X", value.length === 2 ? "Z" : "Y", "Z"][index],
            ),
          )}
        </fieldset>
      );
    if (value && typeof value === "object")
      return (
        <fieldset key={path.join(".")}>
          <legend>{label}</legend>
          {Object.entries(value).map(([key, item]) => field(item, [...path, key], key))}
        </fieldset>
      );
    return null;
  };
  const grade = document.kind !== "lighting" && document.kind !== "water" ? document.grade : undefined;
  const riverDiagnostics =
    document.kind === "water" && document.flow?.river ? reviewRiverChannel(document.flow.river) : [];
  return (
    <section aria-label="Environment authoring">
      <h3>
        {document.kind === "water"
          ? "Flow and shores"
          : document.kind === "lighting"
            ? "Lighting zones"
            : "Environment composition"}
      </h3>
      {(document.kind === "stage" || document.kind === "environment") &&
        (grade ? (
          <>
            {field(grade, ["grade"], "Exposure and light color")}
            <button type="button" onClick={() => edit(["grade"], undefined)}>
              Reset exposure and tint
            </button>
          </>
        ) : (
          <button type="button" onClick={() => edit(["grade"], identityGrade())}>
            Add exposure and light tint
          </button>
        ))}
      {document.kind === "environment" && (
        <>
          <p>
            The day clock moves the sun and moon. Weather keyframes independently change clouds, haze, wind,
            wetness, and light. Playback and seeking evaluate the same state.
          </p>
          {field(document.cloudCover ?? 0, ["cloudCover"], "cloudCover")}
          {document.dayCycle ? (
            <>
              {field(document.dayCycle, ["dayCycle"], "Day and night cycle")}
              <button type="button" onClick={() => edit(["dayCycle"], undefined)}>
                Remove day and night cycle
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() =>
                edit(["dayCycle"], {
                  duration: 1200,
                  startHour: 8,
                  latitude: 42,
                  declination: 8,
                  northOffset: 0,
                  moonIntensity: 0.1,
                  nightExposure: 2.25,
                })
              }
            >
              Add day and night cycle
            </button>
          )}
          {document.cloudscape ? (
            <>
              {field(document.cloudscape, ["cloudscape"], "Cloudscape")}
              {!document.cloudscape.front && (
                <button
                  type="button"
                  onClick={() =>
                    edit(["cloudscape", "front"], {
                      origin: [0, 0],
                      direction: [1, 0],
                      width: 4000,
                      strength: 0.5,
                    })
                  }
                >
                  Add moving weather front
                </button>
              )}
              {document.cloudscape.front && (
                <button type="button" onClick={() => edit(["cloudscape", "front"], undefined)}>
                  Remove weather front
                </button>
              )}
              <button type="button" onClick={() => edit(["cloudscape"], undefined)}>
                Reset cloudscape
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() =>
                onApply(
                  [
                    {
                      kind: "document.set",
                      target: document.id,
                      path: ["cloudscape"],
                      value: { development: 0.6, storminess: 0, highCloudCover: 0.2 },
                    },
                    ...(document.cloudCover === undefined
                      ? [
                          {
                            kind: "document.set" as const,
                            target: document.id,
                            path: ["cloudCover"],
                            value: 0.48,
                          },
                        ]
                      : []),
                  ],
                  "Shape cloudscape",
                )
              }
            >
              Shape cloudscape
            </button>
          )}
          {!document.sequence ? (
            <button
              type="button"
              onClick={() => {
                const state: EnvironmentState = {
                  sunElevation: document.sunElevation,
                  sunAzimuth: document.sunAzimuth,
                  turbidity: document.turbidity,
                  fogDensity: document.fogDensity,
                  cloudCover: document.cloudCover ?? 0,
                  cloudscape: document.cloudscape,
                  wind: document.wind,
                  ambient: 0.45,
                  sunIntensity: 2.5,
                  wetness: 0,
                  grade: identityGrade(),
                };
                edit(["sequence"], {
                  duration: 300,
                  loop: true,
                  interpolation: "smooth",
                  keyframes: [
                    { time: 0, state },
                    {
                      time: 150,
                      state: {
                        ...state,
                        sunElevation: 0.08,
                        sunAzimuth: document.sunAzimuth + (document.sunAzimuth > 3 ? -1 : 1),
                        fogDensity: Math.min(0.1, document.fogDensity * 2),
                        cloudCover: 0.8,
                        wind: [2, 0, 0.7],
                        wetness: 0.6,
                        grade: { exposureCompensation: 0.5, tint: [1, 0.85, 0.7] },
                      },
                    },
                  ],
                });
              }}
            >
              Create environment sequence
            </button>
          ) : (
            <>
              {field(document.sequence.duration, ["sequence", "duration"], "Duration (seconds)")}
              <label className="row-control">
                <input
                  type="checkbox"
                  checked={document.sequence.loop}
                  onChange={(event) => edit(["sequence", "loop"], event.target.checked)}
                />{" "}
                Loop
              </label>
              <label className="row-control">
                Transition{" "}
                <select
                  value={document.sequence.interpolation}
                  onChange={(event) => edit(["sequence", "interpolation"], event.target.value)}
                >
                  <option value="smooth">Smooth</option>
                  <option value="linear">Linear</option>
                </select>
              </label>
              {document.sequence.keyframes.map((keyframe, index) => (
                <details key={`${document.id}-${index}`}>
                  <summary>
                    State {index + 1} · {keyframe.time}s
                  </summary>
                  {field(keyframe.time, ["sequence", "keyframes", index, "time"], "Time (seconds)")}
                  {field(
                    document.dayCycle
                      ? Object.fromEntries(
                          Object.entries(keyframe.state).filter(
                            ([name]) => name !== "sunElevation" && name !== "sunAzimuth",
                          ),
                        )
                      : keyframe.state,
                    ["sequence", "keyframes", index, "state"],
                    "Weather and light",
                  )}
                  {!keyframe.state.cloudscape && (
                    <button
                      type="button"
                      onClick={() =>
                        edit(
                          ["sequence", "keyframes", index, "state", "cloudscape"],
                          structuredClone(
                            document.cloudscape ?? {
                              development: 0.6,
                              storminess: 0,
                              highCloudCover: 0.2,
                            },
                          ),
                        )
                      }
                    >
                      Shape clouds in this state
                    </button>
                  )}
                  {keyframe.state.cloudscape && (
                    <button
                      type="button"
                      onClick={() => edit(["sequence", "keyframes", index, "state", "cloudscape"], undefined)}
                    >
                      Use base cloudscape
                    </button>
                  )}
                  {onSeek && (
                    <button type="button" onClick={() => onSeek(keyframe.time)}>
                      Preview this state
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={(document.sequence?.keyframes.length ?? 0) <= 2}
                    onClick={() =>
                      edit(
                        ["sequence", "keyframes"],
                        document.sequence?.keyframes.filter((_, i) => i !== index),
                      )
                    }
                  >
                    Remove state
                  </button>
                </details>
              ))}
              <button
                type="button"
                disabled={document.sequence.keyframes.length >= 32}
                onClick={() => {
                  const sequence = document.sequence;
                  if (!sequence) return;
                  const candidates = sequence.keyframes
                    .map((key, index) => ({
                      key,
                      time:
                        key.time +
                        ((sequence.keyframes[index + 1]?.time ?? sequence.duration) - key.time) / 2,
                      gap: (sequence.keyframes[index + 1]?.time ?? sequence.duration) - key.time,
                    }))
                    .sort((a, b) => b.gap - a.gap);
                  const candidate = candidates[0];
                  if (candidate.gap > 0)
                    edit(
                      ["sequence", "keyframes"],
                      [
                        ...sequence.keyframes,
                        { time: candidate.time, state: structuredClone(candidate.key.state) },
                      ].sort((a, b) => a.time - b.time),
                    );
                }}
              >
                Insert state
              </button>
              {onPlay && (
                <button type="button" onClick={() => onPlay()}>
                  Play environment
                </button>
              )}
              <button type="button" onClick={() => edit(["sequence"], undefined)}>
                Remove sequence
              </button>
            </>
          )}
        </>
      )}
      {document.kind === "lighting" && (
        <>
          <p>
            Spherical zones or rotated room volumes blend the viewed environment as the camera enters them.
            Higher priority zones apply last. Point lights remain spatial lights.
          </p>
          {(document.zones ?? []).map((zone, index) => (
            <details key={zone.id}>
              <summary>{zone.name}</summary>
              <label className="row-control">
                Name{" "}
                <input
                  value={zone.name}
                  onChange={(event) => edit(["zones", index, "name"], event.target.value)}
                />
              </label>
              {Object.entries(zone)
                .filter(([key]) => key !== "id" && key !== "name" && !(key === "radius" && zone.box))
                .map(([key, value]) => field(value, ["zones", index, key], key))}
              <button
                type="button"
                onClick={() =>
                  edit(
                    ["zones", index, "box"],
                    zone.box
                      ? undefined
                      : { halfExtents: [zone.radius, zone.radius / 2, zone.radius], yaw: 0 },
                  )
                }
              >
                {zone.box ? "Use spherical zone" : "Use room volume"}
              </button>
              <button
                type="button"
                onClick={() =>
                  edit(
                    ["zones"],
                    (document.zones ?? []).filter((_, i) => i !== index),
                  )
                }
              >
                Remove zone
              </button>
            </details>
          ))}
          <button
            type="button"
            disabled={(document.zones?.length ?? 0) >= 16}
            onClick={() =>
              edit(
                ["zones"],
                [
                  ...(document.zones ?? []),
                  {
                    id: `zone-${crypto.randomUUID().slice(0, 8)}`,
                    name: "Interior light",
                    center: [0, 2, 0],
                    radius: 10,
                    blendDistance: 2,
                    priority: 0,
                    ambient: 0.2,
                    sunIntensity: 0.25,
                    fogMultiplier: 0.5,
                    grade: identityGrade(),
                  },
                ],
              )
            }
          >
            Add lighting zone
          </button>
        </>
      )}
      {document.kind === "water" && (
        <>
          <p>
            Water can combine a directional wave spectrum with a bounded riverbed and fluid simulation. River
            point levels set the downhill profile; basins form lakes and obstacles shape currents.
          </p>
          {document.spectrum ? (
            <details open>
              <summary>Wave spectrum</summary>
              <label className="row-control">
                <span>Sea state</span>
                <select
                  aria-label="Sea state mode"
                  value={document.spectrum.mode ?? "artistic"}
                  onChange={(e) =>
                    edit(["spectrum"], {
                      ...document.spectrum,
                      mode: e.target.value,
                      ...(e.target.value === "sea-state"
                        ? {
                            significantHeight: document.spectrum?.significantHeight ?? 1.4,
                            peakPeriod: document.spectrum?.peakPeriod ?? 4,
                          }
                        : {}),
                    })
                  }
                >
                  <option value="artistic">Art-directed waves</option>
                  <option value="sea-state">Height and period</option>
                </select>
              </label>
              {Object.entries(document.spectrum)
                .filter(
                  ([key]) =>
                    key !== "mode" &&
                    key !== "swell" &&
                    (document.spectrum?.mode !== "sea-state" || !["amplitude", "wavelength"].includes(key)),
                )
                .map(([key, value]) => field(value, ["spectrum", key], key))}
              {document.spectrum.swell ? (
                <fieldset>
                  <legend>Independent swell</legend>
                  {field(document.spectrum.swell, ["spectrum", "swell"], "Swell")}
                  <button type="button" onClick={() => edit(["spectrum", "swell"], undefined)}>
                    Remove swell
                  </button>
                </fieldset>
              ) : (
                <button
                  type="button"
                  onClick={() =>
                    edit(["spectrum", "swell"], { height: 0.5, period: 6, direction: 0.2, spread: 0.15 })
                  }
                >
                  Add independent swell
                </button>
              )}
              <button type="button" onClick={() => edit(["spectrum"], undefined)}>
                Remove spectrum
              </button>
            </details>
          ) : (
            <button
              type="button"
              onClick={() =>
                edit(["spectrum"], {
                  seed: 17,
                  windSpeed: 6,
                  direction: 0.6,
                  amplitude: 0.3,
                  wavelength: 12,
                  spread: 0.7,
                  choppiness: 0.35,
                })
              }
            >
              Add ocean waves
            </button>
          )}
          {document.optics ? (
            <details>
              <summary>Water clarity and foam</summary>
              {field(
                {
                  ...document.optics,
                  scattering: document.optics.scattering ?? [0.012, 0.025, 0.03],
                  anisotropy: document.optics.anisotropy ?? 0.35,
                  foamLifetime: document.optics.foamLifetime ?? 4,
                },
                ["optics"],
                "Optics",
              )}
              <button
                type="button"
                onClick={() =>
                  edit(["optics"], {
                    ...document.optics,
                    absorption: [0.23, 0.075, 0.05],
                    scattering: [0.008, 0.018, 0.022],
                    anisotropy: 0.35,
                  })
                }
              >
                Clear stream
              </button>
              <button
                type="button"
                onClick={() =>
                  edit(["optics"], {
                    ...document.optics,
                    absorption: [0.18, 0.06, 0.035],
                    scattering: [0.035, 0.06, 0.055],
                    anisotropy: 0.55,
                  })
                }
              >
                Mineral-rich water
              </button>
            </details>
          ) : (
            <button
              type="button"
              onClick={() =>
                edit(["optics"], { absorption: [0.18, 0.055, 0.025], caustics: 0.35, foam: 0.6 })
              }
            >
              Adjust clarity and foam
            </button>
          )}
          <details>
            <summary>Local breakers and waterfalls</summary>
            {(document.effects ?? []).map((effect, index) => (
              <fieldset key={effect.id}>
                <legend>{effect.id}</legend>
                <select
                  aria-label={`${effect.id} effect type`}
                  value={effect.kind}
                  onChange={(e) => edit(["effects", index, "kind"], e.target.value)}
                >
                  <option value="breaker">Breaking wave</option>
                  <option value="waterfall">Waterfall sheet</option>
                </select>
                {Object.entries(effect)
                  .filter(([key]) => key !== "id" && key !== "kind")
                  .map(([key, value]) => field(value, ["effects", index, key], key))}
                <button
                  type="button"
                  onClick={() =>
                    edit(
                      ["effects"],
                      document.effects?.filter((_, i) => i !== index),
                    )
                  }
                >
                  Remove effect
                </button>
              </fieldset>
            ))}
            <button
              type="button"
              disabled={(document.effects?.length ?? 0) >= 8}
              onClick={() =>
                edit(
                  ["effects"],
                  [
                    ...(document.effects ?? []),
                    {
                      id: `breaker-${crypto.randomUUID().slice(0, 8)}`,
                      kind: "breaker",
                      start: [-3, 0],
                      end: [3, 0],
                      height: 0.7,
                      width: 3,
                      period: 6,
                      phase: 0,
                    },
                  ],
                )
              }
            >
              Add local effect
            </button>
          </details>
          {document.domain ? (
            <details open>
              <summary>Riverbed and local fluid</summary>
              {field(document.domain.min, ["domain", "min"], "Origin")}
              {field(document.domain.size, ["domain", "size"], "Extent")}
              <label className="row-control">
                <span>Grid resolution</span>
                <select
                  aria-label="Water grid resolution"
                  value={document.domain.resolution}
                  onChange={(e) => edit(["domain", "resolution"], Number(e.target.value))}
                >
                  {[32, 64, 128].map((n) => (
                    <option key={n} value={n}>
                      {n} × {n}
                    </option>
                  ))}
                </select>
              </label>
              <label className="row-control">
                <span>Simulate flow</span>
                <input
                  type="checkbox"
                  checked={document.domain.simulate}
                  onChange={(e) => edit(["domain", "simulate"], e.target.checked)}
                />
              </label>
              <label className="row-control">
                <span>Bank detail</span>
                <select
                  aria-label="Water bank detail"
                  value={document.domain.renderResolution ?? document.domain.resolution}
                  onChange={(e) => edit(["domain", "renderResolution"], Number(e.target.value))}
                >
                  <option value={64}>Standard</option>
                  <option value={128}>Fine</option>
                  <option value={256}>Close-up</option>
                </select>
              </label>
              {field(document.domain.bedDetail ?? 0, ["domain", "bedDetail"], "bedDetail")}
              {field(document.domain.dryingSeconds ?? 18, ["domain", "dryingSeconds"], "dryingSeconds")}
              {field(document.domain.friction, ["domain", "friction"], "Friction")}
              {field(document.domain.bankHeight, ["domain", "bankHeight"], "Bank height")}
              {field(document.domain.bankWidth, ["domain", "bankWidth"], "Bank width")}
              {(["basins", "obstacles", "sources"] as const).map((key) => (
                <fieldset key={key}>
                  <legend>{key}</legend>
                  {document.domain?.[key].map((value, index) => (
                    <details key={index}>
                      <summary>
                        {key} {index + 1}
                      </summary>
                      {field(value, ["domain", key, index], key)}
                      <button
                        type="button"
                        onClick={() =>
                          edit(
                            ["domain", key],
                            document.domain?.[key].filter((_, i) => i !== index),
                          )
                        }
                      >
                        Remove
                      </button>
                    </details>
                  ))}
                  <button
                    type="button"
                    disabled={
                      (document.domain?.[key].length ?? 0) >= { basins: 8, obstacles: 64, sources: 16 }[key]
                    }
                    onClick={() =>
                      edit(
                        ["domain", key],
                        [
                          ...(document.domain?.[key] ?? []),
                          key === "basins"
                            ? { center: [0, 0], radii: [6, 6], depth: 1.5 }
                            : key === "obstacles"
                              ? { center: [0, 0], radius: 1, height: 1.5 }
                              : { position: [0, 0], radius: 1, rate: 0.2, velocity: [0, 0] },
                        ],
                      )
                    }
                  >
                    Add{" "}
                    {key === "basins"
                      ? "lake basin"
                      : key === "obstacles"
                        ? "bed obstacle"
                        : "source or drain"}
                  </button>
                </fieldset>
              ))}
              <button type="button" onClick={() => edit(["domain"], undefined)}>
                Remove riverbed and simulation
              </button>
            </details>
          ) : (
            <button
              type="button"
              onClick={() =>
                edit(["domain"], {
                  min: [-16, -16],
                  size: [32, 32],
                  resolution: 64,
                  basins: [{ center: [0, 0], radii: [10, 10], depth: 2 }],
                  obstacles: [],
                  sources: [],
                  bankHeight: 1.2,
                  bankWidth: 2,
                  simulate: true,
                  friction: 0.12,
                })
              }
            >
              Add lake and fluid simulation
            </button>
          )}
          {!document.flow ? (
            <button type="button" onClick={() => edit(["flow"], { velocity: [1, 0] })}>
              Add water current
            </button>
          ) : (
            <>
              {field(document.flow.velocity, ["flow", "velocity"], "velocity")}
              {document.flow.weatherResponse ? (
                <>
                  {field(
                    document.flow.weatherResponse,
                    ["flow", "weatherResponse"],
                    "Water response to wind",
                  )}
                  <button type="button" onClick={() => edit(["flow", "weatherResponse"], undefined)}>
                    Remove weather response
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  onClick={() =>
                    edit(["flow", "weatherResponse"], {
                      referenceWindSpeed: 0,
                      waveGain: 0.12,
                      roughnessGain: 0.012,
                    })
                  }
                >
                  Coordinate waves with weather
                </button>
              )}
              {!document.flow.river ? (
                <button
                  type="button"
                  onClick={() =>
                    edit(["flow", "river"], {
                      points: [
                        { position: [-12, 0], width: 6, depth: 2 },
                        { position: [12, 0], width: 6, depth: 2 },
                      ],
                      shoreWidth: 1,
                      foam: 0.25,
                    })
                  }
                >
                  Create river channel
                </button>
              ) : (
                <>
                  <section aria-label="River layout review" aria-live="polite">
                    {riverDiagnostics.length ? (
                      <ul>
                        {riverDiagnostics.map((diagnostic, index) => (
                          <li key={`${diagnostic.code}-${index}`}>
                            {diagnostic.severity}: {diagnostic.message}
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p>Channel banks are continuous and do not cross.</p>
                    )}
                  </section>
                  {field(document.flow.river.shoreWidth, ["flow", "river", "shoreWidth"], "shoreWidth")}
                  {field(document.flow.river.foam, ["flow", "river", "foam"], "foam")}
                  {document.flow.river.points.map((point, index) => (
                    <details key={`${document.id}-river-${index}`}>
                      <summary>River point {index + 1}</summary>
                      {field(point, ["flow", "river", "points", index], "Channel section")}
                      {point.level === undefined && (
                        <button
                          type="button"
                          onClick={() => edit(["flow", "river", "points", index, "level"], document.level)}
                        >
                          Set surface elevation
                        </button>
                      )}
                      <button
                        type="button"
                        disabled={(document.flow?.river?.points.length ?? 0) <= 2}
                        onClick={() =>
                          edit(
                            ["flow", "river", "points"],
                            document.flow?.river?.points.filter((_, i) => i !== index),
                          )
                        }
                      >
                        Remove river point
                      </button>
                    </details>
                  ))}
                  <button
                    type="button"
                    disabled={document.flow.river.points.length >= 32}
                    onClick={() => {
                      const points = document.flow?.river?.points;
                      if (!points) return;
                      const last = points[points.length - 1],
                        previous = points[points.length - 2];
                      edit(
                        ["flow", "river", "points"],
                        [
                          ...points,
                          {
                            ...last,
                            position: [
                              last.position[0] * 2 - previous.position[0],
                              last.position[1] * 2 - previous.position[1],
                            ],
                          },
                        ],
                      );
                    }}
                  >
                    Extend river
                  </button>
                  <button type="button" onClick={() => edit(["flow", "river"], undefined)}>
                    Remove channel boundary
                  </button>
                </>
              )}
              <button type="button" onClick={() => edit(["flow"], undefined)}>
                Remove flow authoring
              </button>
            </>
          )}
        </>
      )}
      {onPreview && (
        <div>
          {[5, 25, 100].map((distance) => (
            <button type="button" key={distance} onClick={() => onPreview({ distance })}>
              Review at {distance}m
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
