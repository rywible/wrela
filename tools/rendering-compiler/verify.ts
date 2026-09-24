import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";
import {
  ACCEPTANCE_VERSION,
  type AcceptanceScenario,
  type AcceptanceVariant,
  PRIMITIVE_CAMERAS,
  TRAJECTORY,
  VARIANTS,
} from "./manifest";
import { summarizeAlternatingTrials, summarizeMatchedTrial, trialOrder } from "./timing";

type Capture = Awaited<ReturnType<AcceptanceFixture["capture"]>>;
type Run = Awaited<ReturnType<AcceptanceFixture["run"]>>;
const low = process.argv.includes("--low"),
  preview = process.argv.includes("--preview"),
  small = process.argv.includes("--small");
const width = small ? 640 : low ? 1280 : 1920,
  height = small ? 360 : low ? 720 : 1080;
const requested = process.argv.find((value) => value.startsWith("--scenario="))?.slice(11) as
  | AcceptanceScenario
  | undefined;
const scenarios: AcceptanceScenario[] = requested
  ? [requested]
  : preview
    ? ["winter-valley"]
    : process.argv.includes("--targeted")
      ? ["primitive", "water", "lighting", "visibility"]
      : ["winter-valley", "primitive", "water", "lighting", "visibility"];
const trials = preview
  ? 0
  : Number(process.argv.find((value) => value.startsWith("--trials="))?.slice(9) ?? 5);
if (!preview && (!Number.isInteger(trials) || trials < 3 || trials > 9))
  throw Error("Use 3–9 alternating trials");
const frames = 91;
const scenarioVariants: Record<AcceptanceScenario, AcceptanceVariant[]> = {
  "winter-valley": ["production", "parametric-control", "visibility-control"],
  primitive: ["production", "parametric-control"],
  water: ["production", "water-reference"],
  lighting: ["production"],
  visibility: ["production", "visibility-control"],
};
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};createAcceptanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const results = [];
    try {
      await view.navigate(`${server.url}?profile=${low ? "low" : "balanced"}`);
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      await Bun.write(
        join(output, "trajectory.json"),
        JSON.stringify(
          {
            version: ACCEPTANCE_VERSION,
            frames,
            trajectory: TRAJECTORY,
            primitiveCameras: PRIMITIVE_CAMERAS,
            variants: VARIANTS,
          },
          null,
          2,
        ),
      );
      for (const scenario of scenarios) {
        console.log(JSON.stringify({ scenario, stage: "preparing", frames }));
        const preparation = await view.evaluate(`fixture.prepare(${JSON.stringify(scenario)},${frames})`);
        await Bun.write(join(output, `${scenario}-preparation.json`), JSON.stringify(preparation, null, 2));
        const variants = preview ? ["production" as const] : scenarioVariants[scenario];
        // Dense response integration is deliberately restricted to a small image.
        // Both conditions use the same pixel footprint, including capture warmup.
        const captureResolution = scenario === "water" ? [640, 360] : [width, height];
        const capturesByVariant = [];
        for (const variant of variants) {
          console.log(
            JSON.stringify({ scenario, variant, stage: "capture-initialization", captureResolution }),
          );
          const initialization = await view.evaluate(
            `fixture.beginVariant(${JSON.stringify(variant)},{},${JSON.stringify(captureResolution)})`,
          );
          console.log(JSON.stringify({ scenario, variant, stage: "capture-warmup" }));
          const warmup = await view.evaluate<Run>(
            variant === "water-reference" ? "fixture.run(true,1)" : "fixture.run(true)",
          );
          const captures: Capture[] = [];
          const anchors = scenario === "primitive" ? [0, 22, 45, 67, 90] : [0, 30, 60, 90];
          for (const frame of anchors) {
            console.log(JSON.stringify({ scenario, variant, frame, stage: "beauty-capture" }));
            captures.push(await view.evaluate<Capture>(`fixture.capture(${frame},"beauty")`));
            await Bun.write(
              join(output, `${scenario}-${variant}-${frame}-beauty.png`),
              await view.screenshot(),
            );
          }
          for (const mode of (variant === "water-reference" ? [] : ["identity", "depth", "normals"]) as (
            | "identity"
            | "depth"
            | "normals"
          )[]) {
            captures.push(await view.evaluate<Capture>(`fixture.capture(0,${JSON.stringify(mode)})`));
            await Bun.write(join(output, `${scenario}-${variant}-0-${mode}.png`), await view.screenshot());
          }
          const captureSet = {
            variant,
            initialization,
            measurements: warmup.measurements,
            diagnostics: warmup.diagnostics,
            captures,
          };
          await Bun.write(join(output, `${scenario}-${variant}.json`), JSON.stringify(captureSet, null, 2));
          if (
            scenario === "primitive" &&
            variant === "parametric-control" &&
            !captures.some((c) => c.completeness.realizations?.some((r) => r.kind === "parametric-mesh"))
          )
            throw Error("Parametric geometry control did not select any valid parametric mesh");
          if (scenario === "visibility" && variant === "visibility-control") {
            const optimized = capturesByVariant[0].captures.find((c) => c.mode === "identity"),
              unculled = captures.find((c) => c.mode === "identity");
            if (
              !optimized ||
              !unculled ||
              optimized.completeness.culled.length <= unculled.completeness.culled.length
            )
              throw Error("Production visibility did not remove hidden work");
          }
          capturesByVariant.push(captureSet);
          console.log(JSON.stringify({ scenario, variant, captures: captures.length }));
        }
        const paired = [];
        const timingResolution = scenario === "water" ? [320, 180] : [width, height];
        const timingFrames = scenario === "water" ? 17 : frames;
        if (scenario === "water" && trials) await view.evaluate(`fixture.prepare("water",${timingFrames})`);
        if (variants.length > 1)
          for (let trial = 0; trial < trials; trial++) {
            const observations = [];
            for (const variant of trialOrder(variants, trial)) {
              console.log(
                JSON.stringify({
                  scenario,
                  variant,
                  trial,
                  stage: "timing-initialization",
                  timingResolution,
                  timingFrames,
                }),
              );
              await view.evaluate(
                `fixture.beginVariant(${JSON.stringify(variant)},{},${JSON.stringify(timingResolution)})`,
              );
              console.log(JSON.stringify({ scenario, variant, trial, stage: "timing-warmup" }));
              const serialGpu = scenario === "water";
              await view.evaluate(`fixture.run(true,${timingFrames},${serialGpu})`);
              console.log(JSON.stringify({ scenario, variant, trial, stage: "timing-measurement" }));
              const sample = await view.evaluate<Run>(`fixture.run(false,${timingFrames},${serialGpu})`);
              observations.push({ variant, ...sample });
            }
            const matched = summarizeMatchedTrial(observations);
            paired.push(matched);
            await Bun.write(
              join(output, `${scenario}-trial-${trial}.json`),
              JSON.stringify(
                { trial, order: observations.map((o) => o.variant), observations, matched },
                null,
                2,
              ),
            );
            console.log(
              JSON.stringify({
                scenario,
                trial,
                matchedFrames: matched.trajectoryFrames.length,
                gpu: matched.variants.map((v) => ({
                  variant: v.variant,
                  ...v.passes.gpuMs,
                  observedQuantumMs: v.observedQuantumMs,
                })),
              }),
            );
          }
        results.push({
          scenario,
          preparation,
          captureResolution,
          captures: capturesByVariant,
          benchmark: paired.length
            ? {
                resolution: timingResolution,
                frames: timingFrames,
                serialGpu: scenario === "water",
                trials: paired,
                summary: summarizeAlternatingTrials(paired),
              }
            : null,
        });
      }
      const reference =
        await view.evaluate<Awaited<ReturnType<AcceptanceFixture["waterReference"]>>>(
          "fixture.waterReference()",
        );
      await Bun.write(join(output, "water-independent-reference.json"), JSON.stringify(reference, null, 2));
      const fixtureErrors = await view.evaluate<string[]>("fixture.errors");
      const failures = [
        ...errors,
        ...fixtureErrors,
        ...results.flatMap((result) =>
          result.captures.flatMap((run) =>
            run.diagnostics.filter((d) => d.severity === "error").map((d) => d.message),
          ),
        ),
      ];
      if (reference.maximumReferenceConvergence > 1e-6)
        failures.push("Independent water reference failed convergence");
      const squared = reference.cases.reduce(
        (sum, c) => sum + c.actual.reduce((s, v, i) => s + (v - c.reference[i]) ** 2, 0),
        0,
      );
      const energy = reference.cases.reduce((sum, c) => sum + c.reference.reduce((s, v) => s + v * v, 0), 0);
      const relativeRms = Math.sqrt(squared / Math.max(energy, reference.cases.length * 3 * 0.01 ** 2));
      if (relativeRms > 0.01 || reference.maximumRelativeError > 0.05)
        failures.push(
          `Production water quality failed: relative RMS ${relativeRms}; maximum query relative error ${reference.maximumRelativeError}`,
        );
      const report = {
        version: ACCEPTANCE_VERSION,
        status: failures.length ? "failed" : "captured",
        sourceManifest: "source-manifest.json",
        environmentManifest: "environment.json",
        trajectoryManifest: "trajectory.json",
        resolution: [width, height],
        profile: low ? "low" : "balanced",
        results,
        waterReference: { ...reference, relativeRms },
        methodology: {
          controls:
            "Production defaults are primary. Parametric mesh and disabled visibility are representation/cost controls. Water reference uses the same authored BRDF with denser integration, independently checked against CPU normal-vector GGX.",
          timing:
            "Repeated rotated/reversed matched trials after complete trajectory warmup. Only trajectory-frame indices with attributed GPU timing in every condition contribute. Report median trial p50/p95 and all observations, including atmosphere/scene/shadow/water/display and observed timestamp lattice. Metal pass intervals can overlap and scene includes water; these are not exclusive contributions. Water captures use matched 640×360 output; timing uses matched 320×180 output and 17 frames to bound the dense control workload. Both water conditions await GPU timing readback after every frame to avoid query-ring drops; that wait is outside GPU timestamps and CPU render-submission timing and is not a pacing measurement. Other captures use the reported output resolution, including --small staging. Captures and initialization are outside measured frames; CPU measures submission of pre-evaluated packets. Callback intervals are not presentation timestamps.",
          radiance:
            "Unclipped scene-linear rgba16float readback before display; relative RMS denominator floor0.01. Water image delta treats dense integration as reference. Independent water reference directly executes production WGSL and compares reflected response to16384/32768-sample CPU integration in documented constant-sky/unoccluded conditions.",
          completeness:
            "Every expected identity accounted once; uploading/rejected/duplicate/unknown identities fail. Targeted visibility requires exact GPU identity pixels and actual removed work. This does not certify every visibility scene.",
        },
        gates: {
          independentWaterRms: 0.01,
          independentWaterMaximumQueryRelative: 0.05,
          crossHardware: "pending",
          judgedSceneQuality: "requires image and playable-scene review",
          proposedSceneRadianceRms: {
            threshold: 0.01,
            status: "reported-not-certified",
            reason:
              "Dense scene quadrature is a cost/quality control, not an independently converged image reference. Geometry-control deltas are differences between valid realizations, not error against ground truth. Highlight maximum/p95 and trajectory deltas are retained per capture for review.",
          },
          temporalQuality: {
            status: "requires-motion-review",
            reason:
              "Geometry and water-reference trajectory deltas are reported at four camera/light/time anchors. These sparse samples require motion review and do not establish continuous temporal stability or an error bound.",
          },
          proposedGpuP95Improvement: {
            threshold: 0.2,
            comparisons: results.flatMap((result) => {
              const summary = result.benchmark?.summary;
              const production = summary?.find((sample) => sample.variant === "production");
              const productionP95 = production?.passes.gpuMs?.trialP95.p50;
              if (productionP95 === undefined) return [];
              return (summary ?? [])
                .filter((sample) => sample.variant !== "production")
                .map((sample) => {
                  const controlP95 = sample.passes.gpuMs?.trialP95.p50;
                  if (controlP95 === undefined || controlP95 <= 0)
                    throw Error("Missing positive control GPU p95");
                  const improvement = 1 - productionP95 / controlP95;
                  return {
                    scenario: result.scenario,
                    control: sample.variant,
                    productionP95Ms: productionP95,
                    controlP95Ms: controlP95,
                    improvement,
                    timingTargetMet: improvement >= 0.2,
                    matchedJudgedQuality: "requires-image-and-motion-review",
                  };
                });
            }),
            reason:
              "Timing target is reported independently of safety/correctness capture status. A faster dense-reference comparison alone does not prove an accepted equal-quality optimization.",
          },
        },
        errors: failures,
      };
      await Bun.write(join(output, "rendering-compiler.json"), JSON.stringify(report, null, 2));
      console.log(
        JSON.stringify(
          { output, status: report.status, independentWaterRelativeRms: relativeRms, errors: failures },
          null,
          2,
        ),
      );
      if (failures.length) throw Error(failures.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  width,
  height,
);
