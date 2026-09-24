import { compileCharacter } from "@wrela/compiler";
import { type CharacterDefinition, contentKey, type Motion } from "@wrela/model";

import { auditCreatureMotion } from "@wrela/runtime";
import { useState } from "react";

type Report = Awaited<ReturnType<typeof auditCreatureMotion>>;

/** A bounded playtest of the authored clip through the same physics and contact solver as the game. */
export function PerformancePhysicalReview({
  character,
  motion,
}: {
  character: CharacterDefinition;
  motion: Motion;
}) {
  const [report, setReport] = useState<Report>();
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const current = report?.sourceKey === contentKey(character) && report.motion === motion.id;
  if (!character.creature) return null;
  return (
    <details>
      <summary>Physical motion review</summary>
      <button
        type="button"
        disabled={running}
        onClick={async () => {
          setRunning(true);
          setError("");
          try {
            const artifact = compileCharacter(character, "interactive");
            setReport(await auditCreatureMotion(character, artifact, motion.id));
          } catch (reason) {
            setError(reason instanceof Error ? reason.message : String(reason));
          } finally {
            setRunning(false);
          }
        }}
      >
        {running ? "Reviewing motion…" : "Playtest contact and root motion"}
      </button>
      {error && <p role="alert">{error}</p>}
      {report && !current && <p>Motion changed; run the review again.</p>}
      {report && current && (
        <div>
          <p>
            Maximum planted slip: {(report.maximumPlantedSlip * 100).toFixed(1)} cm · contact error:{" "}
            {(report.maximumContactResidual * 100).toFixed(1)} cm · {report.samples.length} runtime frames
          </p>
          {report.plantedContacts.length === 0 && (
            <p>No planted contact windows are authored for this clip.</p>
          )}
          {report.plantedContacts.map((contact) => (
            <p key={contact.id}>
              {contact.joint}, {contact.start.toFixed(2)}–{contact.end.toFixed(2)} s:{" "}
              {(contact.maximumSlip * 100).toFixed(1)} cm across {contact.samples} frames
            </p>
          ))}
          {report.samples.some((sample) => sample.issues.length > 0) && (
            <p role="alert">The physical solver reported contact conflicts or unavailable constraints.</p>
          )}
        </div>
      )}
    </details>
  );
}
