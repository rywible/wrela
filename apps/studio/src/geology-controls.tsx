import { useEffect, useState } from "react";

export function NumberEdit({
  label,
  value,
  min,
  max,
  onChange,
  step = "any",
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number | "any";
  onChange(value: number): void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  return (
    <label className="row-control">
      <span>{label}</span>
      <input
        type="number"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={(event) => {
          const next = event.currentTarget.valueAsNumber;
          if (event.currentTarget.validity.valid && Number.isFinite(next)) {
            if (value !== next) onChange(next);
          } else setDraft(String(value));
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
    </label>
  );
}
export function nextId(prefix: string, entries: { id: string }[]): string {
  let n = 1;
  while (entries.some((entry) => entry.id === `${prefix}-${n}`)) n++;
  return `${prefix}-${n}`;
}
