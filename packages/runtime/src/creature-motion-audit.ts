import { type CharacterDefinition, type CompiledCharacter, contentKey, type Vec3 } from "@wrela/model";

import { RuntimeSession } from "./session";

/** The real runtime (physical root motion, contacts and extraction), not an isolated pose solver. */
export async function auditCreatureMotion(
  character: CharacterDefinition,
  artifact: CompiledCharacter,
  motionId: string,
) {
  const motion = character.motions.find((m) => m.id === motionId);
  if (!motion || motion.duration > 10 || !character.creature)
    throw Error("Motion audit requires a creature clip of at most ten seconds");
  if (artifact.creatureSourceKey !== contentKey(character)) throw Error("Motion audit artifact is stale");
  const runtime = await RuntimeSession.create();
  const samples: {
    tick: number;
    time: number;
    root: Vec3;
    feet: { joint: string; position: Vec3 }[];
    maximumContactResidual: number;
    issues: string[];
  }[] = [];
  const plantedContacts: {
    id: string;
    joint: string;
    start: number;
    end: number;
    samples: number;
    maximumSlip: number;
  }[] = [];
  const transform = (m: ArrayLike<number>, p: Vec3): Vec3 =>
    [0, 1, 2].map((axis) => m[axis] * p[0] + m[4 + axis] * p[1] + m[8 + axis] * p[2] + m[12 + axis]) as Vec3;
  try {
    runtime.physics.addGround();
    runtime.setCreatureSecondaryGroundPlane({ height: 0, minX: -100, maxX: 100, minZ: -100, maxZ: 100 });
    runtime.addCharacter(character.id, artifact, character);
    runtime.playMotion(character.id, motionId, 0);
    runtime.advance(1 / 60); // Establish the requested clip at local time zero.
    for (let tick = 0; tick <= Math.ceil(motion.duration * 60); tick++) {
      if (tick) runtime.advance(1 / 60);
      const evaluated = runtime.evaluatedCharacters()[0];
      const diagnostics = runtime.creatureDiagnostics(character.id),
        contacts = diagnostics.filter((d) => d.kind === "contact");
      const feet = [...new Set(character.creature.contacts.map((c) => c.joint))].map((id) => {
        const index = artifact.joints.findIndex((j) => j.id === id),
          joint = artifact.joints[index];
        if (!joint) throw Error("Audit foot joint missing");
        return {
          joint: id,
          position: transform(
            evaluated.matrix,
            transform(evaluated.skinMatrices.subarray(index * 16, index * 16 + 16), joint.position),
          ),
        };
      });
      samples.push({
        tick,
        time: tick / 60,
        root: [evaluated.matrix[12], evaluated.matrix[13], evaluated.matrix[14]],
        feet,
        maximumContactResidual: Math.max(0, ...contacts.map((d) => d.residual)),
        issues: diagnostics
          .filter((d) => d.status === "unavailable" || d.status === "conflict")
          .map((d) => d.id),
      });
    }
    const worst = samples
      .slice()
      .sort((a, b) => b.maximumContactResidual - a.maximumContactResidual || a.tick - b.tick)
      .slice(0, 3);
    for (const contact of character.creature.contacts.filter((entry) => entry.motion === motionId)) {
      // Measure the world-space foot path only while the authored contact is fully weighted.
      // A blend edge is allowed to move as the foot is lifted or lowered.
      const start = contact.start + contact.blendIn + 1 / 60;
      const end = contact.end - contact.blendOut - 1 / 60;
      const positions = samples
        .filter((sample) => sample.time >= start && sample.time <= end)
        .map((sample) => sample.feet.find((foot) => foot.joint === contact.joint)?.position)
        .filter((position): position is Vec3 => position !== undefined);
      if (!positions.length) continue;
      const planted = positions[0];
      plantedContacts.push({
        id: contact.id,
        joint: contact.joint,
        start: contact.start,
        end: contact.end,
        samples: positions.length,
        maximumSlip: Math.max(
          ...positions.map((position) => Math.hypot(position[0] - planted[0], position[2] - planted[2])),
        ),
      });
    }
    return {
      sourceKey: contentKey(character),
      artifactKey: artifact.key,
      motion: motionId,
      sampleRate: 60,
      samples,
      worst,
      maximumContactResidual: Math.max(...samples.map((s) => s.maximumContactResidual)),
      plantedContacts,
      maximumPlantedSlip: Math.max(0, ...plantedContacts.map((contact) => contact.maximumSlip)),
      scope:
        "Actual flat-ground runtime contact/foot trajectories, including world-space planted slip. Skin collision and artistic motion quality still require visual review.",
    };
  } finally {
    runtime.dispose();
  }
}
