import type { DomainBrief } from "./authoring-domain";

/** Curated authored targets, explicitly separate from human-approved art and portable accepted recipes. */
export type AuthoringExemplar = {
  id: string;
  domain: DomainBrief["domain"];
  title: string;
  tags: string[];
  conditions: DomainBrief["conditions"];
  rationale: string;
  watchFor: string[];
  approval: "curated-unreviewed";
};
export const authoringExemplars: readonly AuthoringExemplar[] = [
  {
    id: "silvered-trail-timber",
    domain: "timber",
    title: "Silvered trail timber",
    tags: ["weathered", "silver", "aged", "trail", "gate"],
    conditions: { exposure: 0.82, moisture: 0.5, maturity: 0.88, variation: 0.65 },
    rationale:
      "Restrained silver fibres, longitudinal checks, knots and grounded deposition, with warm sheltered grain.",
    watchFor: [
      "Avoid uniform grey paint and broad pale bands",
      "Keep growth rings continuous across cut faces",
    ],
    approval: "curated-unreviewed",
  },
  {
    id: "sheltered-oiled-timber",
    domain: "timber",
    title: "Sheltered warm timber",
    tags: ["sheltered", "warm", "fresh", "interior"],
    conditions: { exposure: 0.16, moisture: 0.2, maturity: 0.35, variation: 0.4 },
    rationale: "Warm fresh grain, subtle fibre relief and restrained edge softening.",
    watchFor: ["Avoid plastic gloss", "Preserve end grain and joinery"],
    approval: "curated-unreviewed",
  },
  {
    id: "damp-old-timber",
    domain: "timber",
    title: "Damp old timber",
    tags: ["damp", "wet", "old", "forest", "moss"],
    conditions: { exposure: 0.6, moisture: 0.84, maturity: 0.94, variation: 0.7 },
    rationale: "Weathered fibre with stronger lower-member staining and heterogeneous checks.",
    watchFor: [
      "Staining must follow ground height, not each member's length",
      "Do not turn the full member black",
    ],
    approval: "curated-unreviewed",
  },
  {
    id: "exposed-mature-pine",
    domain: "vegetation",
    title: "Exposed mature tree",
    tags: ["exposed", "mature", "pine", "wind", "trail"],
    conditions: { exposure: 0.77, moisture: 0.52, maturity: 0.96, variation: 0.76 },
    rationale: "Coherent crown asymmetry and branch hierarchy, retained foliage clusters and uneven gaps.",
    watchFor: ["Avoid evenly stacked branch shelves", "Keep fine needles legible under grazing light"],
    approval: "curated-unreviewed",
  },
  {
    id: "sheltered-full-crown",
    domain: "vegetation",
    title: "Sheltered full crown",
    tags: ["sheltered", "lush", "full", "healthy"],
    conditions: { exposure: 0.22, moisture: 0.75, maturity: 0.9, variation: 0.48 },
    rationale: "A fuller live crown with varied cohorts and readable secondary growth.",
    watchFor: ["Avoid a solid foliage cone", "Keep species, source dimensions and manual edits"],
    approval: "curated-unreviewed",
  },
  {
    id: "dry-open-crown",
    domain: "vegetation",
    title: "Dry open crown",
    tags: ["dry", "sparse", "open", "ridge"],
    conditions: { exposure: 0.92, moisture: 0.23, maturity: 0.97, variation: 0.85 },
    rationale: "Open mature growth with a clear trunk, irregular lateral reach and restrained foliage loss.",
    watchFor: ["Do not confuse exposure with indiscriminate broken branches", "Preserve a living crown"],
    approval: "curated-unreviewed",
  },
];
export function matchAuthoringExemplar(domain: DomainBrief["domain"], brief: string, id?: string) {
  const eligible = authoringExemplars.filter((e) => e.domain === domain);
  if (id) {
    const exact = eligible.find((e) => e.id === id);
    if (!exact) throw Error("Exemplar does not support this domain");
    return exact;
  }
  const words = new Set(brief.toLowerCase().split(/[^a-z]+/));
  return eligible
    .map((e, i) => ({ e, score: e.tags.filter((t) => words.has(t)).length, order: i }))
    .sort((a, b) => b.score - a.score || a.order - b.order)[0].e;
}
