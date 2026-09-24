import { botanicalPreset, contentKey, createSurfaceLayer, type Project } from "@wrela/model";
import { z } from "zod";
import { planAssemblyIntent } from "./assembly-intent";
import type { Operation } from "./commands";
import type { ResultConstraint } from "./review-contract";

const unit = z.number().finite().min(0).max(1);
/** Shared authored conditions. Domain adapters interpret them; these are not calibrated physical units. */
export const authoringConditionsSchema = z.strictObject({
  exposure: unit.default(0.7),
  moisture: unit.default(0.35),
  maturity: unit.default(0.8),
  variation: unit.default(0.45),
});
export const domainBriefSchema = z.strictObject({
  domain: z.enum(["timber", "vegetation"]),
  target: z.string().min(1).max(100),
  conditions: authoringConditionsSchema.prefault({}),
  quality: z.strictObject({
    direction: z.string().min(1).max(1200),
    criteria: z.array(z.string().min(1).max(240)).min(1).max(8),
    referenceNotes: z.string().max(1200).default(""),
    references: z.array(z.string().min(1).max(1000)).max(4).default([]),
  }),
});
export type DomainBrief = z.infer<typeof domainBriefSchema>;
export type DomainPlan = {
  operations: Operation[];
  constraints: ResultConstraint[];
  explanation: string[];
  limitations: string[];
};
export const domainControls = [
  {
    name: "exposure",
    min: 0,
    max: 1,
    units: "normalized authored exposure",
    description: "Sun/weather exposure for timber; wind/light exposure for vegetation.",
  },
  {
    name: "moisture",
    min: 0,
    max: 1,
    units: "normalized authored moisture",
    description: "Ground staining for timber; canopy retention and vigor for vegetation.",
  },
  {
    name: "maturity",
    min: 0,
    max: 1,
    units: "normalized authored maturity",
    description: "Accumulated patina for timber; fraction of authored mature plant size for vegetation.",
  },
  {
    name: "variation",
    min: 0,
    max: 1,
    units: "normalized irregularity",
    description: "Member-to-member patina variation or botanical growth asymmetry.",
  },
] as const;
export function domainFor(project: Project, target: string): DomainBrief["domain"] | undefined {
  const d = project.documents.find((d) => d.id === target);
  return d?.kind === "object" && d.assembly ? "timber" : d?.kind === "vegetation" ? "vegetation" : undefined;
}
export function domainQuality(domain: DomainBrief["domain"]): DomainBrief["quality"] {
  return domain === "timber"
    ? {
        direction: "Weathered timber with restrained silvering and readable construction",
        criteria: [
          "Grain follows each member and rings cross end faces",
          "Exposed surfaces vary without uniform stripes",
          "Moisture staining concentrates near the ground",
          "Connections and accepted dimensions remain intact",
        ],
        referenceNotes:
          "Judge in neutral, grazing, close-up and gameplay views; this is a procedural starting point, not an approved reference asset.",
        references: [],
      }
    : {
        direction: "A mature exposed tree with a legible irregular crown and retained foliage",
        criteria: [
          "Species and trunk remain recognizable",
          "Branch hierarchy and crown gaps read at gameplay distance",
          "Exposure produces coherent asymmetry, not random broken limbs",
          "Seed, material ownership, branch edits and authored dimensions remain intact",
        ],
        referenceNotes:
          "Compare whole-plant silhouette and canopy detail. Authored growth controls are not a calibrated botanical simulation.",
        references: [],
      };
}

export function planDomainAuthoring(project: Project, input: DomainBrief, namespace: string): DomainPlan {
  const brief = domainBriefSchema.parse(input),
    d = project.documents.find((d) => d.id === brief.target);
  if (domainFor(project, brief.target) !== brief.domain)
    throw Error(`Target does not support ${brief.domain} authoring`);
  const c = brief.conditions;
  if (brief.domain === "timber") {
    const plan = planAssemblyIntent(project, {
      kind: "assembly.timber",
      target: brief.target,
      material: `wood-${contentKey([namespace, contentKey(project), brief.target, c])}`,
      age: c.maturity,
      grainScale: 0.85 + c.variation * 0.55,
      bevel: 0.002 + c.maturity * 0.003,
      edgeWear: c.exposure * c.maturity * 0.8,
    });
    let member = 0;
    for (const op of plan.operations)
      if (op.kind === "document.create" && op.document.kind === "material") {
        const material = op.document;
        if (!material.appearance) continue;
        const phase = (((member++ * 0.61803398875) % 1) - 0.5) * c.variation;
        // Silvering changes the exposed fibre itself, not just an occasional overlaid patch.
        const silvering = Math.min(1, c.exposure * c.maturity * 1.6);
        material.color = [0.27, 0.175, 0.09].map((v, i) => v + ([0.32, 0.315, 0.285][i] - v) * silvering) as [
          number,
          number,
          number,
        ];
        material.secondary = [0.115, 0.069, 0.032].map(
          (v, i) => v + ([0.11, 0.1, 0.078][i] - v) * silvering,
        ) as [number, number, number];
        material.roughness = 0.68 + c.maturity * 0.2;
        const patina = createSurfaceLayer("exposure-patina");
        patina.name = "Exposure-driven silvering";
        patina.color = [0.32, 0.315, 0.285];
        patina.coverage = Math.max(0, Math.min(0.9, c.exposure * c.maturity * (0.78 + phase * 0.18)));
        patina.roughness = 0.86;
        patina.relief = 0.0004;
        patina.mask = { ...patina.mask, kind: "noise", scale: 6.4 + phase, threshold: 0.46, softness: 0.3 };
        // Keep ground deposition above exposure patina, with the existing sweep-height transform.
        for (const layer of material.appearance.layers) layer.coverage *= c.moisture * 1.4;
        material.appearance.layers.unshift(patina);
        material.appearance.weathering = c.maturity * c.exposure * 0.25;
        material.appearance.damage = c.maturity * c.exposure * 0.025;
        material.appearance.detail.strength = 0.92;
        material.appearance.dirt = c.moisture * 0.08;
      }
    return {
      ...plan,
      explanation: [
        "New member-scoped materials preserve shared bindings and construction",
        "Exposure/maturity drive patina; moisture drives ground staining; variation changes each member coherently",
      ],
      limitations: [
        ...plan.limitations,
        "Weathering is an authored approximation; no physical joint-cavity or fracture inference is claimed.",
      ],
    };
  }
  if (d?.kind !== "vegetation") throw Error("Vegetation target required");
  if (d.botanical?.development)
    throw Error(
      "Developmental plants need event-based growth edits; this adapter preserves the existing botanical architecture path",
    );
  const botanical = structuredClone(d.botanical ?? botanicalPreset("pine"));
  botanical.age = 0.55 + c.maturity * 0.45;
  botanical.growth.asymmetry = 0.12 + c.variation * 0.48 + c.exposure * 0.18;
  botanical.growth.tropism = 0.12 + (1 - c.exposure) * 0.32;
  botanical.canopy.density = Math.min(1, 0.91 + c.moisture * 0.12 - c.exposure * 0.075);
  botanical.canopy.droop = 0.12 + c.maturity * c.moisture * 0.3;
  botanical.damage.leafLoss = c.exposure * (1 - c.moisture) * 0.12;
  botanical.motion.stiffness = 0.28 + c.exposure * 0.42;
  if (botanical.conifer) {
    botanical.conifer.whorlJitter = 0.55 + c.variation * 0.4;
    botanical.conifer.limbSag = 0.12 + c.maturity * 0.2;
    botanical.conifer.crownPower = 0.65 + c.exposure * 0.22;
    botanical.conifer.crownBias = 0.82 + c.variation * 0.22;
    if (botanical.conifer.architecture) {
      // Retain coherent live shoots; silhouette gaps come from branch structure,
      // instead of thinning each shoot into a uniformly transparent crown.
      botanical.conifer.needlesPerShoot = 160;
      botanical.conifer.needleLength = 0.065 + c.moisture * 0.025;
      botanical.conifer.needleWidth = 0.0025 + c.moisture * 0.001;
      botanical.conifer.architecture.foliageStart = 0.24 + c.exposure * 0.14;
      botanical.conifer.architecture.twigLength = 0.2 + c.moisture * 0.065;
      botanical.conifer.architecture.twigPairs = 3;
      botanical.conifer.architecture.crownAsymmetry = c.exposure * 0.42 + c.variation * 0.28;
      botanical.conifer.architecture.clusterVariation = 0.25 + c.variation * 0.55;
      botanical.conifer.architecture.cohortContrast = 0.4 + c.moisture * 0.4;
    }
  }
  const constraints: ResultConstraint[] = [];
  for (const key of Object.keys(d))
    if (key !== "botanical")
      constraints.push({
        kind: "source",
        id: `plant-preserve-${constraints.length}`,
        target: d.id,
        path: [key],
      });
  if (d.botanical)
    for (const path of [
      ["species"],
      ["pruning"],
      ["branchEdits"],
      ["growth", "levels"],
      ["growth", "children"],
      ["growth", "branchAngle"],
      ["growth", "lengthRatio"],
      ["growth", "taper"],
    ])
      constraints.push({
        kind: "source",
        id: `plant-preserve-${constraints.length}`,
        target: d.id,
        path: ["botanical", ...path],
      });
  return {
    operations: [{ kind: "document.set", target: d.id, path: ["botanical"], value: botanical }],
    constraints,
    explanation: [
      "Exposure, moisture and maturity coordinate crown retention, branch asymmetry, droop and stiffness",
      "Existing seed, materials, dimensions, branch edits and pruning remain source-protected",
    ],
    limitations: [
      "Uses bounded botanical architecture controls; no climate-calibrated growth or new species inference",
      "Quality direction and reference notes guide visual selection; free text is not silently interpreted as numerical controls",
    ],
  };
}
