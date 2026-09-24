import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import { instantiateEditRecipe, promoteEditRecipe } from "./edit-recipe";
import { experimentAuthoring } from "./experiments";
import { type AuthoringReview, compareSilhouettes } from "./review-contract";
import { AuthoringSession } from "./session";
import { bundleWork, restoreWorkBundle } from "./work-bundle";
import {
  adoptWork,
  appendWorkEvent,
  attachWorkReview,
  createWorkSession,
  decideWork,
  proposeWork,
  verifyWorkSession,
} from "./work-session";

test("progressive discovery stays compact and only returns the requested operation schema", () => {
  const session = new AuthoringSession(referenceProject());
  const result = session.discover();
  expect(JSON.stringify(result).length).toBeLessThan(15000);
  expect(result.schema).toBeUndefined();
  expect(session.discover({ operation: "field.update" }).schema).toBeDefined();
  expect(
    session
      .discover({ target: "polar-bunny", search: "pose" })
      .operations.every((o) => /pose/i.test(o.description + o.kind)),
  ).toBe(true);
  const page = session.inspectFields({ target: "polar-bunny", path: ["field", "nodes"], limit: 2 });
  expect(page.entries).toHaveLength(2);
  expect(page.nextOffset).toBe(2);
  expect(() => session.inspectFields({ target: "polar-bunny", path: ["__proto__"] })).toThrow();
  expect(() => session.inspectFields({ target: "missing" })).toThrow();
});
function setup() {
  const project = referenceProject();
  let work = createWorkSession(project, {
    id: "recolor",
    brief: "Change roughness while keeping the silhouette source",
    constraints: [{ id: "shape", kind: "source", target: "polar-bunny", path: ["field"] }],
  });
  work = proposeWork(work, "matte", [
    { kind: "document.set", target: "snow-fur", path: ["roughness"], value: 0.8 },
  ]);
  const p = work.proposals[0];
  const report: AuthoringReview = {
    version: 1,
    baselineKey: work.baselineKey,
    candidateKey: p.candidateKey,
    constraintsKey: contentKey(work.constraints),
    evaluator: "test",
    createdAt: new Date().toISOString(),
    durationMs: 0,
    results: [
      { id: "shape", status: "passed", scope: "authored source", measured: { equal: true }, evidence: [] },
    ],
  };
  return { project, work, report };
}
test("portable proposals survive restart and require complete source-bound reviews for adoption", () => {
  const { project, work, report } = setup();
  expect(() => adoptWork(work, "matte", new AuthoringSession(project))).toThrow("Every declared");
  expect(() => attachWorkReview(work, "matte", { ...report, results: [] })).toThrow("every constraint");
  expect(() => attachWorkReview(work, "matte", { ...report, candidateKey: "stale" })).toThrow(
    "different source",
  );
  const reviewed = attachWorkReview(JSON.parse(JSON.stringify(work)), "matte", report);
  const blocked = attachWorkReview(work, "matte", {
    ...report,
    results: [{ ...report.results[0], status: "unmeasured" }],
  });
  expect(() => adoptWork(blocked, "matte", new AuthoringSession(project))).toThrow();
  const session = new AuthoringSession(project);
  adoptWork(reviewed, "matte", session);
  expect(contentKey(session.getSnapshot().project)).toBe(work.proposals[0].candidateKey);
  expect(() => adoptWork(reviewed, "matte", session)).toThrow("source changed");
  const corrupted = structuredClone(reviewed);
  corrupted.proposals[0].candidateKey = "tampered";
  expect(() => verifyWorkSession(corrupted)).toThrow();
});
test("learned edits retain acceptance provenance, bounded parameters, and demand fresh review", () => {
  const { project, work, report } = setup();
  const input = {
    id: "matte-coat",
    description: "Roughness edit",
    parameters: [
      {
        name: "roughness",
        units: "unit interval",
        description: "Coat roughness",
        min: 0.5,
        max: 1,
        default: 0.8,
        bindings: [{ operation: 0, path: ["value"] }],
      },
    ],
  };
  expect(() => promoteEditRecipe(work, "matte", input)).toThrow("acceptance");
  const accepted = decideWork(attachWorkReview(work, "matte", report), "matte", {
    reviewer: "artist",
    verdict: "accepted",
    reason: "Reviewed under three lights",
  });
  const recipe = promoteEditRecipe(accepted, "matte", input);
  const next = instantiateEditRecipe(project, recipe, { roughness: 0.65 });
  expect(next.acceptance).toBe("unreviewed");
  expect(next.operations[0]).toMatchObject({ value: 0.65 });
  expect(() => instantiateEditRecipe(project, recipe, { roughness: 0.1 })).toThrow("range");
  const corrupted = structuredClone(recipe);
  corrupted.parameters[0].default = 0.6;
  expect(() => instantiateEditRecipe(project, corrupted)).toThrow("accepted default");
  expect(() =>
    promoteEditRecipe(
      decideWork(work, "matte", { reviewer: "artist", verdict: "accepted", reason: "Looks good" }),
      "matte",
      input,
    ),
  ).toThrow("passing result constraints");
});
test("portable work includes nested evidence, verifies its bytes, and relocates references", async () => {
  const { work } = setup();
  const image = "work-artifact:image/capture.png",
    report = "work-artifact:report/review.json";
  const values = new Map<string, Blob | object>([
    [image, new Blob([new Uint8Array([1, 2, 3])], { type: "image/png" })],
    [report, { evidence: [image] }],
  ]);
  const withEvidence = appendWorkEvent(work, {
    id: "evidence",
    at: new Date().toISOString(),
    kind: "review",
    actor: "test",
    summary: "Retain capture",
    evidence: [report],
  });
  const bundle = await bundleWork(
    withEvidence,
    (s) => s.startsWith("work-artifact:"),
    async (s) => values.get(s)!,
  );
  expect(bundle.artifacts).toHaveLength(2);
  const dangling = appendWorkEvent(work, {
    id: "external",
    at: new Date().toISOString(),
    kind: "review",
    actor: "test",
    summary: "External evidence",
    evidence: ["/outside/report.json"],
  });
  await expect(
    bundleWork(
      dangling,
      (s) => s.startsWith("work-artifact:"),
      async (s) => values.get(s) as Blob,
    ),
  ).rejects.toThrow("Attach local evidence");
  const restored = new Map<string, Blob | object>();
  const result = await restoreWorkBundle(bundle, async (name, value) => {
    const key = `work-artifact:restored/${name}`;
    restored.set(key, value);
    return key;
  });
  expect(result.events.at(-1)?.evidence).toEqual(["work-artifact:restored/review.json"]);
  expect(restored.get("work-artifact:restored/review.json")).toEqual({
    evidence: ["work-artifact:restored/capture.png"],
  });
  expect(await (restored.get("work-artifact:restored/capture.png") as Blob).arrayBuffer()).toEqual(
    new Uint8Array([1, 2, 3]).buffer,
  );
  const corrupt = structuredClone(bundle);
  corrupt.artifacts[0].data = btoa("corrupted");
  await expect(restoreWorkBundle(corrupt, async () => "unused")).rejects.toThrow("checksum");
  const missing = structuredClone(bundle);
  missing.artifacts.pop();
  await expect(restoreWorkBundle(missing, async () => "unused")).rejects.toThrow("missing");
});
test("empty silhouettes cannot pass; local changes are measured only in the protected region", () => {
  expect(
    compareSilhouettes(new Uint8Array(4), new Uint8Array(4), 2, 2, [0, 0, 1, 1]).changedFraction,
  ).toBeNull();
  const a = new Uint8Array([1, 0, 0, 0]),
    b = new Uint8Array([1, 0, 1, 0]);
  expect(compareSilhouettes(a, b, 2, 2, [0, 0, 1, 1]).changedFraction).toBe(0.5);
  expect(compareSilhouettes(a, b, 2, 2, [0, 0, 1, 0.5]).changedFraction).toBe(0);
});
test("sensitivity experiments retain invalid trials and never mutate their source", async () => {
  const project = referenceProject(),
    key = contentKey(project);
  const result = await experimentAuthoring(
    project,
    [{ kind: "source", id: "m", target: "snow-fur", path: ["roughness"] }],
    {
      controls: [
        { target: "snow-fur", path: ["roughness"], delta: 0.01, units: "unit interval" },
        { target: "snow-fur", path: ["roughness"], delta: 10, units: "unit interval" },
      ],
      objective: { constraint: "m", metric: "value", direction: "maximize" },
    },
    async (baseline, candidate, constraints) => ({
      version: 1,
      baselineKey: contentKey(baseline),
      candidateKey: contentKey(candidate),
      constraintsKey: contentKey(constraints),
      evaluator: "test",
      createdAt: new Date().toISOString(),
      durationMs: 0,
      results: [
        {
          id: "m",
          status: "passed",
          scope: "test",
          measured: {
            value: (candidate.documents.find((d) => d.id === "snow-fur") as { roughness: number }).roughness,
          },
          evidence: [],
        },
      ],
    }),
  );
  expect(result.trials).toHaveLength(2);
  expect(result.trials[1].error).not.toBeNull();
  expect(contentKey(project)).toBe(key);
});
