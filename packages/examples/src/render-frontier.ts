import type { ErrorEvidence, RenderCostObservation, RenderProductMetadata } from "@wrela/model";
import { canonical, contentKey } from "@wrela/model";

export const FRONTIER_QUALITY_COORDINATES = [
  "linear-radiance-rms",
  "linear-radiance-max",
  "silhouette-max-pixels",
  "coverage-rms",
  "coverage-max",
  "temporal-rms",
  "temporal-max",
] as const;
export type FrontierQualityCoordinate = (typeof FRONTIER_QUALITY_COORDINATES)[number];
export type FrontierCostAxis = "gpuMs" | "cpuMs" | "ownedBytes";
export type FrontierDomain = {
  sourceKey: string;
  fixture: string;
  cameraKey: string;
  lightingKey: string;
  motionKey: string;
  width: number;
  height: number;
  adapter: string;
  browser: string;
  kernelVersion: string;
  referenceKey: string;
  errorDomain: string;
  cpuScope: "preparation" | "render-p95";
  memoryScope: "selected-payload" | "renderer-retained";
};
export type FrontierCoverageEvidence = {
  kind: "measured-coverage";
  rms: number;
  maximum: number;
  domain: string;
  reference: string;
  samples: number;
};
export type FrontierQualitySample = {
  coordinate: FrontierQualityCoordinate;
  /** Explicit units prevent treating a world-space geometric bound as pixels. */
  units: "linear-radiance" | "pixels" | "fractional-coverage" | "linear-radiance-difference";
  evidence: ErrorEvidence | FrontierCoverageEvidence;
  /** Absolute uncertainty in this coordinate; null is unavailable, never zero. */
  uncertainty: number | null;
};
export type FrontierCandidate = {
  id: string;
  label: string;
  /** A configuration can select several real compiled products together. */
  products: Pick<RenderProductMetadata, "key" | "sourceKey" | "algorithmVersion" | "domainKey">[];
  domain: FrontierDomain;
  quality: FrontierQualitySample[];
  cost: {
    observation: RenderCostObservation | null;
    cpuMs: number | null;
    ownedBytes: number | null;
    samples: number;
    uncertainty: Record<FrontierCostAxis, number | null>;
  };
  provenance: { artifact: string; pointer: string }[];
};
export type FrontierPolicy = {
  budgets: Partial<Record<FrontierQualityCoordinate, number>>;
  costAxes: FrontierCostAxis[];
};
type Interval = { value: number; lower: number; upper: number };
export type FrontierReason = { code: string; message: string; coordinate?: string };
export type FrontierDecision = {
  id: string;
  domainKey: string;
  status: "frontier" | "dominated" | "inadmissible";
  reasons: FrontierReason[];
  coordinates: Record<string, Interval>;
  dominatedBy: { id: string; strictlyBetter: string[] }[];
  tiedWith: string[];
};
export type RenderFrontier = {
  policy: FrontierPolicy;
  groups: { key: string; domain: FrontierDomain; productSourceKeys: string[]; frontier: string[] }[];
  decisions: FrontierDecision[];
  incomparable: { left: string; right: string; reasons: string[] }[];
  guarantee: string;
};

const finite = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= 0;
const costAxes: FrontierCostAxis[] = ["gpuMs", "cpuMs", "ownedBytes"];
const domainFields: (keyof FrontierDomain)[] = [
  "sourceKey",
  "fixture",
  "cameraKey",
  "lightingKey",
  "motionKey",
  "width",
  "height",
  "adapter",
  "browser",
  "kernelVersion",
  "referenceKey",
  "errorDomain",
  "cpuScope",
  "memoryScope",
];
const sourceKeys = (candidate: FrontierCandidate) =>
  [...new Set(candidate.products.map((product) => product.sourceKey))].sort();
export function frontierProductKey(products: FrontierCandidate["products"]): string {
  return products.length === 1
    ? products[0].key
    : `configuration-${contentKey(products.map((product) => product.key).sort())}`;
}
export function frontierDomainKey(candidate: FrontierCandidate): string {
  return contentKey({ ...candidate.domain, productSourceKeys: sourceKeys(candidate) });
}
function interval(value: number, uncertainty: number): Interval {
  return { value, lower: Math.max(0, value - uncertainty), upper: value + uncertainty };
}
function qualityValue(
  sample: FrontierQualitySample,
  candidate: FrontierCandidate,
): { value?: number; reason?: string } {
  const { coordinate, evidence, units } = sample;
  if (evidence.kind === "unknown") return { reason: evidence.reason };
  if (evidence.domain !== candidate.domain.errorDomain)
    return { reason: "Quality evidence belongs to another measurement domain" };
  if (evidence.kind === "real-bound") return { reason: "Real-arithmetic bound has unknown numeric error" };
  const expected = coordinate.startsWith("linear-radiance")
    ? "linear-radiance"
    : coordinate.startsWith("silhouette")
      ? "pixels"
      : coordinate.startsWith("coverage")
        ? "fractional-coverage"
        : "linear-radiance-difference";
  if (units !== expected)
    return { reason: `Expected ${expected} units; no implicit conversion is permitted` };
  if (evidence.kind === "measured" || evidence.kind === "measured-coverage") {
    if (evidence.reference !== candidate.domain.referenceKey)
      return { reason: "Quality reference does not match the comparison reference" };
    if (
      !Number.isSafeInteger(evidence.samples) ||
      evidence.samples < 1 ||
      !finite(evidence.rms) ||
      !finite(evidence.maximum) ||
      evidence.rms > evidence.maximum + 1e-12
    )
      return { reason: "Invalid measured error or sample count" };
  }
  if (!finite(evidence.maximum)) return { reason: "Invalid maximum error" };
  const metric = evidence.kind === "measured-coverage" ? "coverage" : evidence.metric;
  const expectedMetric = coordinate.startsWith("linear-radiance")
    ? "linear-radiance"
    : coordinate.startsWith("silhouette")
      ? "silhouette"
      : coordinate.startsWith("coverage")
        ? "coverage"
        : "temporal";
  if (metric !== expectedMetric)
    return { reason: "Evidence metric does not measure the requested quality coordinate" };
  if (coordinate.endsWith("rms")) {
    if (evidence.kind !== "measured" && evidence.kind !== "measured-coverage")
      return { reason: "A maximum bound is not an RMS observation" };
    return { value: evidence.rms };
  }
  return { value: evidence.maximum };
}
function inspect(candidate: FrontierCandidate, policy: FrontierPolicy): FrontierDecision {
  const reasons: FrontierReason[] = [],
    coordinates: Record<string, Interval> = {};
  const fail = (code: string, message: string, coordinate?: string) =>
    reasons.push({ code, message, coordinate });
  for (const key of domainFields) {
    const value = candidate.domain[key];
    if (key === "width" || key === "height") {
      if (!Number.isSafeInteger(value) || Number(value) < 1 || Number(value) > 32768)
        fail("domain", `Invalid ${key}`);
    } else if (typeof value !== "string" || !value.length) fail("domain", `Missing ${key}`);
  }
  if (
    !["preparation", "render-p95"].includes(candidate.domain.cpuScope) ||
    !["selected-payload", "renderer-retained"].includes(candidate.domain.memoryScope)
  )
    fail("domain", "Invalid CPU or memory measurement scope");
  if (
    !candidate.products.length ||
    candidate.products.some(
      (product) => !product.key || !product.sourceKey || !product.algorithmVersion || !product.domainKey,
    )
  )
    fail("product", "Compiled product provenance is missing");
  if (new Set(candidate.products.map((product) => product.key)).size !== candidate.products.length)
    fail("product", "Duplicate compiled product identities");
  if (
    !candidate.provenance.length ||
    candidate.provenance.some((source) => !source.artifact || !source.pointer)
  )
    fail("provenance", "Retained evidence artifact and pointer are required");
  for (const [coordinate, budget] of Object.entries(policy.budgets) as [
    FrontierQualityCoordinate,
    number,
  ][]) {
    const samples = candidate.quality.filter((sample) => sample.coordinate === coordinate);
    if (samples.length !== 1) {
      fail("quality-missing", `Exactly one explicit ${coordinate} sample is required`, coordinate);
      continue;
    }
    const sample = samples[0],
      result = qualityValue(sample, candidate);
    if (result.reason || result.value === undefined) {
      fail("quality-unknown", result.reason ?? "Missing quality value", coordinate);
      continue;
    }
    if (!finite(sample.uncertainty)) {
      fail("quality-unknown", "Quality uncertainty is unavailable", coordinate);
      continue;
    }
    const range = interval(result.value, sample.uncertainty);
    coordinates[coordinate] = range;
    if (range.upper > budget)
      fail("error-budget", `${coordinate} upper value ${range.upper} exceeds ${budget}`, coordinate);
  }
  const observation = candidate.cost.observation;
  if (!observation) fail("unmeasured-cost", "Comparable cost observation is missing");
  else {
    for (const key of ["adapter", "browser", "kernelVersion", "fixture"] as const)
      if (observation[key] !== candidate.domain[key])
        fail("cost-domain", `Cost ${key} does not match the comparison domain`);
    if (observation.productKey !== frontierProductKey(candidate.products))
      fail("cost-product", "Cost observation belongs to another product configuration");
    if (
      ![observation.gpuP50Ms, observation.gpuP95Ms, observation.preparationMs].every(finite) ||
      observation.gpuP50Ms > observation.gpuP95Ms
    )
      fail("unmeasured-cost", "Invalid GPU/preparation observation");
    if (candidate.domain.cpuScope === "preparation" && candidate.cost.cpuMs !== observation.preparationMs)
      fail("cost-domain", "CPU preparation coordinate must match preparationMs");
  }
  if (!Number.isSafeInteger(candidate.cost.samples) || candidate.cost.samples < 1)
    fail("unmeasured-cost", "Cost sample count is missing");
  for (const axis of policy.costAxes) {
    const value = axis === "gpuMs" ? observation?.gpuP95Ms : candidate.cost[axis];
    const uncertainty = candidate.cost.uncertainty[axis];
    if (!finite(value) || !finite(uncertainty) || !Number.isFinite(value + uncertainty))
      fail("unmeasured-cost", `${axis} or its uncertainty is unavailable`, axis);
    else coordinates[axis] = interval(value, uncertainty);
  }
  return {
    id: candidate.id,
    domainKey: frontierDomainKey(candidate),
    status: reasons.length ? "inadmissible" : "frontier",
    reasons,
    coordinates,
    dominatedBy: [],
    tiedWith: [],
  };
}

/** Finite, evidence-scoped Pareto analysis. Never optimizes or changes runtime rendering. */
export function selectRenderFrontier(
  candidates: readonly FrontierCandidate[],
  policy: FrontierPolicy,
): RenderFrontier {
  if (
    !candidates.length ||
    candidates.length > 128 ||
    new Set(candidates.map((candidate) => candidate.id)).size !== candidates.length ||
    candidates.some((candidate) => !candidate.id)
  )
    throw Error("Supply 1–128 uniquely identified candidates");
  const quality = Object.keys(policy.budgets);
  if (
    !quality.length ||
    quality.some((key) => !FRONTIER_QUALITY_COORDINATES.includes(key as FrontierQualityCoordinate)) ||
    !Object.values(policy.budgets).every(finite)
  )
    throw Error("Explicit finite nonnegative quality budgets are required");
  if (
    !policy.costAxes.length ||
    new Set(policy.costAxes).size !== policy.costAxes.length ||
    policy.costAxes.some((axis) => !costAxes.includes(axis))
  )
    throw Error("Supply unique explicit cost axes");
  const ordered = [...candidates].sort((a, b) => a.id.localeCompare(b.id));
  const decisions = ordered.map((candidate) => inspect(candidate, policy));
  const incomparable: RenderFrontier["incomparable"] = [];
  const axes = [...quality, ...policy.costAxes];
  const dominates = (a: FrontierDecision, b: FrontierDecision) =>
    axes.every((axis) => a.coordinates[axis].upper <= b.coordinates[axis].lower) &&
    axes.some((axis) => a.coordinates[axis].upper < b.coordinates[axis].lower);
  for (let i = 0; i < ordered.length; i++)
    for (let j = i + 1; j < ordered.length; j++) {
      const a = decisions[i],
        b = decisions[j];
      if (a.domainKey !== b.domainKey) {
        const differences = Object.keys(ordered[i].domain).filter(
          (key) =>
            canonical(ordered[i].domain[key as keyof FrontierDomain]) !==
            canonical(ordered[j].domain[key as keyof FrontierDomain]),
        );
        if (canonical(sourceKeys(ordered[i])) !== canonical(sourceKeys(ordered[j])))
          differences.push("productSourceKeys");
        incomparable.push({ left: a.id, right: b.id, reasons: differences.map((key) => `Different ${key}`) });
      } else if (a.status === "inadmissible" || b.status === "inadmissible")
        incomparable.push({
          left: a.id,
          right: b.id,
          reasons: ["One or both candidates lack admissible evidence"],
        });
      else {
        let hasDominance = false;
        for (const [winner, loser] of [
          [a, b],
          [b, a],
        ])
          if (dominates(winner, loser)) {
            loser.status = "dominated";
            loser.dominatedBy.push({
              id: winner.id,
              strictlyBetter: axes.filter(
                (axis) => winner.coordinates[axis].upper < loser.coordinates[axis].lower,
              ),
            });
            hasDominance = true;
          }
        if (!hasDominance) {
          if (axes.every((axis) => canonical(a.coordinates[axis]) === canonical(b.coordinates[axis]))) {
            a.tiedWith.push(b.id);
            b.tiedWith.push(a.id);
          }
          incomparable.push({
            left: a.id,
            right: b.id,
            reasons: [
              a.tiedWith.includes(b.id)
                ? "Equivalent coordinates; both retained"
                : "Quality/cost trade-off or overlapping uncertainty intervals; both retained unless dominated elsewhere",
            ],
          });
        }
      }
    }
  const groups = new Map<string, RenderFrontier["groups"][number]>();
  ordered.forEach((candidate, index) => {
    const key = decisions[index].domainKey;
    if (!groups.has(key))
      groups.set(key, {
        key,
        domain: structuredClone(candidate.domain),
        productSourceKeys: sourceKeys(candidate),
        frontier: [],
      });
    if (decisions[index].status === "frontier") groups.get(key)?.frontier.push(candidate.id);
  });
  return {
    policy: structuredClone(policy),
    groups: [...groups.values()].sort((a, b) => a.key.localeCompare(b.key)),
    decisions,
    incomparable,
    guarantee:
      "Empirical nondominance over supplied candidates and explicitly budgeted quality coordinates within each exact recorded domain. Unknown evidence is inadmissible. No global optimum, complete image equivalence, artistic acceptance or hardware generalization is established.",
  };
}
