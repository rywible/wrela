import type { MaterialDefinition } from "@wrela/model";
import type { DomainPanelProps } from "./domain-authoring";
import { MaterialNumberControl, materialColorFromHex, materialColorToHex } from "./material-controls";

export function MaterialEmissionPanel({
  document,
  onChange,
}: Pick<DomainPanelProps<MaterialDefinition>, "document" | "onChange">) {
  const emission = document.emission ?? { color: [1, 1, 1] as [number, number, number], intensity: 0 };
  return (
    <fieldset>
      <legend>Emitted light</legend>
      <MaterialNumberControl
        label="Light strength"
        value={emission.intensity}
        min={0}
        max={10000}
        step={0.1}
        change={(intensity) => onChange(["emission"], { ...emission, intensity }, "Edit emitted light")}
      />
      <label className="row-control">
        Light color
        <input
          aria-label="Emitted light color"
          type="color"
          value={materialColorToHex(emission.color)}
          onChange={(event) =>
            onChange(
              ["emission"],
              { ...emission, color: materialColorFromHex(event.currentTarget.value) },
              "Edit emitted light color",
            )
          }
        />
      </label>
    </fieldset>
  );
}
