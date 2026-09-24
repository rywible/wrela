import {
  type CharacterDefinition,
  type CreatureDefinition,
  type CreatureRegion,
  canonical,
  creatureSchema,
  type Document,
} from "@wrela/model";

import type { CreatureOperation } from "./creature-commands";
import { updateCreatureLandmarkBindings } from "./creature-landmark-bindings";
import { scaleCreaturePhysicalDependents } from "./creature-physical-coherence";
export type CreaturePoint = [number, number, number];
const clone = <T>(value: T): T => structuredClone(value);
const add = (a: CreaturePoint, b: CreaturePoint): CreaturePoint => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a: CreaturePoint, b: CreaturePoint): CreaturePoint => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a: CreaturePoint, b: CreaturePoint): CreaturePoint => [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
/** Euler X then Y then Z, matching source field frames. */
function rotate(point: CreaturePoint, rotation: CreaturePoint, inverse = false): CreaturePoint {
  let [x, y, z] = point;
  for (const axis of inverse ? [2, 1, 0] : [0, 1, 2]) {
    const angle = rotation[axis] * (inverse ? -1 : 1),
      c = Math.cos(angle),
      s = Math.sin(angle);
    if (axis === 0) [y, z] = [y * c - z * s, y * s + z * c];
    if (axis === 1) [x, z] = [x * c + z * s, -x * s + z * c];
    if (axis === 2) [x, y] = [x * c - y * s, x * s + y * c];
  }
  return [x, y, z];
}
/** A diagonal scale in the edit frame is representable with unchanged local
 * orientation only when Rlocal^-1 Redit S Redit^-1 Rlocal is diagonal.
 * Equal-eigenvalue subspaces and axis permutations pass; actual shear does not. */
function representedLocalScale(
  scale: CreaturePoint,
  editRotation: CreaturePoint,
  localRotation: CreaturePoint,
): CreaturePoint {
  const tolerance = 64 * Number.EPSILON * Math.max(1, ...scale);
  const result: CreaturePoint = [1, 1, 1];
  for (let axis = 0; axis < 3; axis++) {
    const basis: CreaturePoint = [0, 0, 0];
    basis[axis] = 1;
    const world = rotate(basis, localRotation);
    const transformed = rotate(mul(rotate(world, editRotation, true), scale), editRotation);
    const column = rotate(transformed, localRotation, true);
    if (column.some((value, row) => row !== axis && Math.abs(value) > tolerance))
      throw Error(
        "Nonuniform source scaling would introduce an unrepresentable shear; edit orientation or use an explicit chart",
      );
    result[axis] = scale.find((value) => Math.abs(value - column[axis]) <= tolerance) ?? column[axis];
  }
  return result;
}
export function requireCreature(
  document: Document | undefined,
): CharacterDefinition & { creature: CreatureDefinition } {
  if (!document || document.kind !== "character" || !document.creature)
    throw Error("Select a character with initialized creature source");
  return document as CharacterDefinition & { creature: CreatureDefinition };
}
export function inspectCreatureSource(document: Document | undefined, regionId?: string) {
  const character = requireCreature(document),
    source = character.creature;
  const region = regionId === undefined ? undefined : source.regions.find((r) => r.id === regionId);
  if (regionId !== undefined && !region) throw Error("Anatomical region does not exist");
  const selected = region ? new Set([region.id]) : new Set(source.regions.map((r) => r.id));
  const joints = new Set(source.regions.filter((r) => selected.has(r.id)).flatMap((r) => r.jointIds));
  return clone({
    target: character.id,
    schemaVersion: source.schemaVersion,
    articulation: source.articulation,
    pelvis: source.pelvis,
    regions: source.regions.filter((r) => selected.has(r.id)),
    attachments: source.attachments.filter((attachment) =>
      source.anchors.some((anchor) => anchor.id === attachment.anchor && selected.has(anchor.region)),
    ),
    cloth: source.cloth.filter((item) => selected.has(item.region)),
    landmarks: source.landmarks.filter((v) => selected.has(v.region)),
    charts: source.charts.filter((v) => selected.has(v.region)),
    anchors: source.anchors.filter((v) => selected.has(v.region)),
    sculpts: source.sculpts.filter((v) => selected.has(v.region)),
    appearance: source.appearance.filter((v) => selected.has(v.region)),
    grooms: source.grooms.filter((v) => selected.has(v.region)),
    influenceRules: source.influenceRules.filter((v) => selected.has(v.region)),
    correctives: source.correctives.filter((v) => selected.has(v.region)),
    contacts: source.contacts.filter((v) => !region || joints.has(v.joint)),
    ikChains: source.ikChains.filter((v) => !region || v.joints.some((id) => joints.has(id))),
    secondaryChains: source.secondaryChains.filter((v) => !region || v.joints.some((id) => joints.has(id))),
    expressions: source.expressions.filter((v) => !region || v.weights.some((w) => joints.has(w.joint))),
    reviewScenarios: source.reviewScenarios,
    joints: character.joints.filter((v) => joints.has(v.id)),
    nodes: character.field.nodes.filter((v) =>
      source.regions.some((r) => selected.has(r.id) && r.nodeIds.includes(v.id)),
    ),
  });
}
export function selectCreatureRegions(document: Document | undefined, point: CreaturePoint, radius = 0) {
  if (point.length !== 3 || !point.every(Number.isFinite) || !Number.isFinite(radius) || radius < 0)
    throw Error("Selection requires finite character-space coordinates and nonnegative radius");
  const character = requireCreature(document);
  return character.creature.regions
    .map((region) => {
      const local = rotate(sub(point, region.frame.position), region.frame.rotation, true);
      const distance = Math.hypot(...local.map((v, i) => Math.max(0, Math.abs(v) - region.extent[i])));
      return {
        region: region.id,
        distance,
        local,
        method: "region-support-box" as const,
        exactSurface: false,
      };
    })
    .filter((result) => result.distance <= radius)
    .sort((a, b) => a.distance - b.distance || a.region.localeCompare(b.region));
}
export function explainCreatureSource(document: Document | undefined, region: string) {
  const selected = inspectCreatureSource(document, region);
  return {
    target: selected.target,
    region,
    basis: "declared-source-dependencies",
    measuredSensitivity: false,
    geometry: [
      ...selected.nodes.map((v) => v.id),
      ...selected.charts.map((v) => v.id),
      ...selected.sculpts.map((v) => v.id),
    ],
    appearance: selected.appearance.map((v) => ({ id: v.id, material: v.material, family: v.family })),
    correspondence: selected.anchors.map((v) => ({
      id: v.id,
      chart: v.chart,
      chartRevision: v.chartRevision,
      landmark: v.landmark,
      purpose: v.purpose,
    })),
    attachments: selected.attachments,
    pelvis:
      selected.pelvis && selected.joints.some((joint) => joint.id === selected.pelvis?.joint)
        ? selected.pelvis
        : undefined,
    articulation: selected.articulation
      ? {
          bodies: selected.articulation.bodies.filter((body) =>
            selected.joints.some((joint) => joint.id === body.joint),
          ),
          joints: selected.articulation.joints.filter((constraint) =>
            selected.joints.some((joint) => joint.id === constraint.parent || joint.id === constraint.child),
          ),
        }
      : undefined,
    cloth: selected.cloth.map((v) => ({
      id: v.id,
      chart: v.chart,
      chartRevision: v.chartRevision,
      fittingLandmarks: v.fittingLandmarks,
    })),
    groom: selected.grooms.map((v) => ({ id: v.id, chart: v.chart, maxCards: v.maxCards })),
    deformation: {
      joints: selected.joints.map((v) => v.id),
      rules: selected.influenceRules.map((v) => v.id),
      correctives: selected.correctives.map((v) => v.id),
    },
    performance: {
      contacts: selected.contacts.map((v) => v.id),
      ik: selected.ikChains.map((v) => v.id),
      secondary: selected.secondaryChains.map((v) => v.id),
    },
    limitations: [
      "Dependencies identify editable controls; they do not establish pixel contribution, solver sensitivity, or measured rendering cost",
    ],
  };
}
const domains = {
  "creature.attachment": "attachments",
  "creature.cloth": "cloth",
  "creature.region": "regions",
  "creature.landmark": "landmarks",
  "creature.chart": "charts",
  "creature.anchor": "anchors",
  "creature.sculpt": "sculpts",
  "creature.appearance": "appearance",
  "creature.groom": "grooms",
  "creature.influence": "influenceRules",
  "creature.corrective": "correctives",
  "creature.contact": "contacts",
  "creature.ik": "ikChains",
  "creature.secondary": "secondaryChains",
  "creature.expression": "expressions",
  "creature.review": "reviewScenarios",
} as const;
export function applyCreatureOperation(document: Document, operation: CreatureOperation) {
  if (document.kind !== "character") throw Error("Select a character");
  if (operation.kind === "creature.initialize") {
    if (document.creature) throw Error("Creature source already exists; edit its domains explicitly");
    document.creature = creatureSchema.parse(operation.source);
    return;
  }
  const character = requireCreature(document),
    source = character.creature;
  if (operation.kind === "creature.pelvis") {
    if (operation.value === null) delete source.pelvis;
    else source.pelvis = clone(operation.value);
    return;
  }
  if (operation.kind === "creature.articulation") {
    if (operation.value === null) delete source.articulation;
    else source.articulation = clone(operation.value);
    return;
  }
  if (operation.kind === "creature.remove") {
    const values = source[operation.domain];
    const index = values.findIndex((value) => value.id === operation.id);
    if (index < 0) throw Error("Creature source identity does not exist");
    values.splice(index, 1);
    return;
  }
  if (operation.kind === "creature.proportion" || operation.kind === "creature.move") {
    transformRegion(character, operation);
    return;
  }
  const domain = domains[operation.kind];
  // The discriminated operation schema enforces the domain/value relationship.
  const values = source[domain] as { id: string }[];
  const index = values.findIndex((value) => value.id === operation.value.id);
  if (operation.kind === "creature.chart" && index >= 0) {
    const previous = source.charts[index],
      next = operation.value;
    const topologyChanged =
      previous.kind !== next.kind ||
      previous.region !== next.region ||
      previous.points.length !== next.points.length;
    if (next.revision < previous.revision) throw Error("Chart revisions cannot go backwards");
    const geometryChanged =
      canonical({ ...previous, material: undefined, revision: 0 }) !==
      canonical({ ...next, material: undefined, revision: 0 });
    if (operation.correspondence === "preserve") {
      const reordered = next.points.some((point, index) =>
        previous.points.some((old, oldIndex) => oldIndex !== index && canonical(old) === canonical(point)),
      );
      const dot = (a: CreaturePoint, b: CreaturePoint) => a.reduce((sum, v, i) => sum + v * b[i], 0);
      const cross = (a: CreaturePoint, b: CreaturePoint): CreaturePoint => [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
      ];
      const direction = (chart: typeof next) =>
        chart.kind === "sweep"
          ? sub(chart.points[chart.points.length - 1], chart.points[0])
          : cross(sub(chart.points[1], chart.points[0]), sub(chart.points[2], chart.points[0]));
      if (topologyChanged || reordered || dot(direction(previous), direction(next)) <= 0)
        throw Error("Chart order or orientation changed; explicit correspondence repair is required");
      if (next.revision !== previous.revision)
        throw Error("Preserved chart correspondence retains its revision");
    } else if (
      (geometryChanged || operation.correspondence === "repair") &&
      next.revision <= previous.revision
    )
      throw Error(
        "Chart geometry changes require correspondence: preserve or an increased revision and explicit anchor/groom repair",
      );
  }
  const previousLandmark = operation.kind === "creature.landmark" ? source.landmarks[index] : undefined;
  if (index < 0) values.push(clone(operation.value));
  else values[index] = clone(operation.value);
  if (operation.kind === "creature.landmark")
    updateCreatureLandmarkBindings(source, operation.value, previousLandmark);
}

function transformRegion(
  character: CharacterDefinition & { creature: CreatureDefinition },
  operation: Extract<CreatureOperation, { kind: "creature.proportion" | "creature.move" }>,
) {
  const source = character.creature,
    region = source.regions.find((r) => r.id === operation.region);
  if (!region) throw Error("Anatomical region does not exist");
  const affected = new Set([region.id]);
  if (operation.propagate === "descendants") {
    for (let pass = 0; pass < source.regions.length; pass++)
      for (const candidate of source.regions)
        if (candidate.parent && affected.has(candidate.parent)) affected.add(candidate.id);
  }
  for (const id of operation.preserve ?? []) {
    if (!source.regions.some((r) => r.id === id)) throw Error(`Preserved region ${id} does not exist`);
    if (affected.has(id)) throw Error(`Hard preservation constraint conflicts with edit of ${id}`);
  }
  const protectedSource = (operation.preserve ?? []).map((id) =>
    canonical(inspectCreatureSource(character, id)),
  );
  const selected = source.regions.filter((r) => affected.has(r.id));
  const attachedNodes = new Set(source.attachments.flatMap((a) => a.nodeIds));
  const selectedNodes = new Set(selected.flatMap((r) => r.nodeIds).filter((id) => !attachedNodes.has(id)));
  const selectedJoints = new Set(selected.flatMap((r) => r.jointIds));
  for (const other of source.regions.filter((r) => !affected.has(r.id))) {
    if (
      other.nodeIds.some((id) => selectedNodes.has(id)) ||
      other.jointIds.some((id) => selectedJoints.has(id))
    )
      throw Error(
        `Shared controls overlap unselected region ${other.id}; select their common region scope explicitly`,
      );
  }
  const scale: CreaturePoint = operation.kind === "creature.proportion" ? operation.scale : [1, 1, 1];
  const translation: CreaturePoint = operation.kind === "creature.move" ? operation.translation : [0, 0, 0];
  if (scale.every((value) => value === 1) && translation.every((value) => value === 0)) return;
  const origin = clone(region.frame.position);
  const scaleDelta = scale.map((value) => value - 1) as CreaturePoint;
  // Apply only the change rather than reconstructing origin + local point. An
  // unaffected axis then remains bit-identical instead of acquiring roundoff.
  const transform = (point: CreaturePoint) =>
    add(
      add(
        point,
        rotate(
          mul(rotate(sub(point, origin), region.frame.rotation, true), scaleDelta),
          region.frame.rotation,
        ),
      ),
      translation,
    );
  const vectorTransform = (vector: CreaturePoint) =>
    add(vector, rotate(mul(rotate(vector, region.frame.rotation, true), scaleDelta), region.frame.rotation));
  const changesPoint = (point: CreaturePoint) =>
    transform(point).some((value, axis) => value !== point[axis]);
  const changesVector = (vector: CreaturePoint) =>
    vectorTransform(vector).some((value, axis) => value !== vector[axis]);
  // A width edit can select axial spine/neck joints without moving them. Only
  // changed rest or control transforms can drive a protected descendant pose.
  const movedJoints = new Set(
    character.joints
      .filter((joint) => selectedJoints.has(joint.id) && changesPoint(joint.position))
      .map((joint) => joint.id),
  );
  for (const motion of character.motions)
    for (const key of motion.keys)
      if (selectedJoints.has(key.joint) && changesVector(key.translation)) movedJoints.add(key.joint);
  for (const expression of source.expressions)
    for (const weight of expression.weights)
      if (selectedJoints.has(weight.joint) && changesVector(weight.translation))
        movedJoints.add(weight.joint);
  for (const clip of character.performance?.clips ?? []) {
    for (const key of clip.facialKeys)
      if (selectedJoints.has(key.joint) && changesVector(key.translation)) movedJoints.add(key.joint);
    // Alignments apply a root correction, so target edits also affect sibling anatomy.
    if (
      clip.alignments.some(
        (alignment) => selectedJoints.has(alignment.joint) && changesPoint(alignment.target),
      )
    )
      for (const joint of character.joints) if (!joint.parent) movedJoints.add(joint.id);
  }
  for (const contact of source.contacts)
    if (selectedJoints.has(contact.joint) && contact.space === "character" && changesPoint(contact.target))
      movedJoints.add(contact.joint);
  for (const chain of source.ikChains)
    if (
      chain.joints.some((id) => selectedJoints.has(id)) &&
      (changesPoint(chain.target) || changesPoint(chain.pole))
    )
      for (const id of chain.joints) movedJoints.add(id);
  const dependsOnMovedJoint = (id: string): boolean => {
    const visited = new Set<string>();
    let current: string | null = id;
    while (current && !visited.has(current)) {
      if (movedJoints.has(current)) return true;
      visited.add(current);
      current = character.joints.find((joint) => joint.id === current)?.parent ?? null;
    }
    return false;
  };
  const protectedRegions = new Set(operation.preserve ?? []);
  const movedAnchors = new Set(
    source.anchors.filter((anchor) => affected.has(anchor.region)).map((anchor) => anchor.id),
  );
  const protectedNodes = new Set(
    source.regions
      .filter((candidate) => protectedRegions.has(candidate.id))
      .flatMap((candidate) => candidate.nodeIds),
  );
  for (const cloth of source.cloth)
    if (protectedRegions.has(cloth.region) && cloth.pins.some((pin) => movedAnchors.has(pin.anchor)))
      throw Error(`Hard preservation constraint protects cloth ${cloth.id} pinned to edited anatomy`);
  for (const attachment of source.attachments)
    if (
      attachment.nodeIds.some((id) => protectedNodes.has(id)) &&
      (movedAnchors.has(attachment.anchor) ||
        (attachment.rigidJoint && dependsOnMovedJoint(attachment.rigidJoint)))
    )
      throw Error(
        `Hard preservation constraint protects mounted component ${attachment.id} driven by edited anatomy`,
      );
  for (const appearance of source.appearance)
    if (protectedRegions.has(appearance.region) && appearance.anchor && movedAnchors.has(appearance.anchor))
      throw Error(
        `Hard preservation constraint protects appearance ${appearance.id} anchored to edited anatomy`,
      );
  for (const candidate of source.regions.filter((item) => protectedRegions.has(item.id)))
    if (candidate.jointIds.some(dependsOnMovedJoint))
      throw Error(`Hard preservation constraint protects rig dependency of ${candidate.id}`);
  for (const candidate of source.regions.filter((item) => protectedRegions.has(item.id))) {
    const rules = source.influenceRules.filter((rule) => rule.region === candidate.id);
    const rigid = rules.find((rule) => rule.rigidJoint)?.rigidJoint;
    const allowed = rules.flatMap((rule) => rule.allowedJoints);
    const excluded = new Set(rules.flatMap((rule) => rule.excludedJoints));
    const candidates = (
      rigid
        ? [rigid]
        : allowed.length
          ? allowed
          : candidate.jointIds.length
            ? candidate.jointIds
            : character.joints.map((joint) => joint.id)
    ).filter((id) => !excluded.has(id));
    if (candidates.some(dependsOnMovedJoint))
      throw Error(`Hard preservation constraint protects binding of ${candidate.id} driven by edited joints`);
    // Joint envelopes influence relative weights only when more than one
    // influence is eligible. Rigid/one-joint regions retain weight one.
    if (
      !rigid &&
      candidates.length > 1 &&
      Math.max(...scale) !== 1 &&
      candidates.some((id) => {
        const joint = character.joints.find((value) => value.id === id);
        return selectedJoints.has(id) || (joint?.parent && selectedJoints.has(joint.parent));
      })
    )
      throw Error(
        `Hard preservation constraint protects binding weights of ${candidate.id} affected by edited envelopes`,
      );
  }
  for (const corrective of source.correctives)
    if (protectedRegions.has(corrective.region) && dependsOnMovedJoint(corrective.joint))
      throw Error(
        `Hard preservation constraint protects corrective ${corrective.id} driven by edited joints`,
      );
  for (const node of character.field.nodes.filter((n) => selectedNodes.has(n.id)))
    if (node.children.length)
      throw Error(
        "Proportion edits require leaf shape controls; composition transforms need explicit local edits",
      );
  const uniform = Math.abs(scale[0] - scale[1]) < 1e-8 && Math.abs(scale[1] - scale[2]) < 1e-8;
  scaleCreaturePhysicalDependents(character, affected, selectedJoints, scale);
  for (const node of character.field.nodes.filter((n) => selectedNodes.has(n.id))) {
    const localScale = representedLocalScale(scale, region.frame.rotation, node.rotation);
    node.position = transform(node.position);
    node.size = mul(node.size, localScale);
    if (!uniform && ["sphere", "capsule", "torus"].includes(node.kind))
      throw Error("Nonuniform radial primitive scaling needs an explicit ellipsoid or sweep conversion");
    node.radius *= localScale[0];
    node.blend *= Math.max(...localScale);
  }
  for (const node of character.field.nodes)
    if (node.children.some((id) => selectedNodes.has(id)) && !selectedNodes.has(node.id)) {
      if (
        node.position.some((v) => Math.abs(v) > 1e-8) ||
        node.rotation.some((v) => Math.abs(v) > 1e-8) ||
        node.size.some((v) => Math.abs(v - 1) > 1e-8)
      )
        throw Error("Selected legacy nodes have transformed parents; edit their local frames explicitly");
    }
  for (const motion of character.motions)
    for (const key of motion.keys)
      if (selectedJoints.has(key.joint)) key.translation = vectorTransform(key.translation);
  for (const expression of source.expressions)
    for (const weight of expression.weights)
      if (selectedJoints.has(weight.joint)) weight.translation = vectorTransform(weight.translation);
  for (const clip of character.performance?.clips ?? []) {
    for (const key of clip.facialKeys)
      if (selectedJoints.has(key.joint)) key.translation = vectorTransform(key.translation);
    for (const alignment of clip.alignments)
      if (selectedJoints.has(alignment.joint)) alignment.target = transform(alignment.target);
    for (const contact of clip.contacts)
      if (selectedJoints.has(contact.joint)) {
        if (Math.abs(vectorTransform([1, 0, 0])[1]) > 1e-8 || Math.abs(vectorTransform([0, 0, 1])[1]) > 1e-8)
          throw Error("Anatomical edit tilts a performance ground plane; author an explicit contact target");
        contact.groundHeight = transform([0, contact.groundHeight, 0])[1];
        contact.tolerance *= Math.abs(vectorTransform([0, 1, 0])[1]);
      }
  }
  for (const joint of character.joints.filter((j) => selectedJoints.has(j.id))) {
    joint.position = transform(joint.position);
    joint.radius *= Math.max(...scale);
  }
  for (const selectedRegion of selected) {
    const localScale = representedLocalScale(scale, region.frame.rotation, selectedRegion.frame.rotation);
    selectedRegion.frame.position = transform(selectedRegion.frame.position);
    selectedRegion.extent = mul(selectedRegion.extent, localScale);
    scaleRegionFields(source, selectedRegion, localScale);
  }
  for (const contact of source.contacts.filter((v) => selectedJoints.has(v.joint) && v.space === "character"))
    contact.target = transform(contact.target);
  for (const chain of source.ikChains.filter((v) => v.joints.some((id) => selectedJoints.has(id)))) {
    if (chain.joints.some((id) => !selectedJoints.has(id)))
      throw Error("IK chain spans unselected anatomy; expand the edit scope");
    chain.target = transform(chain.target);
    chain.pole = transform(chain.pole);
  }
  // Include rotated primitive support, rather than treating its largest local
  // half extent as a world-space box (which clips diagonally oriented boxes).
  for (const node of character.field.nodes.filter((n) => selectedNodes.has(n.id))) {
    const extent: CreaturePoint =
      node.kind === "sphere"
        ? [node.radius, node.radius, node.radius]
        : node.kind === "capsule"
          ? [node.radius, node.size[1] + node.radius, node.radius]
          : node.kind === "torus"
            ? [node.size[0] + node.radius, node.radius, node.size[0] + node.radius]
            : node.size;
    for (let corner = 0; corner < 8; corner++) {
      const local = extent.map((value, axis) => (corner & (1 << axis) ? value : -value)) as CreaturePoint;
      const point = add(node.position, rotate(local, node.rotation));
      for (let axis = 0; axis < 3; axis++) {
        character.field.bounds.min[axis] = Math.min(
          character.field.bounds.min[axis],
          point[axis] - node.blend,
        );
        character.field.bounds.max[axis] = Math.max(
          character.field.bounds.max[axis],
          point[axis] + node.blend,
        );
      }
    }
  }
  for (const [i, id] of (operation.preserve ?? []).entries())
    if (canonical(inspectCreatureSource(character, id)) !== protectedSource[i])
      throw Error(`Hard preservation constraint was violated for ${id}`);
}

function scaleRegionFields(source: CreatureDefinition, region: CreatureRegion, scale: CreaturePoint) {
  const maximum = Math.max(...scale),
    uniform = Math.abs(scale[0] - scale[1]) < 1e-8 && Math.abs(scale[1] - scale[2]) < 1e-8;
  for (const chart of source.charts.filter((v) => v.region === region.id)) {
    if (chart.kind === "sweep") {
      if (!uniform) {
        const delta = sub(chart.points[1], chart.points[0]);
        const axis = delta.map(Math.abs).indexOf(Math.max(...delta.map(Math.abs)));
        if (
          chart.points.some((p) =>
            p.some((value, i) => i !== axis && Math.abs(value - chart.points[0][i]) > 1e-8),
          )
        )
          throw Error(
            "Nonuniform curved sweep scaling would shear transported sections; use an explicit chart edit",
          );
        const transverse = axis === 0 ? [2, 1] : axis === 1 ? [0, 2] : [0, 1];
        chart.crossSections = chart.radii.map((radius, i) => {
          const [a, b] = chart.crossSections?.[i] ?? [radius, radius];
          return [a * scale[transverse[0]], b * scale[transverse[1]]];
        });
      } else if (chart.crossSections)
        chart.crossSections = chart.crossSections.map(([a, b]) => [a * scale[0], b * scale[0]]);
      chart.points = chart.points.map((p) => mul(p, scale));
      chart.radii = chart.radii.map((r) => r * maximum);
    } else {
      chart.points = chart.points.map((p) => mul(p, scale)) as typeof chart.points;
      if (chart.controlOffsets)
        chart.controlOffsets = chart.controlOffsets.map((row) => row.map((offset) => mul(offset, scale)));
      chart.thickness *= maximum;
    }
  }
  for (const landmark of source.landmarks.filter((v) => v.region === region.id))
    landmark.position = mul(landmark.position, scale);
  for (const anchor of source.anchors.filter((v) => v.region === region.id))
    if (!anchor.chart) anchor.coordinates = mul(anchor.coordinates, scale);
  for (const sculpt of source.sculpts.filter((v) => v.region === region.id)) {
    if (sculpt.support) {
      const localScale = representedLocalScale(scale, [0, 0, 0], sculpt.support.rotation);
      sculpt.support.radii = mul(sculpt.support.radii, localScale);
    }
    if (sculpt.path) sculpt.path = sculpt.path.map((p) => mul(p, scale));
    if (sculpt.detail && uniform) sculpt.detail.maxEdgeLength *= maximum;
    sculpt.center = mul(sculpt.center, scale);
    if (uniform) sculpt.radius *= maximum;
    sculpt.displacement = mul(sculpt.displacement, scale);
  }
  for (const appearance of source.appearance.filter((v) => v.region === region.id))
    if (appearance.mask && !appearance.anchor) {
      appearance.mask.center = mul(appearance.mask.center, scale);
      if (uniform) appearance.mask.radius *= maximum;
    }
  for (const groom of source.grooms.filter((v) => v.region === region.id)) {
    for (const guide of groom.guides) guide.points = guide.points.map((p) => mul(p, scale));
    for (const mask of groom.masks) {
      mask.center = mul(mask.center, scale);
      if (uniform) mask.radius *= maximum;
    }
  }
  for (const corrective of source.correctives.filter((v) => v.region === region.id)) {
    corrective.center = mul(corrective.center, scale);
    if (uniform) corrective.radius *= maximum;
    corrective.displacement = mul(corrective.displacement, scale);
  }
}
