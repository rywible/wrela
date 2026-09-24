import type { AssemblyPart } from "@wrela/model";

import { NumberControl, VectorControl } from "./assembly-controls";

/** Joint editing stays separate from geometry, sockets and material authoring. */
export function AssemblyJointPanel({
  part,
  parts,
  change,
}: {
  part: AssemblyPart;
  parts: readonly AssemblyPart[];
  change: (joint: AssemblyPart["joint"]) => void;
}) {
  const joint = part.joint;
  return (
    <fieldset>
      <legend>Mechanical motion</legend>
      <label className="row-control">
        Joint
        <select
          value={joint?.kind ?? "none"}
          disabled={!!part.mate}
          onChange={(event) =>
            change(
              event.target.value === "none"
                ? undefined
                : {
                    kind: event.target.value as "hinge" | "slider",
                    axis: [0, 1, 0],
                    pivot: [0, 0, 0],
                    minimum: 0,
                    maximum: event.target.value === "hinge" ? Math.PI / 2 : 1,
                    value: 0,
                  },
            )
          }
        >
          <option value="none">Fixed</option>
          <option value="hinge">Hinge</option>
          <option value="slider">Slider</option>
        </select>
      </label>
      {joint && (
        <>
          <VectorControl
            label="Joint axis"
            value={joint.axis}
            change={(axis) => change({ ...joint, axis })}
          />
          <VectorControl
            label="Joint pivot"
            value={joint.pivot}
            change={(pivot) => change({ ...joint, pivot })}
          />
          <NumberControl
            label="Joint minimum"
            value={joint.minimum}
            max={joint.maximum}
            change={(minimum) => change({ ...joint, minimum, value: Math.max(minimum, joint.value) })}
          />
          <NumberControl
            label="Joint maximum"
            value={joint.maximum}
            min={joint.minimum}
            change={(maximum) => change({ ...joint, maximum, value: Math.min(maximum, joint.value) })}
          />
          {!joint.link && (
            <label className="row-control">
              Articulation
              <input
                type="range"
                min={joint.minimum}
                max={joint.maximum}
                step={0.01}
                value={joint.value}
                onChange={(event) => change({ ...joint, value: Number(event.target.value) })}
              />
            </label>
          )}
          <label className="row-control">
            Motion source
            <select
              value={joint.link?.part ?? ""}
              onChange={(event) =>
                change({
                  ...joint,
                  drive: event.target.value ? undefined : joint.drive,
                  link: event.target.value ? { part: event.target.value, ratio: 1, offset: 0 } : undefined,
                })
              }
            >
              <option value="">Independent</option>
              {parts
                .filter((candidate) => candidate.id !== part.id && candidate.joint)
                .map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {candidate.name}
                  </option>
                ))}
            </select>
          </label>
          {joint.link ? (
            <>
              <p>
                Follows the source joint × ratio + offset, clamped to these limits. Ratios can reverse motion
                or convert radians to metres.
              </p>
              <NumberControl
                label="Transmission ratio"
                value={joint.link.ratio}
                min={-1000}
                max={1000}
                change={(ratio) => {
                  if (joint.link) change({ ...joint, link: { ...joint.link, ratio } });
                }}
              />
              <NumberControl
                label="Transmission offset"
                value={joint.link.offset}
                change={(offset) => {
                  if (joint.link) change({ ...joint, link: { ...joint.link, offset } });
                }}
              />
            </>
          ) : (
            <>
              <label className="row-control">
                <input
                  type="checkbox"
                  checked={!!joint.drive}
                  onChange={(event) =>
                    change({ ...joint, drive: event.target.checked ? { period: 4, phase: 0 } : undefined })
                  }
                />
                Drive during playback
              </label>
              {joint.drive && (
                <>
                  <NumberControl
                    label="Drive period (seconds)"
                    value={joint.drive.period}
                    min={0.1}
                    max={3600}
                    change={(period) => {
                      if (joint.drive) change({ ...joint, drive: { ...joint.drive, period } });
                    }}
                  />
                  <NumberControl
                    label="Drive phase"
                    value={joint.drive.phase}
                    min={0}
                    max={1}
                    step={0.01}
                    change={(phase) => {
                      if (joint.drive) change({ ...joint, drive: { ...joint.drive, phase } });
                    }}
                  />
                </>
              )}
            </>
          )}
        </>
      )}
    </fieldset>
  );
}
