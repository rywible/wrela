/** Capture success never stands in for artistic acceptance. Keep the evidence and judgment separate. */
export const AUTHORING_STUDIES = [
  {
    id: "creatures",
    name: "Creatures and characters",
    tool: "tools/character-lookdev.ts",
    args: ["--fitting"],
    criteria: [
      "Coherent anatomy and landmarks",
      "Close-up skin, clothing and attachments",
      "Silhouette and pose readability",
    ],
  },
  {
    id: "assembly",
    name: "Props, architecture and machinery",
    tool: "tools/lookdev.ts",
    args: ["--study=architecture"],
    criteria: [
      "Exact dimensions and functional openings",
      "Convincing profiles, bevels and joints",
      "Material separation and surface history",
    ],
  },
  {
    id: "vegetation",
    name: "Vegetation",
    tool: "tools/vegetation-lookdev.ts",
    args: [],
    criteria: [
      "Believable branch hierarchy and foliage",
      "Species, age and damage variation",
      "Near, distant and moving stand readability",
    ],
  },
  {
    id: "terrain",
    name: "Terrain and geology",
    tool: "tools/terrain-lookdev.ts",
    args: [],
    criteria: [
      "Directed landforms and geological structure",
      "Passable corridors and cave openings",
      "Near and distant terrain continuity",
    ],
  },
  {
    id: "materials",
    name: "Materials and surface appearance",
    tool: "tools/material-lookdev.ts",
    args: ["--families"],
    followups: ["tools/foliage-lookdev.ts", "tools/surface-relief-lookdev.ts"],
    criteria: [
      "Distinct substance response under varied light",
      "Plausible layering, scale and weathering",
      "Stable detail at gameplay distance",
      "Actual bark and stone relief in clay and silhouette",
      "Thin leaf reflection, transmission and shadow occlusion",
    ],
  },
  {
    id: "world",
    name: "World and level composition",
    tool: "tools/world-traversal-lookdev.ts",
    args: ["--include-stills", "--frames=24"],
    criteria: [
      "Readable landmarks and deliberate sightlines",
      "Coherent paths, rooms and encounters",
      "Grounded assemblies and plant communities",
    ],
  },
  {
    id: "performance",
    name: "Animation and physical behavior",
    tool: "tools/performance-lookdev.ts",
    args: ["--frames=16"],
    criteria: [
      "Weight, contact and coherent deformation",
      "Timing, transitions and interaction alignment",
      "Readable facial and gameplay performance",
    ],
  },
  {
    id: "environment",
    name: "Lighting, atmosphere and water",
    tool: "tools/environment-lookdev.ts",
    args: [],
    criteria: [
      "Composed lighting and exposure",
      "Coherent time and weather transitions",
      "Believable flow, shores and environmental response",
    ],
  },
] as const;
export type AuthoringStudyId = (typeof AUTHORING_STUDIES)[number]["id"];
export type StudyEvidence = {
  id: AuthoringStudyId;
  exitCode: number;
  durationSeconds: number;
  artifacts: string[];
  log: string;
  visualAcceptance: "unreviewed" | "needs-work" | "accepted";
  observations: string[];
};
export type AuthoringLookdevReport = {
  sourceFingerprint: string;
  startedAt: string;
  studies: StudyEvidence[];
};
export function authoringAcceptance(report: AuthoringLookdevReport) {
  const missing = AUTHORING_STUDIES.filter((study) => !report.studies.some((r) => r.id === study.id)).map(
    (s) => s.id,
  );
  const failed = report.studies
    .filter((s) => s.exitCode !== 0 || !s.artifacts.some((a) => a.endsWith(".png")))
    .map((s) => s.id);
  const pending = report.studies.filter((s) => s.visualAcceptance !== "accepted").map((s) => s.id);
  return {
    missing,
    failed,
    pending,
    complete: missing.length === 0 && failed.length === 0,
    accepted: missing.length === 0 && failed.length === 0 && pending.length === 0,
  };
}
const escapeHtml = (s: string) =>
  s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
const url = (path: string) => path.split("/").map(encodeURIComponent).join("/");
export function renderAuthoringLookdev(report: AuthoringLookdevReport): string {
  const status = authoringAcceptance(report);
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Wrela authoring review</title><style>
  :root{color-scheme:dark;font-family:system-ui,sans-serif;background:#121613;color:#e6ece6}body{max-width:1440px;margin:0 auto;padding:36px}header{border-bottom:1px solid #3a463e;padding-bottom:24px}h1{font-size:36px;margin:0 0 12px}p{line-height:1.6;color:#b9c5bc}nav{display:flex;flex-wrap:wrap;gap:16px;margin:24px 0}a{color:#c2d9c9}section{margin:48px 0}h2{font-size:24px}small{overflow-wrap:anywhere;color:#9fb1a5}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(440px,100%),1fr));gap:20px}figure{margin:0;background:#1c231e;border:1px solid #3a463e}img,video{display:block;width:100%;height:auto}figcaption{padding:12px;font-size:13px}li{margin:8px 0}.status{padding:14px;background:#263229;border-left:3px solid #abc7b1}.fail{border-color:#de946e}.criteria{display:flex;gap:24px;flex-wrap:wrap}.criteria span{border-bottom:1px solid #53675a;padding:8px 0}details{margin:18px 0}
  </style><header><small>WRELA / AUTHORING REVIEW</small><h1>Eight disciplines. One rendered world.</h1><p>Inspect full-size frames, motion and technical evidence. A complete capture establishes that the tools ran; visual acceptance requires reviewing the actual output.</p><small>Source ${escapeHtml(report.sourceFingerprint)} · ${escapeHtml(report.startedAt)}</small><p class="status ${status.failed.length ? "fail" : ""}">${status.accepted ? "All categories visually accepted" : status.complete ? (report.studies.every((study) => study.visualAcceptance !== "unreviewed") ? "Visual review complete · AAA bar not met" : "All categories captured · visual review remains open") : "Capture suite incomplete"}</p><a href="report.json">Evidence manifest</a></header><nav>${AUTHORING_STUDIES.map((s) => `<a href="#${s.id}">${escapeHtml(s.name)}</a>`).join("")}</nav>${AUTHORING_STUDIES.map(
    (study) => {
      const evidence = report.studies.find((s) => s.id === study.id);
      if (!evidence)
        return `<section id="${study.id}"><h2>${escapeHtml(study.name)}</h2><p>Not captured.</p></section>`;
      const media = evidence.artifacts.filter((a) => /\.(png|webm)$/.test(a) && !a.endsWith("failure.png"));
      const docs = evidence.artifacts.filter((a) => a.endsWith(".json"));
      return `<section id="${study.id}"><h2>${escapeHtml(study.name)}</h2><p>${evidence.exitCode === 0 ? "Capture complete" : "Capture failed"} · ${evidence.durationSeconds.toFixed(1)} s · Visual verdict: ${evidence.visualAcceptance}</p><div class="criteria">${study.criteria.map((c) => `<span>${escapeHtml(c)}</span>`).join("")}</div>${evidence.observations.length ? `<ul>${evidence.observations.map((o) => `<li>${escapeHtml(o)}</li>`).join("")}</ul>` : ""}<details><summary>Technical evidence</summary><ul><li><a href="${url(evidence.log)}">Capture log</a></li>${docs.map((a) => `<li><a href="${url(a)}">${escapeHtml(a.split("/").at(-1) ?? a)}</a></li>`).join("")}</ul></details><div class="grid">${media.map((a) => `<figure>${a.endsWith(".webm") ? `<video controls loop preload="metadata" src="${url(a)}"></video>` : `<a href="${url(a)}"><img loading="lazy" alt="${escapeHtml(a.split("/").at(-1) ?? a)}" src="${url(a)}"></a>`}<figcaption>${escapeHtml(a.split("/").at(-1) ?? a)}</figcaption></figure>`).join("")}</div></section>`;
    },
  ).join("")}</html>`;
}
