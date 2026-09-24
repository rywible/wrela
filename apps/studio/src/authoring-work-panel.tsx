import type { WorkSession } from "@wrela/authoring";
import { contentKey } from "@wrela/model";

import { useEffect, useState } from "react";
import { AuthoringStudyPanel } from "./authoring-study-panel";
import { downloadBlob, studio } from "./controller";
import { HeroAuthoringPanel } from "./hero-authoring-panel";

function WorkEvidence({ reference }: { reference: string }) {
  const [artifact, setArtifact] = useState<{ url: string; image: boolean }>();
  const [error, setError] = useState("");
  useEffect(() => {
    let cancelled = false,
      url: string | undefined;
    void studio.work.store
      .artifact(reference)
      .then((value) => {
        const blob =
          value instanceof Blob
            ? value
            : new Blob([JSON.stringify(value, null, 2)], { type: "application/json" });
        if (cancelled) return;
        url = URL.createObjectURL(blob);
        setArtifact({ url, image: blob.type.startsWith("image/") });
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [reference]);
  const name = reference.split("/").at(-1) ?? "evidence";
  if (error)
    return (
      <p role="alert">
        {name}: {error}
      </p>
    );
  if (!artifact) return <span>Loading evidence…</span>;
  return (
    <a href={artifact.url} download={name}>
      {artifact.image ? (
        <img src={artifact.url} alt={name} style={{ maxWidth: "100%", maxHeight: 240 }} />
      ) : (
        name
      )}
    </a>
  );
}

export function AuthoringWorkPanel({ operations = "[]" }: { operations?: string }) {
  const [items, setItems] = useState<{ id: string; brief: string; key: string }[]>([]);
  const [work, setWork] = useState<WorkSession>();
  const [brief, setBrief] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState("");
  const [constraints, setConstraints] = useState("[]");
  const refresh = async () => setItems(await studio.work.list());
  useEffect(() => {
    void refresh().catch((e) => setError(String(e)));
  }, []);
  const run = async (action: () => Promise<unknown>, reload = true) => {
    setBusy(true);
    setError("");
    try {
      const result = await action();
      if (result && typeof result === "object" && "receiptPending" in result && result.receiptPending)
        setError("The edit was accepted. Saving needs to finish; retry Adopt to recover the same request.");
      if (result && typeof result === "object" && "packagingPending" in result && result.packagingPending)
        setError(
          "Finishing is incomplete. The source may already be accepted; retry Finish to recover publication and export.",
        );
      await refresh();
      if (reload && work) setWork(await studio.work.export(work.id));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section aria-label="Authoring work sessions" className="work-sessions">
      <h3>Work sessions</h3>
      <p>
        Keep a brief, alternatives, review evidence and feedback together. Sessions are saved on this device.
      </p>
      {work?.publication?.state === "pending" && (
        <output>
          Adoption is pending. Choose Adopt again to finish saving or recover the saved request.
        </output>
      )}
      <label>
        Open session{" "}
        <select
          disabled={busy}
          value={work?.id ?? ""}
          onChange={(event) => {
            const id = event.target.value;
            if (!id) setWork(undefined);
            else void run(async () => setWork(await studio.work.export(id)), false);
          }}
        >
          <option value="">Choose a session</option>
          {items.map((item) => (
            <option key={item.id} value={item.id}>
              {item.id}
            </option>
          ))}
        </select>
      </label>
      <label>
        New brief{" "}
        <textarea
          value={brief}
          onChange={(event) => setBrief(event.target.value)}
          placeholder="Describe the result and what must be preserved"
        />
      </label>
      <details>
        <summary>Result constraints</summary>
        <label>
          Constraints JSON{" "}
          <textarea
            value={constraints}
            spellCheck={false}
            onChange={(event) => setConstraints(event.target.value)}
          />
        </label>
        <p>Record the dimensions, source, silhouette, route or frame budgets this work must preserve.</p>
      </details>
      <button
        type="button"
        disabled={busy || !brief.trim()}
        onClick={() =>
          void run(async () => {
            const id = `work-${crypto.randomUUID().slice(0, 8)}`;
            await studio.work.create({ id, brief: brief.trim(), constraints: JSON.parse(constraints) });
            setWork(await studio.work.export(id));
            setBrief("");
          }, false)
        }
      >
        Start work session
      </button>
      {work && (
        <>
          <p>{work.brief}</p>
          <p>
            {work.constraints.length} constraints · {work.proposals.length} alternatives ·{" "}
            {work.events.length} retained events
          </p>
          <button
            type="button"
            disabled={busy || operations.trim() === "[]" || !operations.trim()}
            onClick={() =>
              void run(() =>
                studio.work.propose(
                  work.id,
                  contentKey(work),
                  `alternative-${crypto.randomUUID().slice(0, 8)}`,
                  JSON.parse(operations),
                ),
              )
            }
          >
            Keep operations below as an alternative
          </button>
          <AuthoringStudyPanel key={work.id} work={work} busy={busy} run={run} />
          <HeroAuthoringPanel key={`hero-${work.id}`} work={work} busy={busy} run={run} />
          {work.proposals.map((proposal) => (
            <article key={proposal.id}>
              <strong>{proposal.id}</strong> <span>{proposal.status}</span>
              <ul>
                {proposal.reviews.at(-1)?.results.map((r) => (
                  <li key={r.id}>
                    {r.id}: {r.status}
                    {r.reason ? ` — ${r.reason}` : ""}
                  </li>
                )) ?? <li>Not reviewed</li>}
              </ul>
              <div className="work-evidence">
                {proposal.reviews
                  .at(-1)
                  ?.results.flatMap((r) => r.evidence)
                  .filter((ref) => ref.startsWith("work-artifact:"))
                  .map((reference) => (
                    <WorkEvidence key={reference} reference={reference} />
                  ))}
              </div>
              <div className="button-row">
                <button
                  type="button"
                  disabled={busy || proposal.decision?.verdict !== "accepted"}
                  onClick={() =>
                    void run(() =>
                      studio.work.fast.remember(work.id, proposal.id, `semantic-${crypto.randomUUID()}`),
                    )
                  }
                >
                  Remember accepted study
                </button>
                <button
                  type="button"
                  disabled={
                    busy ||
                    proposal.decision?.verdict !== "accepted" ||
                    !proposal.batch.operations.some(
                      (op) => op.kind === "document.create" && op.document.kind === "object",
                    )
                  }
                  onClick={() =>
                    void run(() => {
                      const created = proposal.batch.operations.find(
                        (op) => op.kind === "document.create" && op.document.kind === "object",
                      );
                      if (created?.kind !== "document.create") throw Error("No created asset");
                      return studio.work.fast.rememberCreation(work.id, proposal.id, created.document.id);
                    })
                  }
                >
                  Remember accepted construction
                </button>
                <button
                  type="button"
                  disabled={busy || proposal.status !== "proposed"}
                  onClick={() => void run(() => studio.work.review(work.id, contentKey(work), proposal.id))}
                >
                  Run constraints
                </button>
                <button
                  type="button"
                  disabled={busy || proposal.status !== "proposed"}
                  onClick={() =>
                    void run(async () => {
                      const target = proposal.batch.operations.find(
                        (op) =>
                          "target" in op &&
                          work.baseline.documents.some(
                            (d) =>
                              d.id === op.target &&
                              ["object", "character", "world", "terrain", "vegetation"].includes(d.kind),
                          ),
                      );
                      await studio.work.fast.evaluate(work.id, {
                        expectedKey: contentKey(work),
                        proposal: proposal.id,
                        target: target && "target" in target ? target.target : work.baseline.entry,
                      });
                    })
                  }
                >
                  Review views
                </button>
                <button
                  type="button"
                  disabled={busy || proposal.status === "rejected"}
                  onClick={() =>
                    void run(async () => {
                      const result = await studio.work.fast.finish(work.id, {
                        expectedKey: contentKey(work),
                        proposal: proposal.id,
                      });
                      if ("bundle" in result && result.bundle)
                        downloadBlob(
                          new Blob([JSON.stringify(result.bundle)], { type: "application/json" }),
                          `${work.id}-handoff.json`,
                        );
                      return result;
                    })
                  }
                >
                  Finish and export
                </button>
                <button
                  type="button"
                  disabled={busy || proposal.status !== "proposed"}
                  onClick={() => void run(() => studio.work.adopt(work.id, contentKey(work), proposal.id))}
                >
                  Adopt edit
                </button>
                <button
                  type="button"
                  disabled={busy || proposal.status !== "proposed" || !feedback.trim()}
                  onClick={() =>
                    void run(() =>
                      studio.work.decide(work.id, contentKey(work), proposal.id, {
                        reviewer: "human",
                        verdict: "accepted",
                        reason: feedback.trim(),
                      }),
                    )
                  }
                >
                  Accept design with feedback
                </button>
                <button
                  type="button"
                  disabled={busy || proposal.decision?.verdict !== "accepted"}
                  onClick={() =>
                    void run(() =>
                      studio.work.recipe(work.id, proposal.id, {
                        id: `recipe-${crypto.randomUUID().slice(0, 8)}`,
                        description: work.brief.slice(0, 2000),
                        parameters: [],
                      }),
                    )
                  }
                >
                  Retain reusable edit
                </button>
              </div>
            </article>
          ))}
          <label>
            Feedback{" "}
            <textarea
              value={feedback}
              onChange={(event) => setFeedback(event.target.value)}
              placeholder="Describe what worked, what failed, and what the next author should do"
            />
          </label>
          <button
            type="button"
            disabled={busy || !feedback.trim()}
            onClick={() =>
              void run(async () => {
                await studio.work.feedback(work.id, contentKey(work), {
                  id: `feedback-${crypto.randomUUID()}`,
                  at: new Date().toISOString(),
                  kind: "feedback",
                  actor: "human",
                  summary: feedback,
                  humanInterventions: 1,
                });
                setFeedback("");
              })
            }
          >
            Keep feedback
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              void run(async () =>
                downloadBlob(
                  new Blob([JSON.stringify(await studio.work.backup(work.id))], { type: "application/json" }),
                  `${work.id}.json`,
                ),
              )
            }
          >
            Export work and evidence
          </button>
        </>
      )}
      <label>
        Import work record{" "}
        <input
          type="file"
          accept="application/json,.json"
          disabled={busy}
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file)
              void run(async () => {
                if (file.size > 64 * 1024 * 1024) throw Error("Work record exceeds 64 MiB");
                const parsed = JSON.parse(await file.text());
                const input = parsed.work && !parsed.format && !parsed.version ? parsed.work : parsed;
                if (input.format === "wrela-work-bundle") {
                  const restored = await studio.work.restore(input);
                  setWork(await studio.work.export(restored.id));
                } else {
                  await studio.work.import(input);
                  setWork(await studio.work.export(input.id));
                }
              }, false);
          }}
        />
      </label>
      {error && <p role="alert">{error}</p>}
      {busy && <output>Preparing work…</output>}
    </section>
  );
}
