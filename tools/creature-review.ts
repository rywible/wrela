import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { AuthoringSession } from "@wrela/authoring/session";
import {
  compileCreatureGeometry,
  projectCreatureAnchor,
  resolveCreatureAnchor,
} from "@wrela/compiler/creature";
import { prepareCreatureAttachments } from "@wrela/compiler/creature-attachments";
import { type CreatureFixtureId, createCreatureFixture } from "@wrela/examples/creature-fixtures";
import type { CharacterDefinition } from "@wrela/model/documents";
import { contentKey, type Vec3 } from "@wrela/model/math";
import { reviewCreature } from "@wrela/runtime/creature-review";
import { createCreatureCaptureRecipe } from "./creature-capture-recipes";
import {
  compareCreatureDeformation,
  compareCreatureGroom,
  compareCreatureSourceSolver,
  creatureExperimentPreservation,
  exerciseCreatureRecipes,
} from "./creature-experiments";

const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));
export function runCreatureExperiments(id: CreatureFixtureId) {
  const fixture = createCreatureFixture(id);
  const character = fixture.project.documents.find((document) => document.id === id) as CharacterDefinition;
  const original = character.creature!;
  const session = new AuthoringSession(fixture.project);
  const start = performance.now();
  const preservedRegions = creatureExperimentPreservation(character);
  let strictHeadPreservation: { status: "supported" | "rejected"; reason?: string } = { status: "supported" };
  try {
    session.preview({
      expectedRevision: 0,
      operations: [
        {
          kind: "creature.proportion",
          target: id,
          region: "shoulder",
          scale: [1.25, 1, 1],
          preserve: ["head-region", "jaw-region"],
        },
      ],
    });
  } catch (error) {
    strictHeadPreservation = { status: "rejected", reason: String(error) };
  }
  const candidate = session.proposeCandidate({
    id: "broader-shoulder",
    batch: {
      expectedRevision: 0,
      label: "Widen shoulder with explicit protected regions",
      operations: [
        {
          kind: "creature.proportion",
          target: id,
          region: "shoulder",
          scale: [1.25, 1, 1],
          propagate: "region",
          preserve: preservedRegions,
        },
      ],
    },
  });
  const proposed = candidate.changes.find((change) => change.id === id)!.after as CharacterDefinition;
  const edited = proposed.creature!;
  const mountedBefore = prepareCreatureAttachments(character);
  const mountedAfter = prepareCreatureAttachments(proposed);
  const mountedComponents = original.attachments.flatMap((attachment) =>
    attachment.nodeIds.map((nodeId) => {
      const before = mountedBefore.document.field.nodes.find((node) => node.id === nodeId)!;
      const after = mountedAfter.document.field.nodes.find((node) => node.id === nodeId)!;
      return {
        attachment: attachment.id,
        node: nodeId,
        displacement: distance(before.position, after.position),
        dimensionsPreserved: contentKey(before.size) === contentKey(after.size),
        diagnostics: mountedAfter.diagnostics.filter((diagnostic) => diagnostic.node === attachment.id),
      };
    }),
  );
  const anchorResults = original.anchors.map((anchor) => {
    const before = resolveCreatureAnchor(original, anchor);
    const next = edited.anchors.find((entry) => entry.id === anchor.id)!;
    const transported = resolveCreatureAnchor(edited, next);
    const nearest = before.position
      ? projectCreatureAnchor(edited, anchor.region, before.position, 1)
      : undefined;
    return {
      id: anchor.id,
      purpose: anchor.purpose,
      status: transported.status,
      sameRegion: next.region === anchor.region,
      sameChart: next.chart === anchor.chart,
      preservedCoordinates: contentKey(next.coordinates) === contentKey(anchor.coordinates),
      displacement:
        before.position && transported.position ? distance(before.position, transported.position) : null,
      nearestProjectionStatus: nearest?.status ?? "unavailable",
      nearestProjectionDistanceFromTransport:
        nearest?.position && transported.position ? distance(nearest.position, transported.position) : null,
    };
  });
  const changedTopology = structuredClone(edited);
  changedTopology.charts[0].revision++;
  const invalidated = changedTopology.anchors
    .filter((anchor) => anchor.chart === changedTopology.charts[0].id)
    .map((anchor) => ({ id: anchor.id, status: resolveCreatureAnchor(changedTopology, anchor).status }));
  const adopted = session.adoptCandidate(candidate.id, { transactionId: "adopt-broader-shoulder" });
  session.revert(adopted.transactionId, { transactionId: "restore-shoulder" });
  const restored = session.inspect(id) as CharacterDefinition;
  const compileStart = performance.now();
  const geometry = compileCreatureGeometry(original, "review", character.material);
  const compileMilliseconds = performance.now() - compileStart;
  const patches = original.charts
    .filter((chart) => chart.kind === "patch")
    .map((chart) => ({
      id: chart.id,
      thickness: chart.thickness,
      vertices: geometry.coordinates.filter((coordinate) => coordinate?.chart === chart.id).length,
    }));
  return {
    version: 1,
    fixture: id,
    sourceRevision: contentKey(character),
    evidence: "cpu-only",
    visualApproval: "not-reviewed",
    E1: {
      status:
        anchorResults.every(
          (anchor) =>
            anchor.status === "resolved" &&
            anchor.sameRegion &&
            anchor.sameChart &&
            anchor.preservedCoordinates,
        ) &&
        invalidated.every((anchor) => anchor.status === "invalid") &&
        mountedComponents.every(
          (component) =>
            component.dimensionsPreserved &&
            component.diagnostics.every((diagnostic) => diagnostic.severity !== "error"),
        )
          ? "passed-source-correspondence"
          : "failed",
      edit: "Public candidate: shoulder width ×1.25 with named protected regions",
      preservedRegions,
      strictHeadPreservation,
      anchorResults,
      mountedComponents,
      topologyChange: invalidated,
      limitations: [
        "Nearest projection is a bounded within-region baseline, not a general nearest mesh surface baseline.",
        "Source anchors and mounted component positions/dimensions are measured; lit scar and armor attachment coherence still requires captures.",
        "Reed Penitent's shoulder owns an off-center neck pivot. Strict head preservation rejects that width edit; its accepted numerical comparison protects cloth regions instead and does not claim head preservation.",
      ],
    },
    E2: {
      status: "partial-numerical-evidence",
      compileMilliseconds,
      chartCount: original.charts.length,
      generatedVertices: geometry.mesh.positions.length / 3,
      generatedTriangles: geometry.mesh.indices.length / 3,
      uniformFieldSpacing: character.field.bounds.max.map(
        (value, axis) => (value - character.field.bounds.min[axis]) / character.field.resolution,
      ),
      patches,
      diagnostics: geometry.diagnostics,
      limitations: [
        "Patch thickness and explicit chart vertices show retained source features, not matched visual superiority.",
        "Anatomical shape, silhouette, deformation, authoring effort, and thin-feature visibility await matched visual review.",
      ],
    },
    E3: {
      ...compareCreatureDeformation(character),
      scenarios: original.reviewScenarios.map((scenario) => reviewCreature(character, scenario)),
    },
    E4: compareCreatureGroom(character),
    E5: compareCreatureSourceSolver(fixture.project, character),
    E6: {
      status: "not-run",
      reason:
        "No matched dense-detail, filtered-product, and field-response experiment has been implemented. Moving-light/camera/pose error and temporal behavior require visual hardware evidence.",
    },
    E7: {
      status: contentKey(restored) === contentKey(character) ? "passed-transaction-roundtrip" : "failed",
      commandCount: 1,
      candidateDidNotMutateAcceptedSource: candidate.sourceRevision === 0,
      restoredExactly: contentKey(restored) === contentKey(character),
      reuse: exerciseCreatureRecipes(fixture.project, character),
      elapsedMilliseconds: performance.now() - start,
      limitations: [
        "This is one bounded cross-system source edit, not completion of the full authoring-loop experiment.",
      ],
    },
    hardware: {
      status: "not-run",
      reason:
        "This CPU experiment runner never launches a browser; hardware evidence is retained separately by the smoke and Studio verification tools.",
    },
  };
}

if (import.meta.main) {
  const requested = process.argv.slice(2).filter((argument) => !argument.startsWith("--"));
  if (requested.some((id) => !["ash-warden", "reed-penitent"].includes(id)))
    throw new Error("Use ash-warden and/or reed-penitent; --output=path writes a JSON report.");
  const ids = (requested.length ? requested : ["ash-warden", "reed-penitent"]) as CreatureFixtureId[];
  const report = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    fixtures: ids.map(runCreatureExperiments),
  };
  const output = process.argv.find((argument) => argument.startsWith("--output="))?.slice("--output=".length);
  if (output) await Bun.write(output, `${JSON.stringify(report, null, 2)}\n`);
  const captureDirectory = process.argv
    .find((argument) => argument.startsWith("--capture-sources="))
    ?.slice("--capture-sources=".length);
  if (captureDirectory) {
    await mkdir(captureDirectory, { recursive: true });
    for (const id of ids)
      for (const mode of ["clay", "skeleton"] as const) {
        const recipe = createCreatureCaptureRecipe(id, mode);
        await Bun.write(join(captureDirectory, `${id}-${mode}.json`), `${JSON.stringify(recipe, null, 2)}\n`);
      }
  }
  console.log(
    JSON.stringify(
      output
        ? {
            output,
            captureDirectory,
            fixtures: report.fixtures.map((fixture) => ({
              id: fixture.fixture,
              source: fixture.sourceRevision,
              correspondence: fixture.E1.status,
              scenarios: fixture.E3.scenarios.map((scenario) => ({
                id: scenario.scenario,
                status: scenario.status,
              })),
              groomGuides: fixture.E4.guideCount,
              solver: fixture.E5.status,
              visualApproval: fixture.visualApproval,
            })),
          }
        : report,
      null,
      2,
    ),
  );
}
