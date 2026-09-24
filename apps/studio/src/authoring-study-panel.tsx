import {
  authoringExemplars,
  domainControls,
  domainFor,
  domainQuality,
  type WorkSession,
} from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { useState } from "react";
import { studio } from "./controller";

export function AuthoringStudyPanel({
  work,
  busy,
  run,
}: {
  work: WorkSession;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
}) {
  const supported = work.baseline.documents.filter((d) => domainFor(work.baseline, d.id));
  const preferred = work.constraints.find((c) => supported.some((d) => d.id === c.target))?.target;
  const [target, setTarget] = useState(preferred ?? supported[0]?.id ?? "");
  const [conditions, setConditions] = useState({
    exposure: 0.7,
    moisture: 0.35,
    maturity: 0.8,
    variation: 0.45,
  });
  const [direction, setDirection] = useState("");
  const [exemplar, setExemplar] = useState("");
  const [budgetSeconds, setBudgetSeconds] = useState<15 | 30 | 60>(30);
  const domain = domainFor(work.baseline, target);
  if (!domain) return null;
  const quality = domainQuality(domain);
  return (
    <details>
      <summary>Explore reviewed alternatives</summary>
      <label>
        Starting point{" "}
        <select value={exemplar} disabled={busy} onChange={(e) => setExemplar(e.target.value)}>
          <option value="">Match my brief</option>
          {authoringExemplars
            .filter((e) => e.domain === domain)
            .map((e) => (
              <option key={e.id} value={e.id}>
                {e.title}
              </option>
            ))}
        </select>
      </label>
      <label>
        Exploration budget{" "}
        <select
          value={budgetSeconds}
          disabled={busy}
          onChange={(e) => setBudgetSeconds(Number(e.target.value) as 15 | 30 | 60)}
        >
          {[15, 30, 60].map((seconds) => (
            <option key={seconds} value={seconds}>
              {seconds} seconds
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        disabled={busy}
        onClick={() =>
          void run(() =>
            studio.work.fast.job(work.id, {
              target,
              expectedKey: contentKey(work),
              brief: direction || work.brief,
              exemplar: exemplar || undefined,
              budgetSeconds,
            }),
          )
        }
      >
        Create from my brief
      </button>
      <p>
        Curated starting points with a matched comparison gallery. The first review always completes; further
        alternatives stop when the exploration budget is reached.
      </p>
      <label>
        Subject{" "}
        <select
          value={target}
          disabled={busy}
          onChange={(e) => {
            setTarget(e.target.value);
            setExemplar("");
          }}
        >
          {supported.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        Visual direction{" "}
        <textarea
          value={direction}
          onChange={(e) => setDirection(e.target.value)}
          placeholder={quality.direction}
        />
      </label>
      <p>Direction guides your comparison. The controls below determine the generated alternatives.</p>
      {domainControls.map((c) => (
        <label key={c.name}>
          {c.name}: {conditions[c.name].toFixed(2)}
          <input
            type="range"
            min={c.min}
            max={c.max}
            step="0.05"
            value={conditions[c.name]}
            disabled={busy}
            onChange={(e) => setConditions({ ...conditions, [c.name]: Number(e.target.value) })}
          />
          <small>{c.description}</small>
        </label>
      ))}
      <ul>
        {quality.criteria.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <button
        type="button"
        disabled={busy}
        onClick={() =>
          void run(() =>
            studio.work.fast.study(work.id, {
              id: `study-${crypto.randomUUID()}`,
              expectedKey: contentKey(work),
              brief: {
                domain,
                target,
                conditions,
                quality: { ...quality, direction: direction || quality.direction },
              },
              candidates: 3,
            }),
          )
        }
      >
        Generate and review three alternatives
      </button>
      <p>
        Matched neutral and grazing lighting. Source stays unchanged until you finish a selected alternative.
        The comparison gallery appears in retained evidence.
      </p>
    </details>
  );
}
