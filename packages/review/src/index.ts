import {
  type AuthoringReview,
  type ConstraintResult,
  compareSilhouettes,
  type ResultConstraint,
  resultConstraintSchema,
  reviewSourcePreservation,
} from "@wrela/authoring";
import {
  assemblyModuleObstructs,
  botanicalStructure,
  compileCharacter,
  compileDocument,
} from "@wrela/compiler";
import { contentKey, type GpuFrameTiming, type Project, parseProject } from "@wrela/model";
import { auditCreatureMotion } from "@wrela/runtime";
import { defaultWorldReview, reviewWorldComposition } from "@wrela/world";
import { renderSource, silhouetteMask } from "./browser";

export { captureAuthoring } from "./browser";
export { reviewStage } from "./review-stage";
export { createAuthoringReviewSession } from "./session";
export { reviewAuthoringStudy } from "./study";

export type ReviewOptions = {
  signal?: AbortSignal;
  saveArtifact?: (name: string, value: Blob | object) => Promise<string>;
  onProgress?: (result: ConstraintResult) => void;
};
const p95 = (values: number[]) => [...values].sort((a, b) => a - b)[Math.ceil(values.length * 0.95) - 1];
/** Same evaluator is used by the Studio API and the CLI hardware fixture. */
export async function reviewAuthoring(
  baselineInput: Project,
  candidateInput: Project,
  inputs: ResultConstraint[],
  options: ReviewOptions = {},
): Promise<AuthoringReview> {
  const baseline = parseProject(baselineInput),
    project = parseProject(candidateInput),
    constraints = inputs.map((c) => resultConstraintSchema.parse(c));
  if (constraints.length > 64 || new Set(constraints.map((c) => c.id)).size !== constraints.length)
    throw Error("Review needs unique bounded constraints");
  for (const doc of project.documents) {
    if (doc.kind !== "vegetation" || !doc.botanical || doc.botanical.development) continue;
    const before = baseline.documents.find((d) => d.id === doc.id),
      knownEdits = before?.kind === "vegetation" ? (before.botanical?.branchEdits ?? []) : [];
    const introduced = doc.botanical.branchEdits.filter(
      (e) => !knownEdits.some((old) => contentKey(old) === contentKey(e)),
    );
    if (introduced.length) {
      const branches = new Set(botanicalStructure(doc).branches.map((b) => b.id));
      for (const edit of introduced)
        if (!branches.has(edit.branch))
          throw Error(`Edited botanical branch is not realized: ${edit.branch}`);
    }
  }
  const started = performance.now(),
    results: ConstraintResult[] = [];
  for (const constraint of constraints) {
    options.signal?.throwIfAborted();
    const evidence: string[] = [];
    const save = async (name: string, value: Blob | object) => {
      if (options.saveArtifact) evidence.push(await options.saveArtifact(`${constraint.id}-${name}`, value));
    };
    let result: ConstraintResult;
    try {
      if (constraint.kind === "source") {
        const check = reviewSourcePreservation(baseline, project, [
          { target: constraint.target, path: constraint.path, reason: constraint.reason },
        ])[0];
        result = {
          id: constraint.id,
          status: check.passed ? "passed" : "failed",
          measured: { equal: check.passed },
          evidence,
          scope: "Exact authored-source equality; appearance is not implied",
        };
      } else if (constraint.kind === "geometry") {
        const doc = project.documents.find((d) => d.id === constraint.target);
        if (!doc) throw Error("Geometry target missing");
        const artifact = compileDocument(doc, "review");
        if (!artifact) throw Error("Target has no compiled geometry");
        const bounds = artifact.kind === "vegetation" ? artifact.bounds : artifact.mesh.bounds;
        const meshes =
          artifact.kind === "vegetation" ? artifact.surfaces.map((s) => s.mesh) : [artifact.mesh];
        const triangles = meshes.reduce(
          (n, m) => n + (m.indices.length / 3) * (m.shoots?.sourceIds.length ?? 1),
          0,
        );
        const extents = bounds.max.map((n, i) => n - bounds.min[i]);
        result = {
          id: constraint.id,
          status:
            triangles >= constraint.minTriangles &&
            triangles <= constraint.maxTriangles &&
            extents.every(
              (v, i) =>
                Number.isFinite(v) && v >= constraint.minimumExtent[i] && v <= constraint.maximumExtent[i],
            )
              ? "passed"
              : "failed",
          evidence,
          measured: { triangles, extentX: extents[0], extentY: extents[1], extentZ: extents[2] },
          scope:
            "Compiled local bounds and expanded instance triangle count. Not a visual-quality score or measured GPU budget.",
        };
      } else if (constraint.kind === "dimensions" || constraint.kind === "clearance") {
        const doc = project.documents.find((d) => d.id === constraint.target);
        if (!doc) throw Error("Dimension target is missing");
        const artifact = compileDocument(doc, "review");
        if (!artifact || artifact.kind === "vegetation")
          throw Error("Dimension review requires a finite object or character");
        const mesh = artifact.mesh;
        if (constraint.kind === "clearance") {
          const positions = Array.from(mesh.indices).flatMap((i) => [
            mesh.positions[i * 3],
            mesh.positions[i * 3 + 1],
            mesh.positions[i * 3 + 2],
          ]);
          if (!positions.length) throw Error("Clearance target has no triangles");
          const blocked = assemblyModuleObstructs(positions, 0, positions.length, {
            min: constraint.minimum,
            max: constraint.maximum,
          });
          result = {
            id: constraint.id,
            evidence,
            status: blocked ? "failed" : "passed",
            measured: { blocked, triangles: mesh.indices.length / 3 },
            scope:
              "Compiled rest-local triangle/box intersection and closed-surface winding; depends on watertight outward-wound source, not an analytic field proof",
          };
        } else {
          let bounds = mesh.bounds;
          if (constraint.part) {
            const vertices = mesh.sourceIds?.flatMap((id, i) => (id === constraint.part ? [i] : []));
            if (!vertices?.length) throw Error("Requested part has no attributed vertices");
            bounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
            for (const i of vertices)
              for (let axis = 0; axis < 3; axis++) {
                bounds.min[axis] = Math.min(bounds.min[axis], mesh.positions[i * 3 + axis]);
                bounds.max[axis] = Math.max(bounds.max[axis], mesh.positions[i * 3 + axis]);
              }
          }
          const errors = [
            ...bounds.min.map((v, i) => Math.abs(v - constraint.minimum[i])),
            ...bounds.max.map((v, i) => Math.abs(v - constraint.maximum[i])),
          ];
          const maxError = Math.max(...errors);
          result = {
            id: constraint.id,
            evidence,
            status: maxError <= constraint.tolerance ? "passed" : "failed",
            measured: {
              maxError,
              ...Object.fromEntries(bounds.min.map((v, i) => [`min${i}`, v])),
              ...Object.fromEntries(bounds.max.map((v, i) => [`max${i}`, v])),
            },
            scope:
              "Compiled mesh or attributed-part axis-aligned bounds in rest-local metres; not internal clearance or animated bounds",
          };
        }
      } else if (constraint.kind === "route" || constraint.kind === "sightline") {
        const world = structuredClone(project.documents.find((d) => d.id === constraint.target));
        if (world?.kind !== "world" || !world.composition)
          throw Error("Constraint target needs a composed world");
        const terrain = project.documents.find((d) => d.id === world.terrain);
        if (terrain?.kind !== "terrain") throw Error("World terrain is missing");
        if (constraint.kind === "sightline")
          world.composition.review = {
            ...(world.composition.review ?? defaultWorldReview()),
            sightline: { from: constraint.from, to: constraint.to },
          };
        const report = reviewWorldComposition(world, terrain, project.documents);
        await save("spatial.json", report);
        if (constraint.kind === "route") {
          const route = report.routes.find((r) => r.id === constraint.route);
          if (!route || !route.samples.length) throw Error("Required route has no samples");
          result = {
            id: constraint.id,
            status: route.blockedSamples === 0 && !route.narrow ? "passed" : "failed",
            evidence,
            measured: {
              blockedSamples: route.blockedSamples,
              narrow: route.narrow,
              sampleSpacing: route.sampleSpacing,
              samples: route.samples.length,
            },
            scope:
              "Compiled assembly bounds and sampled graded terrain. No free-space navmesh or scattered vegetation collision guarantee.",
          };
        } else
          result = {
            id: constraint.id,
            status: report.sightline.clear ? "passed" : "failed",
            evidence,
            measured: {
              clear: report.sightline.clear,
              obstruction: report.sightline.obstruction?.id ?? null,
            },
            scope: report.approximation,
          };
      } else if (constraint.kind === "motion") {
        const character = project.documents.find((d) => d.id === constraint.target);
        if (character?.kind !== "character") throw Error("Motion constraint requires a character");
        const audit = await auditCreatureMotion(
          character,
          compileCharacter(character, "review"),
          constraint.motion,
        );
        await save("motion.json", audit);
        const conflicts = audit.samples.reduce((n, s) => n + s.issues.length, 0);
        const measured = {
          slip: audit.maximumPlantedSlip,
          residual: audit.maximumContactResidual,
          contacts: audit.plantedContacts.length,
          conflicts,
        };
        result = {
          id: constraint.id,
          evidence,
          measured,
          scope: audit.scope,
          status: !audit.plantedContacts.length
            ? "unmeasured"
            : !conflicts && measured.slip <= constraint.maxSlip && measured.residual <= constraint.maxResidual
              ? "passed"
              : "failed",
        };
      } else if (constraint.kind === "silhouette") {
        const masks: Uint8Array[] = [];
        for (const [label, source] of [
          ["baseline", baseline],
          ["candidate", project],
        ] as const) {
          options.signal?.throwIfAborted();
          const render = await renderSource(
            source,
            constraint.target,
            constraint.stage,
            constraint.camera,
            constraint.width,
            constraint.height,
          );
          try {
            const blob = await render.capture("silhouette", constraint.tick);
            await save(`${label}.png`, blob);
            masks.push(await silhouetteMask(blob, constraint.width, constraint.height));
          } finally {
            render.dispose();
          }
        }
        const measured = compareSilhouettes(
          masks[0],
          masks[1],
          constraint.width,
          constraint.height,
          constraint.region,
        );
        result = {
          id: constraint.id,
          evidence,
          measured,
          status:
            measured.changedFraction === null || !measured.baselinePixels || !measured.candidatePixels
              ? "unmeasured"
              : measured.changedFraction <= constraint.maxChangedFraction
                ? "passed"
                : "failed",
          scope:
            "Matched hardware binary silhouette symmetric difference / union in the declared camera, time, resolution and region. Not a global surface error bound.",
        };
      } else {
        const render = await renderSource(
          project,
          constraint.target,
          constraint.stage,
          constraint.camera,
          constraint.width,
          constraint.height,
        );
        try {
          await render.capture("beauty", 0);
          const cpu: number[] = [],
            timings: GpuFrameTiming[] = [],
            frames: number[] = [];
          let peakBytes = 0,
            incomplete = 0;
          for (let i = 0; i < 30 + constraint.frames; i++) {
            options.signal?.throwIfAborted();
            await new Promise(requestAnimationFrame);
            const start = performance.now();
            render.host.advance(1 / 60, constraint.camera);
            const scene = render.host.extract(constraint.camera);
            scene.grid = false;
            render.renderer.render(scene);
            const elapsed = performance.now() - start;
            timings.push(...render.renderer.drainGpuTimings());
            if (i >= 30) {
              cpu.push(elapsed);
              frames.push(render.renderer.measurements.frame);
              peakBytes = Math.max(peakBytes, render.renderer.measurements.gpuBytes);
              if (!render.renderer.completeness.complete) incomplete++;
            }
          }
          await render.renderer.flushGpuTimings();
          timings.push(...render.renderer.drainGpuTimings());
          const matched = timings.filter((t) => frames.includes(t.frame));
          const completeTimings =
            matched.length === frames.length && new Set(matched.map((t) => t.frame)).size === frames.length;
          const measured = {
            cpuP95Ms: p95(cpu),
            gpuP95Ms: completeTimings ? p95(matched.map((t) => t.gpuMs)) : null,
            peakGpuBytes: peakBytes,
            frames: frames.length,
            gpuSamples: matched.length,
            incomplete,
            adapter: render.renderer.measurements.adapter,
          };
          await save("frames.json", { measured, frames, timings: matched, cpu, errors: render.errors });
          result = {
            id: constraint.id,
            measured,
            evidence,
            status: !completeTimings
              ? "unmeasured"
              : !incomplete &&
                  !render.errors.length &&
                  measured.cpuP95Ms <= constraint.maxCpuMs &&
                  measured.gpuP95Ms !== null &&
                  measured.gpuP95Ms <= constraint.maxGpuMs &&
                  peakBytes <= constraint.maxGpuBytes
                ? "passed"
                : "failed",
            scope:
              "Stationary camera with fixed-step simulation; 30 warmup frames then attributed GPU timestamps and CPU simulation/extraction/submission. Owned GPU allocation excludes driver memory. Not broad hardware qualification.",
          };
        } finally {
          render.dispose();
        }
      }
    } catch (error) {
      options.signal?.throwIfAborted();
      result = {
        id: constraint.id,
        status: "unmeasured",
        measured: {},
        evidence,
        scope: "Review could not establish this constraint",
        reason: String(error),
      };
    }
    results.push(result);
    options.onProgress?.(result);
  }
  return {
    version: 1,
    baselineKey: contentKey(baseline),
    candidateKey: contentKey(project),
    constraintsKey: contentKey(constraints),
    results,
    durationMs: performance.now() - started,
    createdAt: new Date().toISOString(),
    evaluator: "wrela-authoring-review-1",
  };
}

export { reviewAuthoringPacket } from "./packet";
