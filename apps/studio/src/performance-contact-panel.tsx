import type { CharacterDefinition, Joint, Motion, PerformanceClip, Vec3 } from "@wrela/model";

import {
  type PerformanceContactDiagnostic,
  type PerformanceContactReview,
  reviewPerformanceContacts,
} from "@wrela/runtime";
import { useState } from "react";

type Props = {
  document: CharacterDefinition;
  motion: Motion;
  joint: Joint;
  time: number;
  translation: Vec3;
  clip?: PerformanceClip;
  editClip: (edit: (value: PerformanceClip) => void) => void;
  contacts: PerformanceContactDiagnostic[];
  scrub: (seconds: number) => void;
};

export function PerformanceContactPanel({
  document,
  motion,
  joint,
  time,
  translation,
  clip,
  editClip,
  contacts,
  scrub,
}: Props) {
  const [review, setReview] = useState<{
    source: CharacterDefinition;
    motion: string;
    contacts: PerformanceContactReview[];
  }>();
  const current = review?.source === document && review.motion === motion.id;
  return (
    <details>
      <summary>Interaction and contacts</summary>
      <p>Alignment uses the translation fields above as a model-space contact target.</p>
      <button
        type="button"
        onClick={() =>
          editClip((entry) => {
            let index = 1;
            while (entry.alignments.some((alignment) => alignment.id === `alignment-${index}`)) index++;
            entry.alignments.push({
              id: `alignment-${index}`,
              joint: joint.id,
              time,
              target: [...translation],
              blendIn: 0.2,
              blendOut: 0.2,
              weight: 1,
            });
          })
        }
      >
        Align joint at cursor
      </button>
      {clip?.alignments.map((alignment, index) => (
        <fieldset key={alignment.id}>
          <legend>
            {alignment.joint} · {alignment.id}
          </legend>
          <button type="button" onClick={() => scrub(alignment.time)}>
            Go to contact
          </button>
          {(["time", "blendIn", "blendOut", "weight"] as const).map((field) => (
            <label className="row-control" key={field}>
              {
                {
                  time: "Contact time (seconds)",
                  blendIn: "Approach (seconds)",
                  blendOut: "Release (seconds)",
                  weight: "Strength",
                }[field]
              }
              <input
                type="number"
                min={field === "blendIn" || field === "blendOut" ? 0.001 : 0}
                max={field === "time" ? motion.duration : field === "weight" ? 1 : 10}
                step={0.01}
                value={alignment[field]}
                onChange={(event) =>
                  editClip((entry) => {
                    entry.alignments[index][field] = Number(event.target.value);
                  })
                }
              />
            </label>
          ))}
          {([0, 1, 2] as const).map((axis) => (
            <label className="row-control" key={axis}>
              Target {["X", "Y", "Z"][axis]} (metres)
              <input
                type="number"
                step={0.01}
                value={alignment.target[axis]}
                onChange={(event) =>
                  editClip((entry) => {
                    entry.alignments[index].target[axis] = Number(event.target.value);
                  })
                }
              />
            </label>
          ))}
          <button
            type="button"
            onClick={() =>
              editClip((entry) => {
                entry.alignments.splice(index, 1);
              })
            }
          >
            Remove alignment
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        disabled={time >= motion.duration}
        onClick={() =>
          editClip((entry) => {
            entry.contacts.push({
              joint: joint.id,
              start: time,
              end: Math.min(motion.duration, time + 0.2),
              groundHeight: 0,
              tolerance: 0.02,
            });
          })
        }
      >
        Mark grounded contact
      </button>
      {clip?.contacts.map((contact, index) => (
        <fieldset key={`${contact.joint}:${index}`}>
          <legend>{contact.joint}</legend>
          {(["start", "end", "groundHeight", "tolerance"] as const).map((field) => (
            <label className="row-control" key={field}>
              {
                {
                  start: "Start (seconds)",
                  end: "End (seconds)",
                  groundHeight: "Ground height (metres)",
                  tolerance: "Height tolerance (metres)",
                }[field]
              }
              <input
                type="number"
                min={field === "groundHeight" ? undefined : field === "tolerance" ? 0.001 : 0}
                max={
                  field === "start" || field === "end"
                    ? motion.duration
                    : field === "tolerance"
                      ? 1
                      : undefined
                }
                step={0.01}
                value={contact[field]}
                onChange={(event) =>
                  editClip((entry) => {
                    entry.contacts[index][field] = Number(event.target.value);
                  })
                }
              />
            </label>
          ))}
          <button
            type="button"
            onClick={() =>
              editClip((entry) => {
                entry.contacts.splice(index, 1);
              })
            }
          >
            Remove contact
          </button>
        </fieldset>
      ))}
      {contacts.map((contact, index) => (
        <p key={`${contact.joint}:${index}`}>
          {contact.joint}: {contact.valid ? "Contact stable" : "Contact needs review"}; height{" "}
          {contact.heightError.toFixed(3)} m, slide {contact.slideSpeed.toFixed(3)} m/s
        </p>
      ))}
      <button
        type="button"
        disabled={!clip?.contacts.length}
        onClick={() =>
          setReview({
            source: document,
            motion: motion.id,
            contacts: reviewPerformanceContacts(
              document.joints,
              document.motions,
              document.performance,
              motion.id,
            ),
          })
        }
      >
        Review every contact window
      </button>
      {review && !current && <p>Performance changed; run contact review again.</p>}
      {current &&
        review.contacts.map((contact, index) => (
          <div key={`${contact.joint}:${index}`}>
            <p>
              {contact.joint} · {contact.start.toFixed(2)}–{contact.end.toFixed(2)} s:{" "}
              {contact.valid ? "Stable" : "Needs review"} · maximum height{" "}
              {(contact.maximumHeightError * 100).toFixed(1)} cm · slide{" "}
              {contact.maximumSlideSpeed.toFixed(3)} m/s · {contact.samples} samples
            </p>
            <button type="button" onClick={() => scrub(contact.worstTime)}>
              Inspect largest error
            </button>
          </div>
        ))}
      <p>
        Authored contact review measures the pose before procedural IK and physics. Use physical motion review
        below to judge the solved result.
      </p>
    </details>
  );
}
