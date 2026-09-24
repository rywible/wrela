import type { GeologicalProfile, Landform } from "@wrela/model";

import { NumberEdit } from "./geology-controls";

/** Replaces the parent feature when optional controls are enabled, so source
 * edits work with the same atomic document commands as every other geology tool. */
export function GeologyProfilePanel({
  feature,
  onChange,
}: {
  feature: Landform;
  onChange: (value: Landform) => void;
}) {
  if (feature.kind === "ridge") return null;
  const profile = feature.profile;
  const variation = profile?.variation;
  const set = (value: GeologicalProfile | undefined) => onChange({ ...feature, profile: value });
  const edit = (label: string, key: string, value: number, min: number, max: number) => (
    <NumberEdit
      label={label}
      value={value}
      min={min}
      max={max}
      onChange={(next) => {
        if (profile) set({ ...profile, [key]: next });
      }}
    />
  );
  return (
    <details>
      <summary>{feature.kind === "drainage" ? "Channel and banks" : "Cliff profile"}</summary>
      <button
        type="button"
        onClick={() =>
          set(
            profile
              ? undefined
              : feature.kind === "drainage"
                ? { kind: "river", bedFraction: 0.2, bankFraction: 0.45, shoulderDepth: 0.15, asymmetry: 0 }
                : { kind: "cliff", faceFraction: 0.12, toeHeight: 0.2, crestFraction: 0.7 },
          )
        }
      >
        {profile ? "Use simple falloff" : "Shape cross section"}
      </button>
      {profile?.kind === "river" && (
        <>
          {edit(
            "Flat bed width share",
            "bedFraction",
            profile.bedFraction,
            0.05,
            Math.min(0.6, 0.95 - profile.bankFraction),
          )}
          {edit(
            "Sloping bank width share",
            "bankFraction",
            profile.bankFraction,
            0.1,
            Math.min(0.7, 0.95 - profile.bedFraction),
          )}
          {edit("Floodplain depth share", "shoulderDepth", profile.shoulderDepth, 0, 0.6)}
          {edit("Bank asymmetry", "asymmetry", profile.asymmetry, -0.6, 0.6)}
        </>
      )}
      {profile?.kind === "cliff" && (
        <>
          {edit("Face width share", "faceFraction", profile.faceFraction, 0.03, 0.4)}
          {edit("Talus toe height share", "toeHeight", profile.toeHeight, 0, 0.6)}
          {edit("Crest width share", "crestFraction", profile.crestFraction, 0.45, 0.9)}
        </>
      )}
      {profile && (
        <>
          <button
            type="button"
            onClick={() =>
              set({
                ...profile,
                variation: variation ? undefined : { seed: 71, amplitude: 0.12, wavelength: 20 },
              })
            }
          >
            {variation ? "Remove bank variation" : "Add bank variation"}
          </button>
          {variation && (
            <>
              <NumberEdit
                label="Variation seed"
                value={variation.seed}
                min={0}
                max={65535}
                step={1}
                onChange={(seed) => set({ ...profile, variation: { ...variation, seed } })}
              />
              <NumberEdit
                label="Variation amount"
                value={variation.amplitude}
                min={0}
                max={0.25}
                onChange={(amplitude) => set({ ...profile, variation: { ...variation, amplitude } })}
              />
              <NumberEdit
                label="Variation distance"
                value={variation.wavelength}
                min={2}
                max={500}
                onChange={(wavelength) => set({ ...profile, variation: { ...variation, wavelength } })}
              />
            </>
          )}
        </>
      )}
    </details>
  );
}
