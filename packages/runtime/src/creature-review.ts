import { resolveCreatureAnchor } from "@wrela/compiler/creature";
import { type CharacterDefinition, type CreatureReviewScenario, contentKey, type Vec3 } from "@wrela/model";
import { validateCreature } from "@wrela/model/creature-validation";
import { sampleMotion } from "./animation";
import { createCreatureRuntimeState, creatureJointFrames, evaluateCreaturePose } from "./creature-runtime";

export type CreatureReviewDiagnostic = {
  kind: "source" | "anchor" | "contact" | "penetration" | "stretch" | "solver" | "finite";
  source: string;
  time?: number;
  measured: number | null;
  threshold: number | null;
  passed: boolean;
  message: string;
};
export type CreatureReviewReport = {
  version: 1;
  character: string;
  scenario: string;
  sourceRevision: string;
  scenarioRevision: string;
  status: "passed" | "failed" | "unavailable";
  evidence: "cpu-simulation";
  visualApproval: "not-reviewed";
  sampleCount: number;
  metrics: {
    anchorCount: number;
    contactSamples: number;
    baselineContactError: number | null;
    solvedContactError: number | null;
    maximumPlantedSlip: number | null;
    maximumPenetration: number;
    maximumBoneStretch: number;
  };
  diagnostics: CreatureReviewDiagnostic[];
  captures: { camera: string; status: "pending" }[];
  limitations: string[];
};
const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));

/** Pure bounded CPU review. No browser, renderer, file I/O, or source mutation.
 * Measures joint/contact behavior, not skin volume, image quality, or gameplay quality. */
export function reviewCreature(
  character: CharacterDefinition,
  scenario: CreatureReviewScenario,
): CreatureReviewReport {
  const report: CreatureReviewReport = {
    version: 1,
    character: character.id,
    scenario: scenario.id,
    sourceRevision: contentKey(character),
    scenarioRevision: contentKey(scenario),
    status: "unavailable",
    evidence: "cpu-simulation",
    visualApproval: "not-reviewed",
    sampleCount: 0,
    metrics: {
      anchorCount: 0,
      contactSamples: 0,
      baselineContactError: null,
      solvedContactError: null,
      maximumPlantedSlip: null,
      maximumPenetration: 0,
      maximumBoneStretch: 0,
    },
    diagnostics: [],
    captures: scenario.cameras.map((camera) => ({ camera: camera.id, status: "pending" })),
    limitations: [
      "CPU evidence cannot approve appearance, groom shadows, temporal LOD, or encounter quality.",
      "Penetration measures foot-joint clearance against the declared flat review floor; full deformed-surface self-collision is not measured.",
      "Bone stretch does not measure skin volume preservation or artistic deformation quality.",
    ],
  };
  const source = character.creature;
  if (!source) {
    report.diagnostics.push({
      kind: "source",
      source: character.id,
      measured: null,
      threshold: null,
      passed: false,
      message: "Character has no anatomical creature source.",
    });
    return report;
  }
  const problems = validateCreature(character).filter((diagnostic) => diagnostic.severity === "error");
  for (const problem of problems)
    report.diagnostics.push({
      kind: "source",
      source: problem.node ?? character.id,
      measured: null,
      threshold: null,
      passed: false,
      message: problem.message,
    });
  const motion = scenario.motion
    ? character.motions.find((entry) => entry.id === scenario.motion)
    : undefined;
  if (scenario.motion && !motion)
    report.diagnostics.push({
      kind: "source",
      source: scenario.motion,
      measured: null,
      threshold: null,
      passed: false,
      message: "Review motion is missing.",
    });
  if (
    !(scenario.duration > 0) ||
    !Number.isFinite(scenario.duration) ||
    !(scenario.sampleRate >= 1) ||
    scenario.sampleRate > 120 ||
    Math.ceil(scenario.duration * scenario.sampleRate) > 14400
  )
    report.diagnostics.push({
      kind: "source",
      source: scenario.id,
      measured: null,
      threshold: null,
      passed: false,
      message: "Scenario exceeds the bounded CPU review domain.",
    });
  if (report.diagnostics.length) return report;
  for (const anchor of source.anchors) {
    const resolved = resolveCreatureAnchor(source, anchor);
    const threshold = Math.min(anchor.tolerance, scenario.thresholds.anchorError);
    report.metrics.anchorCount++;
    report.diagnostics.push({
      kind: "anchor",
      source: anchor.id,
      measured: resolved.residual,
      threshold,
      passed: resolved.status === "resolved" && resolved.residual !== null && resolved.residual <= threshold,
      message:
        resolved.status === "resolved"
          ? "Anchor resolves within its authored anatomical chart."
          : resolved.diagnostics.map((d) => d.message).join(" "),
    });
  }
  const state = createCreatureRuntimeState();
  const steps = Math.ceil(scenario.duration * scenario.sampleRate);
  const dt = scenario.duration / steps;
  const firstPlant = new Map<string, Vec3>();
  const maxBySource = new Map<string, CreatureReviewDiagnostic>();
  const recordMaximum = (diagnostic: CreatureReviewDiagnostic) => {
    const key = `${diagnostic.kind}:${diagnostic.source}`;
    const prior = maxBySource.get(key);
    if (!prior || (diagnostic.measured ?? Infinity) > (prior.measured ?? Infinity))
      maxBySource.set(key, diagnostic);
  };
  let baselineError = 0,
    solvedError = 0,
    slip = 0;
  const restJoints = new Map(character.joints.map((j) => [j.id, j]));
  for (let step = 0; step <= steps; step++) {
    const time = step * dt;
    const baseline = sampleMotion(character.joints, motion, time);
    const baselineFrames = creatureJointFrames(character.joints, baseline);
    // Fixed substeps keep simulation stable even for a low review sample rate.
    const substeps = Math.max(1, Math.ceil(dt * 60));
    let evaluated = evaluateCreaturePose(
      character.joints,
      source,
      baseline,
      state,
      {
        position: [0, 0, 0],
        rotation: [0, 0, 0, 1],
        scale: 1,
        time,
        motion,
        motionTime: time,
        ground: () => ({ height: 0, normal: [0, 1, 0] }),
      },
      0,
    );
    if (step > 0)
      for (let substep = 1; substep <= substeps; substep++) {
        const subtime = time - dt + (substep * dt) / substeps;
        evaluated = evaluateCreaturePose(
          character.joints,
          source,
          sampleMotion(character.joints, motion, subtime),
          state,
          {
            position: [0, 0, 0],
            rotation: [0, 0, 0, 1],
            scale: 1,
            time: subtime,
            motion,
            motionTime: subtime,
            ground: () => ({ height: 0, normal: [0, 1, 0] }),
          },
          dt / substeps,
        );
      }
    const frames = creatureJointFrames(character.joints, evaluated.pose);
    report.sampleCount++;
    for (const [id, frame] of frames) {
      if (![...frame.position, ...frame.rotation].every(Number.isFinite))
        recordMaximum({
          kind: "finite",
          source: id,
          time,
          measured: null,
          threshold: null,
          passed: false,
          message: "Pose contains a non-finite joint transform.",
        });
      const joint = restJoints.get(id)!;
      if (!joint.parent) continue;
      const parent = frames.get(joint.parent)!;
      const restLength = distance(joint.position, restJoints.get(joint.parent)!.position);
      if (restLength < 1e-8) continue;
      const stretch = Math.abs(distance(frame.position, parent.position) / restLength - 1);
      report.metrics.maximumBoneStretch = Math.max(report.metrics.maximumBoneStretch, stretch);
      recordMaximum({
        kind: "stretch",
        source: id,
        time,
        measured: stretch,
        threshold: scenario.thresholds.stretch,
        passed: stretch <= scenario.thresholds.stretch,
        message: "Relative joint-segment length change from authored rest length.",
      });
    }
    const cycleTime = motion?.loop ? time % motion.duration : time;
    // A foot can penetrate during a clip without a planted contact track too.
    for (const contact of new Map(source.contacts.map((entry) => [entry.joint, entry])).values()) {
      const position = frames.get(contact.joint)!.position;
      const penetration = Math.max(0, contact.offset - position[1]);
      report.metrics.maximumPenetration = Math.max(report.metrics.maximumPenetration, penetration);
      recordMaximum({
        kind: "penetration",
        source: contact.joint,
        time,
        measured: penetration,
        threshold: scenario.thresholds.penetration,
        passed: penetration <= scenario.thresholds.penetration,
        message: "Foot contact point below the declared review floor.",
      });
    }
    for (const contact of source.contacts.filter((c) => c.motion === motion?.id)) {
      const position = frames.get(contact.joint)!.position;
      if (cycleTime < contact.start + contact.blendIn || cycleTime >= contact.end - contact.blendOut) {
        firstPlant.delete(contact.id);
        continue;
      }
      const target: Vec3 = contact.ground
        ? [contact.target[0], contact.offset, contact.target[2]]
        : contact.target;
      const before = distance(baselineFrames.get(contact.joint)!.position, target);
      const after = distance(position, target);
      baselineError = Math.max(baselineError, before);
      solvedError = Math.max(solvedError, after);
      report.metrics.contactSamples++;
      const start = firstPlant.get(contact.id) ?? position;
      firstPlant.set(contact.id, start);
      slip = Math.max(slip, distance(start, position));
      recordMaximum({
        kind: "contact",
        source: contact.id,
        time,
        measured: after,
        threshold: contact.tolerance,
        passed: after <= contact.tolerance,
        message: "Planted contact target residual after constraint evaluation.",
      });
      recordMaximum({
        kind: "contact",
        source: `${contact.id}-slip`,
        time,
        measured: distance(start, position),
        threshold: scenario.thresholds.contactSlip,
        passed: distance(start, position) <= scenario.thresholds.contactSlip,
        message: "Displacement from the first fully planted sample of this interval.",
      });
    }
    for (const diagnostic of evaluated.diagnostics)
      if (["unavailable", "conflict"].includes(diagnostic.status))
        recordMaximum({
          kind: "solver",
          source: diagnostic.id,
          time,
          measured: diagnostic.residual,
          threshold: null,
          passed: false,
          message: diagnostic.message ?? diagnostic.status,
        });
  }
  if (report.metrics.contactSamples) {
    report.metrics.baselineContactError = baselineError;
    report.metrics.solvedContactError = solvedError;
    report.metrics.maximumPlantedSlip = slip;
  } else
    report.limitations.push(
      "This scenario has no fully weighted planted contacts; contact error and slip are unavailable.",
    );
  report.diagnostics.push(...maxBySource.values());
  report.status = report.diagnostics.every((diagnostic) => diagnostic.passed) ? "passed" : "failed";
  return report;
}
