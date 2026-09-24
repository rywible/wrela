import type { CharacterDefinition, CreatureDefinition, Vec3 } from "@wrela/model";

/** Uniform anatomical edits preserve dimensioned physical relationships. Gravity,
 * stiffness, mass, timing and material constants remain explicit art direction.
 * Collision primitives cannot represent anisotropic capsule/sphere scaling. */
export function scaleCreaturePhysicalDependents(
  character: CharacterDefinition & { creature: CreatureDefinition },
  regions: Set<string>,
  joints: Set<string>,
  scale: Vec3,
) {
  const source = character.creature;
  const uniform = Math.abs(scale[0] - scale[1]) < 1e-8 && Math.abs(scale[1] - scale[2]) < 1e-8;
  const bodies = source.articulation?.bodies.filter((body) => joints.has(body.joint)) ?? [];
  if (!uniform && bodies.length)
    throw Error("Nonuniform articulated-body scaling needs explicit collision-body dimensions");
  if (!uniform) return;
  const factor = scale[0];
  if (factor === 1) return;
  const vector = (value: Vec3) => value.map((component) => component * factor) as Vec3;
  for (const body of bodies) {
    body.radius *= factor;
    if (body.halfHeight !== undefined) body.halfHeight *= factor;
    if (body.offset) body.offset = vector(body.offset);
  }
  if (source.pelvis && joints.has(source.pelvis.joint))
    source.pelvis.maxOffset = vector(source.pelvis.maxOffset);
  const affectedAnchors = new Set(
    source.anchors.filter((anchor) => regions.has(anchor.region)).map((anchor) => anchor.id),
  );
  for (const anchor of source.anchors)
    if (affectedAnchors.has(anchor.id)) {
      anchor.offset *= factor;
      anchor.tolerance *= factor;
    }
  for (const attachment of source.attachments)
    if (affectedAnchors.has(attachment.anchor)) {
      attachment.offset = vector(attachment.offset);
      attachment.minimumClearance *= factor;
    }
  for (const cloth of source.cloth) if (regions.has(cloth.region)) cloth.collisionRadius *= factor;
  // World-space authored contact targets and their ground offsets are external
  // constraints. Only character-space contacts follow a proportion edit.
  for (const contact of source.contacts)
    if (joints.has(contact.joint) && contact.space === "character") {
      contact.tolerance *= factor;
      contact.offset *= factor;
    }
  for (const chain of source.ikChains)
    if (chain.joints.every((joint) => joints.has(joint))) chain.tolerance *= factor;
  for (const chain of source.secondaryChains)
    if (chain.joints.some((joint) => joints.has(joint))) {
      if (!chain.joints.every((joint) => joints.has(joint)))
        throw Error("Secondary chain spans unselected anatomy; expand the edit scope");
      chain.collisionRadius *= factor;
    }
}
