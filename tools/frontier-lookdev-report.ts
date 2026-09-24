import { mkdir, readFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { type RenderFrontier, selectRenderFrontier } from "@wrela/examples/render-frontier";
import {
  type FrontierEvidenceBundle,
  frontierEvidenceBundleSchema,
  frontierPolicySchema,
  readReliefFrontierEvidence,
  verifyFrontierProvenance,
} from "./frontier-lookdev-input";

const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char] ?? char,
  );
const number = (value: number | null | undefined) =>
  value === null || value === undefined
    ? "unknown"
    : value.toLocaleString("en-US", { maximumFractionDigits: 5 });
export function renderFrontierReport(input: FrontierEvidenceBundle, result: RenderFrontier) {
  const body = result.groups
    .map((group) => {
      const candidates = input.candidates.filter(
        (candidate) =>
          result.decisions.find((decision) => decision.id === candidate.id)?.domainKey === group.key,
      );
      return `<section><h2>${escapeHtml(group.domain.fixture)} · ${group.domain.width}×${group.domain.height}</h2><p>${escapeHtml(group.domain.adapter)} · ${escapeHtml(group.domain.browser)} · ${escapeHtml(group.domain.kernelVersion)}</p><p>Frontier: ${escapeHtml(group.frontier.join(", ") || "none with admissible evidence")}</p><table><thead><tr><th>Candidate</th><th>Status</th><th>GPU p95 ms</th><th>CPU ${escapeHtml(group.domain.cpuScope)} ms</th><th>Owned bytes (${escapeHtml(group.domain.memoryScope)})</th><th>Decision</th></tr></thead><tbody>${candidates
        .map((candidate) => {
          const decision = result.decisions.find((entry) => entry.id === candidate.id);
          if (!decision) throw Error("Missing frontier decision");
          const explanation = [
            ...decision.reasons.map((reason) => `${reason.code}: ${reason.message}`),
            ...decision.dominatedBy.map(
              (other) => `Dominated by ${other.id}; strictly better: ${other.strictlyBetter.join(", ")}`,
            ),
            ...decision.tiedWith.map((other) => `Equivalent observed coordinates: ${other}`),
          ];
          return `<tr><td>${escapeHtml(candidate.label)}</td><td>${escapeHtml(decision.status)}</td><td>${number(candidate.cost.observation?.gpuP95Ms)}</td><td>${number(candidate.cost.cpuMs)}</td><td>${number(candidate.cost.ownedBytes)}</td><td>${escapeHtml(explanation.join("; ") || "No candidate dominates under these explicit coordinates")}</td></tr>`;
        })
        .join(
          "",
        )}</tbody></table><details><summary>Quality intervals and explicit budgets</summary><pre>${escapeHtml(JSON.stringify({ budgets: result.policy.budgets, coordinates: result.decisions.filter((decision) => decision.domainKey === group.key).map(({ id, coordinates }) => ({ id, coordinates })) }, null, 2))}</pre></details></section>`;
    })
    .join("");
  return `<!doctype html><meta charset="utf-8"><title>Measured rendering frontier</title><style>body{font:15px system-ui;color:#e7e9eb;background:#15191f;margin:32px}h1{font-size:28px}section{margin:30px 0;padding:18px;background:#20262e}table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:10px;border-bottom:1px solid #39424d;vertical-align:top}th{color:#a8c9d9}pre{white-space:pre-wrap;overflow-wrap:anywhere}p{max-width:100ch}a{color:#abd4e8}</style><h1>Measured rendering frontier</h1><p>${escapeHtml(result.guarantee)}</p><p>Numbers in the table are retained observations. They do not become admissible until all requested quality budgets and uncertainty requirements pass. CPU and GPU work are separate axes and are never added into a frame-time prediction.</p><p><a href="frontier.json">Decisions and evidence</a> · <a href="input.json">Normalized measurement input</a></p>${input.notes.map((note) => `<p>${escapeHtml(note)}</p>`).join("")}${body}<section><h2>Why comparisons remain inconclusive</h2>${result.incomparable.map((pair) => `<p><strong>${escapeHtml(pair.left)} / ${escapeHtml(pair.right)}</strong>: ${escapeHtml(pair.reasons.join("; "))}</p>`).join("") || "<p>All pairs were comparable and strictly ordered.</p>"}</section>`;
}

export async function writeFrontierLookdevReport(
  input: FrontierEvidenceBundle,
  output: string,
  base: string,
) {
  const parsed = frontierEvidenceBundleSchema.parse(input);
  const artifacts = await verifyFrontierProvenance(parsed.candidates, base);
  const result = selectRenderFrontier(parsed.candidates, parsed.policy);
  const directory = resolve(output);
  await mkdir(directory, { recursive: true });
  // Persist absolute artifact paths so normalized input remains replayable from its new location.
  for (const candidate of parsed.candidates)
    for (const source of candidate.provenance) {
      source.artifact = resolve(base, source.artifact);
    }
  await Bun.write(join(directory, "input.json"), JSON.stringify(parsed, null, 2));
  await Bun.write(
    join(directory, "frontier.json"),
    JSON.stringify(
      {
        schemaVersion: 1,
        result,
        candidates: parsed.candidates,
        notes: parsed.notes,
        artifacts: artifacts.map((artifact) => ({
          ...artifact,
          relativePath: relative(directory, artifact.path),
        })),
        visualAcceptance: "unreviewed",
      },
      null,
      2,
    ),
  );
  await Bun.write(join(directory, "index.html"), renderFrontierReport(parsed, result));
  return {
    directory,
    groups: result.groups.length,
    candidates: result.decisions.length,
    frontier: result.decisions
      .filter((decision) => decision.status === "frontier")
      .map((decision) => decision.id),
    inadmissible: result.decisions
      .filter((decision) => decision.status === "inadmissible")
      .map((decision) => decision.id),
  };
}
export async function runFrontierLookdevReport(args: string[]) {
  const argument = (name: string) =>
    args.find((value) => value.startsWith(`--${name}=`))?.slice(name.length + 3);
  const path = argument("input"),
    relief = argument("relief"),
    policyPath = argument("policy"),
    output = argument("out") ?? "output/frontier-lookdev";
  if (Number(Boolean(path)) + Number(Boolean(relief)) !== 1)
    throw Error(
      "Use --input=<matched evidence bundle.json> or --relief=<capture.json> --policy=<explicit budgets.json>, with optional --out=<directory>",
    );
  if (path) {
    const input = frontierEvidenceBundleSchema.parse(JSON.parse(await readFile(path, "utf8")));
    return writeFrontierLookdevReport(input, output, dirname(resolve(path)));
  }
  if (!relief || !policyPath) throw Error("Native relief import requires an explicit --policy JSON file");
  const policy = frontierPolicySchema.parse(JSON.parse(await readFile(policyPath, "utf8")));
  const input = await readReliefFrontierEvidence(relief, policy);
  return writeFrontierLookdevReport(input, output, dirname(resolve(relief)));
}
if (import.meta.main) console.log(JSON.stringify(await runFrontierLookdevReport(process.argv.slice(2))));
