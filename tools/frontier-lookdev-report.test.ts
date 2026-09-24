import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type FrontierPolicy, selectRenderFrontier } from "@wrela/examples/render-frontier";
import { readReliefFrontierEvidence, verifyFrontierProvenance } from "./frontier-lookdev-input";
import { renderFrontierReport, runFrontierLookdevReport } from "./frontier-lookdev-report";

async function fixture(directory: string) {
  const product = (kind: "direct-mesh" | "parametric-mesh") => ({
    key: kind,
    sourceKey: "one-shape",
    algorithmVersion: "test-1",
    domainKey: "one-domain",
    kind,
    errors: [
      kind === "direct-mesh"
        ? { kind: "numeric-bound", metric: "silhouette", maximum: 0, domain: "one-domain" }
        : {
            kind: "real-bound",
            metric: "silhouette",
            maximum: 0.02,
            domain: "one-domain",
            numericError: "unknown",
          },
    ],
    projectedGeometryErrors: [{ metric: "silhouette", maximumPixels: kind === "direct-mesh" ? 0 : 0.13 }],
  });
  const capture = {
    reports: ["automatic", "forced-near"].map((choice) => ({
      choice,
      variant: "physical-relief",
      shot: "far-clay",
      camera: { position: [0, 1, 10], target: [0, 0, 0], fov: 40 },
      geometry: [
        {
          selected: { key: choice === "automatic" ? "parametric-mesh" : "direct-mesh" },
          candidates: [product("direct-mesh"), product("parametric-mesh")],
        },
      ],
      timing: { preparationMs: 2, gpuP50Ms: 1, gpuP95Ms: 1.1, cpuP95Ms: 0.5, gpuSamples: 12 },
      measurements: { adapter: "test adapter", gpuBytes: 1000, outputResolution: [960, 640] },
      completeness: { complete: true },
      failures: [],
    })),
  };
  const path = join(directory, "capture.json");
  await Bun.write(path, JSON.stringify(capture));
  await Bun.write(
    join(directory, "source-relief.json"),
    JSON.stringify({ documents: [{ kind: "lighting", ambient: 1 }] }),
  );
  return path;
}
test("native matched lookdev import retains real-bound uncertainty and cannot manufacture a certified frontier", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-frontier-"));
  try {
    const path = await fixture(directory),
      policy: FrontierPolicy = {
        budgets: { "silhouette-max-pixels": 0.25, "linear-radiance-rms": 0.02 },
        costAxes: ["gpuMs", "cpuMs", "ownedBytes"],
      };
    const input = await readReliefFrontierEvidence(path, policy);
    expect(input.candidates).toHaveLength(2);
    expect(input.candidates[0].quality[0].evidence.kind).toBe("real-bound");
    expect(input.candidates[0].cost.observation?.gpuP95Ms).toBe(1.1);
    expect(input.candidates[0].cost.uncertainty.gpuMs).toBeNull();
    expect(input.candidates[0].domain.referenceKey).toBe(input.candidates[1].domain.referenceKey);
    const result = selectRenderFrontier(input.candidates, input.policy);
    expect(result.groups).toHaveLength(1);
    expect(result.groups[0].frontier).toHaveLength(0);
    expect(result.decisions.every((decision) => decision.status === "inadmissible")).toBe(true);
    expect(
      (await verifyFrontierProvenance(input.candidates, directory)).every(
        (artifact) => artifact.sha256.length === 64,
      ),
    ).toBe(true);
    await Bun.write(join(directory, "policy.json"), JSON.stringify(policy));
    const output = join(directory, "report");
    const saved = await runFrontierLookdevReport([
      `--relief=${path}`,
      `--policy=${join(directory, "policy.json")}`,
      `--out=${output}`,
    ]);
    expect(saved.inadmissible).toEqual(["automatic", "forced-near"]);
    const retained = JSON.parse(await readFile(join(output, "frontier.json"), "utf8"));
    expect(retained.artifacts).toHaveLength(2);
    expect(retained.visualAcceptance).toBe("unreviewed");
    const replay = await runFrontierLookdevReport([
      `--input=${join(output, "input.json")}`,
      `--out=${join(directory, "replay")}`,
    ]);
    expect(replay.inadmissible).toEqual(saved.inadmissible);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("frontier reports escape source labels and artifact pointers must resolve", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-frontier-"));
  try {
    const path = await fixture(directory),
      input = await readReliefFrontierEvidence(path, {
        budgets: { "silhouette-max-pixels": 0.25 },
        costAxes: ["ownedBytes"],
      });
    input.candidates[0].label = '<script>alert("bad")</script>';
    const html = renderFrontierReport(input, selectRenderFrontier(input.candidates, input.policy));
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain("<script>");
    input.candidates[0].provenance[0].pointer = "/missing";
    await expect(verifyFrontierProvenance(input.candidates, directory)).rejects.toThrow("does not exist");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
