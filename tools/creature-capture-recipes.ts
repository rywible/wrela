import { type CreatureFixtureId, createCreatureFixture } from "@wrela/examples/creature-fixtures";
import type { CharacterDefinition } from "@wrela/model/documents";
import { contentKey, type Vec3 } from "@wrela/model/math";

/** Presentation-only form review. Both recipes retain exactly the same source,
 * cooked body geometry, displacement, correction, motion, and cloth state. */
export function createCreatureCaptureRecipe(id: CreatureFixtureId, mode: "clay" | "skeleton") {
  const fixture = createCreatureFixture(id);
  const character = fixture.project.documents.find((document) => document.id === id) as CharacterDefinition;
  const inspection = {
    channel: "clay" as const,
    hideGroom: true,
    overlays: mode === "skeleton" ? ["rig"] : [],
  };
  const captures = [
    { id: "front", position: [0, 1.6, 6] as Vec3 },
    { id: "side", position: [6, 1.6, 0] as Vec3 },
    { id: "back", position: [0, 1.6, -6] as Vec3 },
    { id: "three-quarter", position: [4.8, 2.6, 5.8] as Vec3 },
  ].map((camera) => ({
    camera: { ...camera, target: [0, 1.5, 0] as Vec3, fov: 40 },
    motion: "idle",
    time: 0,
    ...inspection,
    status: "pending",
  }));
  return {
    project: fixture.project,
    sourceRevision: contentKey(fixture.project),
    characterId: id,
    stageId: fixture.stageId,
    mode,
    inspection,
    sourceOperations: [],
    captures,
    poseCaptures: [
      { motion: "walk", time: character.motions.find((motion) => motion.id === "walk")!.duration * 0.25 },
      { motion: "lunge", time: 0.65 },
      { motion: "lunge", time: 1.05 },
      { motion: "hit", time: 0.12 },
    ],
    acceptance:
      mode === "clay"
        ? "Review silhouette, mass, anatomy, jaw and foot proportions before surface detail."
        : "Review actual posed joint placement and contact timing as a rig overlay on the identical clay body.",
    visualApproval: "not-reviewed",
  };
}
