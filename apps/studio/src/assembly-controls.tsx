import type { Vec3 } from "@wrela/model";

export function NumberControl({
  label,
  value,
  change,
  min,
  max,
  step = 0.05,
}: {
  label: string;
  value: number;
  change: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <label className="row-control">
      {label}
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(event) => {
          const value = event.currentTarget.valueAsNumber;
          if (
            Number.isFinite(value) &&
            (min === undefined || value >= min) &&
            (max === undefined || value <= max)
          )
            change(value);
        }}
      />
    </label>
  );
}
export function VectorControl({
  label,
  value,
  change,
}: {
  label: string;
  value: Vec3;
  change: (value: Vec3) => void;
}) {
  return (
    <fieldset>
      <legend>{label}</legend>
      {value.map((v, axis) => (
        <NumberControl
          key={axis}
          label={["X", "Y", "Z"][axis]}
          value={v}
          change={(next) => change(value.map((n, i) => (i === axis ? next : n)) as Vec3)}
        />
      ))}
    </fieldset>
  );
}
