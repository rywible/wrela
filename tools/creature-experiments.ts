import { AuthoringSession } from "@wrela/authoring/session";
import {
  applyCreatureCorrectives,
  bindCreatureGeometry,
  type CreatureGeometry,
  compileCreatureCorrectives,
  compileCreatureGeometry,
} from "@wrela/compiler/creature";
import { compileCharacter } from "@wrela/compiler/surface";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import type { MeshData } from "@wrela/model/contracts";
import type { CharacterDefinition, Project } from "@wrela/model/documents";
import { contentKey, type Vec3 } from "@wrela/model/math";
import { instantiateRecipe } from "@wrela/model/recipes";
import { poseMatrices, quatFromEuler, sampleMotion } from "@wrela/runtime/animation";

type Binding = ReturnType<typeof bindCreatureGeometry>;
const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));
export function creatureExperimentPreservation(character: CharacterDefinition) {
  return character.id === "reed-penitent" ? ["cloth-left", "cloth-right"] : ["head-region", "jaw-region"];
}
function skin(mesh: MeshData, binding: Binding, matrices: Float32Array): Float32Array {
  const result = new Float32Array(mesh.positions.length);
  for (let vertex = 0; vertex < result.length / 3; vertex++) {
    const x = mesh.positions[vertex * 3],
      y = mesh.positions[vertex * 3 + 1],
      z = mesh.positions[vertex * 3 + 2];
    for (let influence = 0; influence < 4; influence++) {
      const slot = vertex * 4 + influence,
        matrix = binding.jointIndices[slot] * 16,
        weight = binding.weights[slot];
      for (let axis = 0; axis < 3; axis++)
        result[vertex * 3 + axis] +=
          weight *
          (matrices[matrix + axis] * x +
            matrices[matrix + 4 + axis] * y +
            matrices[matrix + 8 + axis] * z +
            matrices[matrix + 12 + axis]);
    }
  }
  return result;
}
function shoulderVolume(geometry: CreatureGeometry, positions: Float32Array) {
  let sixVolume = 0;
  const p = (index: number): Vec3 => [
    positions[index * 3],
    positions[index * 3 + 1],
    positions[index * 3 + 2],
  ];
  for (let index = 0; index < geometry.mesh.indices.length; index += 3) {
    const vertices = [...geometry.mesh.indices.slice(index, index + 3)];
    if (
      !vertices.every(
        (vertex) =>
          geometry.regions[vertex] === "shoulder" &&
          geometry.coordinates[vertex]?.chart === "shoulder-surface",
      )
    )
      continue;
    const [a, b, c] = vertices.map(p);
    sixVolume +=
      a[0] * (b[1] * c[2] - b[2] * c[1]) +
      a[1] * (b[2] * c[0] - b[0] * c[2]) +
      a[2] * (b[0] * c[1] - b[1] * c[0]);
  }
  return Math.abs(sixVolume / 6);
}
export function compareCreatureDeformation(character: CharacterDefinition) {
  const source = character.creature!;
  // E3 intentionally samples a closed anatomical chart as a measurement proxy, even
  // when the production chart is correspondence-only and renders no duplicate skin.
  const proxySource = structuredClone(source);
  proxySource.charts.find((chart) => chart.id === "shoulder-surface")!.realization = "surface";
  const geometry = compileCreatureGeometry(proxySource, "review", character.material);
  const anatomical = bindCreatureGeometry(character.joints, source, geometry);
  const unrestricted = {
    ...source,
    regions: source.regions.map((region) => ({ ...region, jointIds: [] })),
    influenceRules: [],
    attachments: [],
  };
  const envelope = bindCreatureGeometry(character.joints, unrestricted, geometry);
  const correctives = compileCreatureCorrectives(source, geometry);
  const baselineVolume = shoulderVolume(geometry, geometry.mesh.positions);
  const shoulderIndices = geometry.regions.flatMap((region, index) => (region === "shoulder" ? [index] : []));
  const allowed = new Set(
    source.influenceRules.filter((rule) => rule.region === "shoulder").flatMap((rule) => rule.allowedJoints),
  );
  const influenceLeakage = (binding: Binding) =>
    shoulderIndices.reduce(
      (sum, vertex) =>
        sum +
        Array.from({ length: 4 }, (_, index) => {
          const slot = vertex * 4 + index;
          return allowed.has(character.joints[binding.jointIndices[slot]].id) ? 0 : binding.weights[slot];
        }).reduce((a, b) => a + b, 0),
      0,
    ) / Math.max(1, shoulderIndices.length);
  const poses = [
    { id: "compressed-shoulder", spine: [0.4, 0, 0] as Vec3, limb: [0.95, 0, 0] as Vec3 },
    { id: "torso-twist", spine: [0, 0.6, 0] as Vec3, limb: [-0.7, 0, 0.2] as Vec3 },
  ].map((description) => {
    const base = sampleMotion(character.joints, undefined, 0);
    base.set("spine", { rotation: quatFromEuler(description.spine), translation: [0, 0, 0] });
    const active = new Map(base);
    for (const joint of character.joints.filter((joint) => joint.id.endsWith("-upper")))
      active.set(joint.id, { rotation: quatFromEuler(description.limb), translation: [0, 0, 0] });
    const neutralMatrices = poseMatrices(character.joints, base),
      activeMatrices = poseMatrices(character.joints, active);
    const envelopeBase = skin(geometry.mesh, envelope, neutralMatrices),
      envelopeActive = skin(geometry.mesh, envelope, activeMatrices);
    const anatomicalBase = skin(geometry.mesh, anatomical, neutralMatrices),
      anatomicalActive = skin(geometry.mesh, anatomical, activeMatrices);
    const corrected = applyCreatureCorrectives(geometry.mesh, correctives, { spine: description.spine });
    let outsideCorrectiveDisplacement = 0,
      correctiveDisplacement = 0;
    for (let index = 0; index < geometry.regions.length; index++) {
      const delta = Math.hypot(
        ...[0, 1, 2].map(
          (axis) => corrected.positions[index * 3 + axis] - geometry.mesh.positions[index * 3 + axis],
        ),
      );
      if (geometry.regions[index] !== "shoulder")
        outsideCorrectiveDisplacement = Math.max(outsideCorrectiveDisplacement, delta);
      else correctiveDisplacement = Math.max(correctiveDisplacement, delta);
    }
    const drift = (a: Float32Array, b: Float32Array) =>
      Math.max(
        0,
        ...shoulderIndices.map((index) =>
          distance(
            [a[index * 3], a[index * 3 + 1], a[index * 3 + 2]],
            [b[index * 3], b[index * 3 + 1], b[index * 3 + 2]],
          ),
        ),
      );
    return {
      id: description.id,
      unrelatedLimbShoulderDisplacement: {
        envelope: drift(envelopeBase, envelopeActive),
        anatomical: drift(anatomicalBase, anatomicalActive),
      },
      correctiveDisplacement,
      outsideCorrectiveDisplacement,
      closedShoulderVolumeRatio: {
        envelope: shoulderVolume(geometry, envelopeActive) / baselineVolume,
        anatomical: shoulderVolume(geometry, anatomicalActive) / baselineVolume,
        corrected: shoulderVolume(geometry, skin(corrected, anatomical, activeMatrices)) / baselineVolume,
      },
    };
  });
  return {
    status: "measured-chart-deformation",
    vertices: geometry.mesh.positions.length / 3,
    meanDisallowedShoulderInfluence: {
      envelope: influenceLeakage(envelope),
      anatomical: influenceLeakage(anatomical),
    },
    poses,
    limitations: [
      "Unrestricted compact bone envelopes and constrained envelopes use identical chart vertices and four-influence skinning; this is not a different tessellation comparison.",
      "A measurement-only clone realizes the shoulder chart as a closed surface; production correspondence-only charts do not render duplicate skin. Signed closed shoulder-chart volume is a limited geometric proxy. It does not measure full creature skin volume, appearance, or anatomical plausibility.",
      "No conclusion that a larger corrected volume is artistically better; art review remains pending.",
    ],
  };
}

export function compareCreatureGroom(character: CharacterDefinition) {
  const source = character.creature!;
  const started = performance.now(),
    product = compileCharacter(character, "review").creatureGroom!;
  const milliseconds = performance.now() - started;
  const empty = compileCharacter({ ...character, creature: { ...source, grooms: [] } }, "review")
    .creatureGroom!;
  const roots = new Map(product.guides.map((guide) => [guide.root.id, contentKey(guide.root)]));
  const ribbonStart = performance.now();
  const ribbons = source.grooms.length
    ? compileCharacter(
        {
          ...character,
          creature: {
            ...source,
            grooms: source.grooms.map((groom) => ({ ...groom, representation: "ribbons" as const })),
          },
        },
        "review",
      ).creatureGroom
    : undefined;
  const ribbonMilliseconds = performance.now() - ribbonStart;
  return {
    status: source.grooms.length ? "measured-two-opaque-realizations" : "not-applicable-no-authored-groom",
    representation: product.representation,
    diagnostics: product.diagnostics,
    timingScope: "Full character compilation including actual body projection; shared caches may be warm",
    compileMilliseconds: milliseconds,
    guideCount: product.guides.length,
    details: product.details.map((detail, index) => ({
      label: detail.label,
      cost: detail.cost,
      fidelity: detail.fidelity,
      nestedInPrior:
        index === 0 || detail.guideIds.every((id) => product.details[index - 1].guideIds.includes(id)),
      stableRootIdentities: detail.guideIds.every((id) => roots.has(id)),
      maxError: detail.maxError,
    })),
    noGroom: {
      guideCount: empty.guides.length,
      bytes: empty.details.reduce((sum, detail) => sum + detail.cost.bytes, 0),
      triangles: empty.details.reduce((sum, detail) => sum + detail.cost.triangles, 0),
    },
    alternate: ribbons
      ? {
          representation: ribbons.representation,
          compileMilliseconds: ribbonMilliseconds,
          sameRootCoordinates:
            ribbons.guides.length === product.guides.length &&
            ribbons.guides.every((guide) => roots.get(guide.root.id) === contentKey(guide.root)),
          sameCanonicalGuideCurves:
            contentKey(ribbons.guides.map((guide) => guide.points)) ===
            contentKey(product.guides.map((guide) => guide.points)),
          details: ribbons.details.map((detail) => ({
            label: detail.label,
            cost: detail.cost,
            fidelity: detail.fidelity,
            maxError: detail.maxError,
          })),
        }
      : null,
    limitations: [
      "Counts and rest-space guide bounds are CPU products, not measured GPU cost or visible equivalence.",
      "Closed opaque tufts and ribbons are measured alternatives. Alpha cards, strands, scattering/shadow equivalence and temporal transitions remain unverified.",
      "The no-groom control deliberately lacks fur and is not a matched-quality performance baseline.",
    ],
  };
}

export function compareCreatureSourceSolver(project: Project, character: CharacterDefinition) {
  const source = character.creature!,
    region = source.regions.find((region) => region.id === "shoulder")!,
    contact = source.contacts[0];
  const desiredWidth = region.extent[0] * 1.2,
    desiredContact = contact.target[2] + 0.08;
  const session = new AuthoringSession(project),
    started = performance.now();
  const preservedRegions = creatureExperimentPreservation(character);
  const solved = session.solveCreature({
    id: "width-and-contact-target",
    target: character.id,
    expectedRevision: 0,
    controls: [
      { kind: "regionScale", region: "shoulder", axis: 0, minimum: 0.8, maximum: 1.4 },
      {
        kind: "contactTarget",
        contact: contact.id,
        axis: 2,
        minimum: contact.target[2] - 0.2,
        maximum: contact.target[2] + 0.2,
      },
    ],
    objectives: [
      { kind: "regionExtent", region: "shoulder", axis: 0, value: desiredWidth, tolerance: 0.0001 },
      { kind: "contactTarget", contact: contact.id, axis: 2, value: desiredContact, tolerance: 0.0001 },
    ],
    preserve: preservedRegions,
    budget: { evaluations: 64, iterations: 12 },
  });
  const solveMilliseconds = performance.now() - started;
  const nextContact = structuredClone(contact);
  nextContact.target[2] = desiredContact;
  const directStart = performance.now();
  const direct = session.proposeCandidate({
    id: "direct-width-and-target",
    batch: {
      expectedRevision: 0,
      operations: [
        {
          kind: "creature.proportion",
          target: character.id,
          region: "shoulder",
          scale: [1.2, 1, 1],
          preserve: preservedRegions,
        },
        { kind: "creature.contact", target: character.id, value: nextContact },
      ],
    },
  });
  const directMilliseconds = performance.now() - directStart;
  const candidate = solved.candidate.changes.find((change) => change.id === character.id)!
    .after as CharacterDefinition;
  return {
    status: solved.status,
    preservedRegions,
    scope: solved.scope,
    solveMilliseconds,
    evaluations: solved.evaluations,
    iterations: solved.iterations,
    solverOperations: solved.batch.operations.length,
    directMilliseconds,
    directOperations: direct.batch.operations.length,
    residuals: solved.residuals,
    attainedWidth: candidate.creature!.regions.find((entry) => entry.id === "shoulder")!.extent[0],
    attainedContactTarget: candidate.creature!.contacts.find((entry) => entry.id === contact.id)!.target[2],
    acceptedSourceUnchanged: contentKey(session.inspect(character.id)) === contentKey(character),
    limitations: [
      "The two objectives are explicit source coordinates. Matching an authored contact target is not solving runtime foot slip.",
      "Direct parameter edits are the strong baseline for these simple objectives; solver time includes candidate preparation and is not a demonstrated speedup.",
    ],
  };
}

export function exerciseCreatureRecipes(project: Project, character: CharacterDefinition) {
  const recipe = project.recipes![0],
    recipeStart = performance.now();
  const variant = instantiateRecipe(recipe, {
    id: "reused-creature",
    name: "Recipe reuse study",
    recipe: recipe.id,
    version: recipe.version,
    parameters: { chestWidth: 0.5, jawDepth: 0.2 },
  }) as CharacterDefinition;
  const recipeMilliseconds = performance.now() - recipeStart;
  const session = new AuthoringSession(project);
  const wolf = createCreatureFixture("ash-warden").project.documents.find(
    (entry) => entry.id === "ash-warden",
  ) as CharacterDefinition;
  const groom = structuredClone(character.creature!.grooms[0] ?? wolf.creature!.grooms[0]);
  if (!character.creature!.grooms.length) {
    groom.id = "reed-collar-fibers";
    groom.material = "marsh-linen";
    groom.length = 0.035;
    groom.density = 40;
    groom.maxCards = 60;
  } else groom.length *= 1.15;
  const started = performance.now();
  const candidate = session.proposeCandidate({
    id: "performance-and-fiber-variant",
    batch: {
      expectedRevision: 0,
      operations: [
        {
          kind: "character.addKey",
          target: character.id,
          motion: "idle",
          joint: "head",
          time: 1,
          rotation: [-0.06, 0.1, 0.02],
          translation: [0, 0, 0],
        },
        { kind: "creature.groom", target: character.id, value: groom },
      ],
    },
  });
  const acceptedUnchangedBeforeAdoption = contentKey(session.inspect(character.id)) === contentKey(character);
  session.adoptCandidate(candidate.id, { transactionId: "adopt-performance-and-fiber" });
  const adopted = session.inspect(character.id) as CharacterDefinition;
  session.undo();
  return {
    status: "completed-source-reuse-and-roundtrip",
    recipe: recipe.id,
    recipeMilliseconds,
    recipeMatches: {
      fieldWidth: variant.field.nodes.find((node) => node.id === "ribcage")!.size[0] === 0.5,
      chartWidth:
        variant.creature!.charts[0].kind === "sweep" &&
        variant.creature!.charts[0].crossSections![1][0] === 0.5,
    },
    candidateOperations: candidate.batch.operations.length,
    acceptedUnchangedBeforeAdoption,
    motionChanged: contentKey(adopted.motions) !== contentKey(character.motions),
    groomChanged: contentKey(adopted.creature!.grooms) !== contentKey(character.creature!.grooms),
    undoRestoredExactly: contentKey(session.inspect(character.id)) === contentKey(character),
    elapsedMilliseconds: performance.now() - started,
    limitations: [
      "Recipe reuse and candidate transactions are executable source operations. No finished second-creature visual quality, human correction time, or authoring labor reduction is inferred.",
    ],
  };
}
