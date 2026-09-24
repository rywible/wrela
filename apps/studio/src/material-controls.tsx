import type { Vec3 } from "@wrela/model";

const srgb = (linear: number) => (linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055);
const linear = (value: number) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
/** Source colors are linear; the browser color picker edits display-encoded sRGB. */
export const materialColorToHex = (color: Vec3) =>
  `#${color
    .map((value) =>
      Math.round(srgb(Math.max(0, Math.min(1, value))) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
export const materialColorFromHex = (color: string): Vec3 =>
  [1, 3, 5].map((offset) => linear(Number.parseInt(color.slice(offset, offset + 2), 16) / 255)) as Vec3;

export function MaterialNumberControl({
  label,
  value,
  min = 0,
  max = 1,
  step = 0.01,
  change,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  change: (value: number) => void;
}) {
  return (
    <label className="row-control">
      {label}
      <input
        aria-label={label}
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => {
          const next = event.currentTarget.valueAsNumber;
          if (Number.isFinite(next) && next >= min && next <= max) change(next);
        }}
      />
    </label>
  );
}
