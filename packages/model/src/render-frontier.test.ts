import { expect, test } from "bun:test";
import {
  type FrontierCandidate,
  type FrontierPolicy,
  frontierProductKey,
  selectRenderFrontier,
} from "@wrela/examples/render-frontier";

const policy: FrontierPolicy = {
  budgets: {
    "linear-radiance-rms": 0.02,
    "linear-radiance-max": 0.1,
    "coverage-rms": 0.03,
    "temporal-max": 0.05,
  },
  costAxes: ["gpuMs", "cpuMs", "ownedBytes"],
};
function candidate(id: string, gpuMs = 2, error = 0.01): FrontierCandidate {
  const products = [
    { key: `${id}-product`, sourceKey: "authored-field", algorithmVersion: "v1", domainKey: "valid-poses" },
  ];
  return {
    id,
    label: id,
    products,
    domain: {
      sourceKey: "project",
      fixture: "canopy",
      cameraKey: "orbit-1",
      lightingKey: "lighting-1",
      motionKey: "wind-1",
      width: 960,
      height: 640,
      adapter: "adapter-1",
      browser: "browser-1",
      kernelVersion: "kernel-1",
      referenceKey: "dense-reference",
      errorDomain: "matched-trial-1",
      cpuScope: "preparation",
      memoryScope: "renderer-retained",
    },
    quality: [
      {
        coordinate: "linear-radiance-rms",
        units: "linear-radiance",
        evidence: {
          kind: "measured",
          metric: "linear-radiance",
          rms: error,
          maximum: error * 2,
          domain: "matched-trial-1",
          reference: "dense-reference",
          samples: 500,
        },
        uncertainty: 0,
      },
      {
        coordinate: "linear-radiance-max",
        units: "linear-radiance",
        evidence: {
          kind: "measured",
          metric: "linear-radiance",
          rms: error,
          maximum: error * 2,
          domain: "matched-trial-1",
          reference: "dense-reference",
          samples: 500,
        },
        uncertainty: 0,
      },
      {
        coordinate: "coverage-rms",
        units: "fractional-coverage",
        evidence: {
          kind: "measured-coverage",
          rms: error,
          maximum: error * 2,
          domain: "matched-trial-1",
          reference: "dense-reference",
          samples: 500,
        },
        uncertainty: 0,
      },
      {
        coordinate: "temporal-max",
        units: "linear-radiance-difference",
        evidence: {
          kind: "measured",
          metric: "temporal",
          rms: error,
          maximum: error * 2,
          domain: "matched-trial-1",
          reference: "dense-reference",
          samples: 500,
        },
        uncertainty: 0,
      },
    ],
    cost: {
      observation: {
        productKey: frontierProductKey(products),
        adapter: "adapter-1",
        browser: "browser-1",
        kernelVersion: "kernel-1",
        fixture: "canopy",
        gpuP50Ms: gpuMs,
        gpuP95Ms: gpuMs,
        preparationMs: 3,
      },
      cpuMs: 3,
      ownedBytes: 1024,
      samples: 50,
      uncertainty: { gpuMs: 0, cpuMs: 0, ownedBytes: 0 },
    },
    provenance: [{ artifact: "capture.json", pointer: `/candidates/${id}` }],
  };
}
test("frontier preserves real cost/quality tradeoffs, removes dominated samples and retains stable ties", () => {
  const fast = candidate("fast", 1, 0.015),
    accurate = candidate("accurate", 2, 0.005),
    slow = candidate("slow", 4, 0.015),
    twin = candidate("twin", 1, 0.015);
  const result = selectRenderFrontier([slow, twin, fast, accurate], policy);
  expect(result.groups[0].frontier).toEqual(["accurate", "fast", "twin"]);
  expect(result.decisions.find((entry) => entry.id === "slow")?.dominatedBy.map((entry) => entry.id)).toEqual(
    ["accurate", "fast", "twin"],
  );
  expect(result.decisions.find((entry) => entry.id === "fast")?.tiedWith).toEqual(["twin"]);
  expect(selectRenderFrontier([accurate, fast, twin, slow], policy)).toEqual(result);
});
test("different sources, cameras, resolution, lights, motion, adapter or kernel remain incomparable", () => {
  for (const key of [
    "sourceKey",
    "cameraKey",
    "width",
    "height",
    "lightingKey",
    "motionKey",
    "adapter",
    "browser",
    "kernelVersion",
    "referenceKey",
    "errorDomain",
    "cpuScope",
    "memoryScope",
  ] as const) {
    const a = candidate("a"),
      b = candidate("b", 0.1);
    if (key === "width" || key === "height") b.domain[key] *= 2;
    else if (key === "cpuScope") b.domain.cpuScope = "render-p95";
    else if (key === "memoryScope") b.domain.memoryScope = "selected-payload";
    else b.domain[key] = "other";
    const result = selectRenderFrontier([a, b], policy);
    expect(result.groups).toHaveLength(2);
    expect(result.incomparable[0].reasons).toContain(`Different ${key}`);
    expect(result.decisions[0].dominatedBy).toHaveLength(0);
  }
  const other = candidate("other", 0.1);
  other.products[0].sourceKey = "different-field";
  const result = selectRenderFrontier([candidate("a"), other], policy);
  expect(result.incomparable[0].reasons).toContain("Different productSourceKeys");
});
test("unknown quality, uncertainty, reference mismatch and unquantified real bounds are never certified", () => {
  for (const mutation of [
    (value: FrontierCandidate) => {
      value.quality[0].evidence = { kind: "unknown", reason: "Unmeasured" };
    },
    (value: FrontierCandidate) => {
      value.quality[0].uncertainty = null;
    },
    (value: FrontierCandidate) => {
      if (value.quality[0].evidence.kind === "measured") value.quality[0].evidence.reference = "wrong";
    },
    (value: FrontierCandidate) => {
      value.quality[0].evidence = {
        kind: "real-bound",
        metric: "linear-radiance",
        maximum: 0,
        domain: "matched-trial-1",
        numericError: "unknown",
      };
    },
    (value: FrontierCandidate) => {
      value.quality[0].units = "pixels";
    },
    (value: FrontierCandidate) => {
      value.quality = [];
    },
  ]) {
    const value = candidate("unknown", 0.001);
    mutation(value);
    const result = selectRenderFrontier([value], policy);
    expect(result.decisions[0].status).toBe("inadmissible");
    expect(result.groups[0].frontier).toHaveLength(0);
  }
});
test("budgets use upper uncertainty endpoints and exact boundary inclusion", () => {
  const value = candidate("boundary", 1, 0.019);
  value.quality[0].uncertainty = 0.001;
  expect(selectRenderFrontier([value], policy).decisions[0].status).toBe("frontier");
  value.quality[0].uncertainty = 0.0011;
  expect(
    selectRenderFrontier([value], policy).decisions[0].reasons.some(
      (reason) => reason.code === "error-budget",
    ),
  ).toBe(true);
});
test("overlapping timing uncertainty cannot prune a potentially equal or faster alternative", () => {
  const a = candidate("a", 1),
    b = candidate("b", 1.1);
  a.cost.uncertainty.gpuMs = 0.1;
  b.cost.uncertainty.gpuMs = 0.1;
  expect(selectRenderFrontier([a, b], policy).groups[0].frontier).toEqual(["a", "b"]);
  if (!b.cost.observation) throw Error("Missing test observation");
  b.cost.observation.gpuP95Ms = 2;
  expect(selectRenderFrontier([a, b], policy).groups[0].frontier).toEqual(["a"]);
});
test("missing or wrong-product cost evidence cannot inherit another candidate's performance", () => {
  for (const mutation of [
    (value: FrontierCandidate) => {
      value.cost.observation = null;
    },
    (value: FrontierCandidate) => {
      value.cost.uncertainty.cpuMs = null;
    },
    (value: FrontierCandidate) => {
      if (!value.cost.observation) throw Error("Missing test observation");
      value.cost.observation.productKey = "another-product";
    },
    (value: FrontierCandidate) => {
      value.cost.samples = 0;
    },
    (value: FrontierCandidate) => {
      value.cost.ownedBytes = Infinity;
    },
  ]) {
    const value = candidate("bad");
    mutation(value);
    expect(selectRenderFrontier([value], policy).decisions[0].status).toBe("inadmissible");
  }
});
test("a geometric maximum cannot be substituted for radiance RMS and explicit quality budgets are mandatory", () => {
  const value = candidate("bound");
  value.quality[0].evidence = {
    kind: "numeric-bound",
    metric: "silhouette",
    maximum: 0,
    domain: "matched-trial-1",
  };
  expect(selectRenderFrontier([value], policy).decisions[0].status).toBe("inadmissible");
  expect(() => selectRenderFrontier([candidate("a")], { ...policy, budgets: {} })).toThrow("budgets");
  expect(() => selectRenderFrontier([candidate("a"), candidate("a")], policy)).toThrow("uniquely");
});
