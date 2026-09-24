import type { Diagnostic } from "./contracts";
import { creaturePatchHasConsistentOrientation } from "./creature-fitting";
import type { CharacterDefinition } from "./documents";

/** Cross-source checks deliberately run after schema parsing, including incremental edits. */
export function validateCreature(character: CharacterDefinition): Diagnostic[] {
  const source = character.creature;
  if (!source) return [];
  const diagnostics: Diagnostic[] = [];
  const error = (node: string, message: string) =>
    diagnostics.push({
      severity: "error",
      code: "creature-reference",
      document: character.id,
      node,
      message,
    } as Diagnostic);
  const unique = (items: { id: string }[], label: string) => {
    const seen = new Set<string>();
    for (const item of items) {
      if (seen.has(item.id)) error(item.id, `${label} IDs must be unique: ${item.id}`);
      seen.add(item.id);
    }
  };
  for (const [label, items] of Object.entries(source)) if (Array.isArray(items)) unique(items, label);
  const regions = new Map(source.regions.map((x) => [x.id, x]));
  const charts = new Map(source.charts.map((x) => [x.id, x]));
  const landmarks = new Map(source.landmarks.map((x) => [x.id, x]));
  const joints = new Map(character.joints.map((x) => [x.id, x]));
  const nodes = new Set(character.field.nodes.map((x) => x.id));
  const motions = new Map(character.motions.map((x) => [x.id, x]));
  for (const sculpt of source.sculpts)
    for (const id of sculpt.nodeIds ?? []) {
      const region = regions.get(sculpt.region);
      if (!region?.nodeIds.includes(id) && charts.get(id)?.region !== sculpt.region)
        error(sculpt.id, "Sculpt source scope must belong to its anatomical region");
    }
  const joint = (owner: string, id: string) => {
    if (!joints.has(id)) error(owner, `Missing creature joint ${id}`);
  };
  for (const region of source.regions) {
    for (const id of region.nodeIds) if (!nodes.has(id)) error(region.id, `Missing region field node ${id}`);
    for (const id of region.jointIds) joint(region.id, id);
    for (const [label, id] of [
      ["parent", region.parent],
      ["mirror", region.mirror],
    ]) {
      if (id && !regions.has(id)) error(region.id, `Missing region ${label} ${id}`);
      if (id === region.id) error(region.id, `Region cannot be its own ${label}`);
    }
    if (region.mirror) {
      const partner = regions.get(region.mirror);
      if (partner?.mirror !== region.id) error(region.id, "Region mirror must be reciprocal");
    }
    const seen = new Set<string>([region.id]);
    let parent = region.parent;
    while (parent && regions.has(parent)) {
      if (seen.has(parent)) {
        error(region.id, "Region parent graph contains a cycle");
        break;
      }
      seen.add(parent);
      parent = regions.get(parent)?.parent;
    }
  }
  const regional = [
    ...source.landmarks,
    ...source.charts,
    ...source.anchors,
    ...source.sculpts,
    ...source.appearance,
    ...source.grooms,
    ...source.influenceRules,
    ...source.correctives,
    ...source.cloth,
  ];
  for (const item of regional)
    if (!regions.has(item.region)) error(item.id, `Missing creature region ${item.region}`);
  for (const chart of source.charts) {
    if (chart.kind === "sweep") {
      if (
        chart.radii.length !== chart.points.length ||
        (chart.crossSections && chart.crossSections.length !== chart.points.length) ||
        (chart.twists && chart.twists.length !== chart.points.length)
      )
        error(chart.id, "Sweep section arrays must match its points");
      for (let i = 1; i < chart.points.length; i++)
        if (Math.hypot(...chart.points[i].map((v, a) => v - chart.points[i - 1][a])) < 1e-8)
          error(chart.id, "Sweep consecutive points must be distinct");
    } else {
      const [a, b, c, d] = chart.points;
      const area = (p: number[], q: number[], r: number[]) => {
        const u = q.map((v, i) => v - p[i]),
          v = r.map((n, i) => n - p[i]);
        return Math.hypot(u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]);
      };
      if (area(a, b, c) < 1e-10 || area(b, d, c) < 1e-10)
        error(chart.id, "Patch triangles must have nonzero area");
    }
  }
  for (const item of [...source.anchors, ...source.grooms]) {
    if (!item.chart) continue;
    const chart = charts.get(item.chart);
    if (!chart) error(item.id, `Missing creature chart ${item.chart}`);
    else if (chart.region !== item.region) error(item.id, "Chart belongs to a different region");
  }
  for (const anchor of source.anchors) {
    if (anchor.landmark) {
      const landmark = landmarks.get(anchor.landmark);
      if (!landmark) error(anchor.id, `Missing bound landmark ${anchor.landmark}`);
      else if (landmark.region !== anchor.region)
        error(anchor.id, "Bound landmark must belong to the anchor's anatomical region");
      else if (anchor.coordinates.some((value, axis) => Math.abs(value - landmark.position[axis]) > 1e-9))
        error(
          anchor.id,
          "Landmark anchor is stale; move its landmark with creature.landmark or detach its binding",
        );
      if (anchor.chart)
        error(anchor.id, "A landmark anchor uses region-local coordinates and cannot also bind to a chart");
    }
    const chart = anchor.chart ? charts.get(anchor.chart) : undefined;
    if (anchor.chart && anchor.chartRevision === undefined)
      error(anchor.id, "Chart anchor must declare chartRevision");
    if (!anchor.chart && anchor.chartRevision !== undefined)
      error(anchor.id, "chartRevision requires a chart");
    if (chart && anchor.chartRevision !== chart.revision)
      error(anchor.id, "Anchor chart revision is stale; explicit correspondence repair required");
    if (
      chart &&
      (anchor.coordinates[0] < 0 ||
        anchor.coordinates[0] > 1 ||
        anchor.coordinates[1] < 0 ||
        anchor.coordinates[1] > 1)
    )
      error(anchor.id, "Chart coordinates u and v must be in [0,1]");
  }
  for (const groom of source.grooms) {
    unique(groom.guides, "Groom guide");
    if (groom.rootProjection) {
      const scope = new Set<string>([groom.region]);
      for (let pass = 0; pass < source.regions.length; pass++)
        for (const region of source.regions)
          if (region.parent && scope.has(region.parent)) scope.add(region.id);
      const allowed = new Set(
        source.regions.filter((region) => scope.has(region.id)).flatMap((region) => region.nodeIds),
      );
      const queue = [...allowed];
      for (let index = 0; index < queue.length; index++)
        for (const child of character.field.nodes.find((node) => node.id === queue[index])?.children ?? [])
          if (!allowed.has(child)) {
            allowed.add(child);
            queue.push(child);
          }
      const targets = groom.rootProjection.nodeIds ?? regions.get(groom.region)?.nodeIds ?? [];
      if (!targets.length) error(groom.id, "Groom root projection requires declared anatomical field nodes");
      if (new Set(targets).size !== targets.length)
        error(groom.id, "Groom root projection node IDs must be unique");
      for (const node of targets) {
        if (!nodes.has(node)) error(groom.id, `Missing groom projection field node ${node}`);
        else if (!allowed.has(node))
          error(groom.id, `Groom projection node ${node} is outside the declared anatomy`);
      }
    }
    if (groom.chart && groom.chartRevision === undefined)
      error(groom.id, "Chart groom must declare chartRevision");
    if (
      groom.chartRevision !== undefined &&
      (!groom.chart || charts.get(groom.chart)?.revision !== groom.chartRevision)
    )
      error(groom.id, "Groom chart revision is stale; explicit correspondence repair required");
    if (Math.hypot(...groom.direction) < 1e-8) error(groom.id, "Groom direction must be nonzero");
    if (groom.lodFractions[0] !== 1 || groom.lodFractions.some((v, i, a) => i > 0 && v > a[i - 1]))
      error(groom.id, "Groom detail fractions must start at one and be nonincreasing");
  }
  for (const rule of source.influenceRules) {
    for (const id of [...rule.allowedJoints, ...rule.excludedJoints]) joint(rule.id, id);
    if (rule.rigidJoint) joint(rule.id, rule.rigidJoint);
    if (rule.allowedJoints.some((id) => rule.excludedJoints.includes(id)))
      error(rule.id, "Allowed and excluded joints overlap");
    if (
      rule.rigidJoint &&
      (rule.excludedJoints.includes(rule.rigidJoint) ||
        (rule.allowedJoints.length > 0 && !rule.allowedJoints.includes(rule.rigidJoint)))
    )
      error(rule.id, "Rigid joint contradicts influence restrictions");
    if (character.joints.every((j) => rule.excludedJoints.includes(j.id)))
      error(rule.id, "Influence rule excludes every joint");
  }
  for (const corrective of source.correctives) {
    joint(corrective.id, corrective.joint);
    if (Math.abs(corrective.angle) < 1e-8)
      error(corrective.id, "Corrective activation angle must be nonzero");
  }
  for (const contact of source.contacts) {
    joint(contact.id, contact.joint);
    const motion = motions.get(contact.motion);
    if (!motion) error(contact.id, `Missing contact motion ${contact.motion}`);
    if (contact.start >= contact.end || (motion && contact.end > motion.duration))
      error(contact.id, "Contact interval must have positive duration within its motion");
  }
  for (const chain of [...source.ikChains, ...source.secondaryChains]) {
    for (const id of chain.joints) joint(chain.id, id);
    if (new Set(chain.joints).size !== chain.joints.length) error(chain.id, "Chain joints must be unique");
    for (let i = 1; i < chain.joints.length; i++)
      if (joints.get(chain.joints[i])?.parent !== chain.joints[i - 1])
        error(chain.id, "Chain must follow direct parent-child skeleton order");
  }
  const ownedSecondary = new Set<string>();
  for (const chain of source.secondaryChains)
    for (const id of chain.joints) {
      if (ownedSecondary.has(id)) error(chain.id, `Secondary joint ${id} has multiple owners`);
      ownedSecondary.add(id);
    }
  for (const expression of source.expressions) {
    const seen = new Set<string>();
    for (const weight of expression.weights) {
      joint(expression.id, weight.joint);
      if (seen.has(weight.joint)) error(expression.id, "Expression joints must be unique");
      seen.add(weight.joint);
    }
  }
  const anchors = new Map(source.anchors.map((a) => [a.id, a]));
  const attachedNodes = new Set<string>();
  for (const attachment of source.attachments) {
    if (!anchors.has(attachment.anchor))
      error(attachment.id, `Missing attachment anchor ${attachment.anchor}`);
    if (attachment.rigidJoint) joint(attachment.id, attachment.rigidJoint);
    for (const node of attachment.nodeIds) {
      if (!nodes.has(node)) error(attachment.id, `Missing attachment field node ${node}`);
      const shape = character.field.nodes.find((n) => n.id === node);
      if (shape?.children.length) error(attachment.id, "Attachments require primitive leaf nodes");
      if (attachedNodes.has(node)) error(attachment.id, `Attachment node ${node} has multiple owners`);
      attachedNodes.add(node);
    }
  }
  for (const appearance of source.appearance) {
    if (!appearance.anchor) continue;
    const anchor = anchors.get(appearance.anchor);
    if (!anchor) error(appearance.id, `Missing appearance anchor ${appearance.anchor}`);
    else if (anchor.region !== appearance.region)
      error(appearance.id, "Appearance anchor belongs to a different region");
  }
  const clothCharts = new Set<string>();
  let clothParticleBudget = 0;
  for (const cloth of source.cloth) {
    const resolution = cloth.simulationResolution ?? 4;
    const axisKnots = (axis: 0 | 1) =>
      new Set([
        ...Array.from({ length: resolution + 1 }, (_, i) => i / resolution),
        ...cloth.pins.map((pin) => pin.coordinates[axis]),
      ]).size;
    clothParticleBudget += axisKnots(0) * axisKnots(1);
    if (clothParticleBudget > 4096) error(cloth.id, "Cloth exceeds the total 4096-particle source budget");
    const chart = charts.get(cloth.chart);
    if (!chart || chart.kind !== "patch" || chart.realization === "correspondence-only")
      error(cloth.id, "Cloth requires an existing patch chart");
    if (chart && (chart.region !== cloth.region || chart.revision !== cloth.chartRevision))
      error(cloth.id, "Cloth chart region or revision is stale; repair correspondence explicitly");
    if (cloth.fittingLandmarks) {
      if (new Set(cloth.fittingLandmarks).size !== 4)
        error(cloth.id, "Garment fitting requires four distinct corner landmarks");
      for (const [index, id] of cloth.fittingLandmarks.entries()) {
        const landmark = landmarks.get(id);
        if (!landmark || landmark.region !== cloth.region)
          error(cloth.id, `Missing garment fitting landmark ${id} in region ${cloth.region}`);
        else if (
          chart?.kind === "patch" &&
          chart.points[index].some((value, axis) => Math.abs(value - landmark.position[axis]) > 1e-9)
        )
          error(
            cloth.id,
            "Garment fitting is stale; move its landmarks with creature.landmark or detach its binding",
          );
      }
      if (chart?.kind === "patch" && !creaturePatchHasConsistentOrientation(chart.points))
        error(cloth.id, "Garment fitting corners must form an unfolded, nondegenerate patch");
    }
    if (clothCharts.has(cloth.chart)) error(cloth.id, "Cloth chart has multiple simulation owners");
    clothCharts.add(cloth.chart);
    if (!cloth.pinEdges.length && !cloth.pins.some((pin) => (pin.weight ?? 1) > 0))
      error(cloth.id, "Cloth requires at least one authored pin or pinned edge");
    if (new Set(cloth.pinEdges).size !== cloth.pinEdges.length)
      error(cloth.id, "Cloth pinned edges must be unique");
    const pins = new Set<string>();
    for (const pin of cloth.pins) {
      if (!anchors.has(pin.anchor)) error(cloth.id, `Missing cloth pin anchor ${pin.anchor}`);
      const key = pin.coordinates.join(":");
      if (pins.has(key)) error(cloth.id, "Cloth pin coordinates must be unique");
      pins.add(key);
    }
  }
  if (source.pelvis) joint(source.pelvis.joint, source.pelvis.joint);
  if (source.articulation) {
    const articulation = source.articulation;
    const bodies = new Set<string>();
    for (const body of articulation.bodies) {
      joint(body.joint, body.joint);
      if (bodies.has(body.joint)) error(body.joint, "Articulation body joints must be unique");
      bodies.add(body.joint);
    }
    unique(articulation.joints, "Articulation joint");
    const parents = new Map<string, string>();
    for (const constraint of articulation.joints) {
      if (!bodies.has(constraint.parent) || !bodies.has(constraint.child))
        error(constraint.id, "Articulation joint requires parent and child bodies");
      if (constraint.parent === constraint.child)
        error(constraint.id, "Articulation joint cannot join itself");
      if (parents.has(constraint.child))
        error(constraint.id, "Articulation child has multiple physical parents");
      parents.set(constraint.child, constraint.parent);
      if (constraint.kind === "revolute" && (!constraint.axis || Math.hypot(...constraint.axis) < 1e-8))
        error(constraint.id, "Revolute joint requires a nonzero hinge axis");
      if (
        constraint.minimum !== undefined &&
        constraint.maximum !== undefined &&
        constraint.minimum > constraint.maximum
      )
        error(constraint.id, "Articulation joint minimum exceeds maximum");
      if ((constraint.minimum === undefined) !== (constraint.maximum === undefined))
        error(constraint.id, "Articulation joint limits require both minimum and maximum");
      if (
        constraint.kind !== "revolute" &&
        (constraint.minimum !== undefined || constraint.maximum !== undefined)
      )
        error(constraint.id, "Only revolute articulation joints support angular limits");
    }
    for (const id of bodies) {
      const seen = new Set<string>([id]);
      let parent = parents.get(id);
      while (parent) {
        if (seen.has(parent)) {
          error(id, "Articulation contains a cycle");
          break;
        }
        seen.add(parent);
        parent = parents.get(parent);
      }
    }
  }
  for (const scenario of source.reviewScenarios) {
    if (scenario.motion && !motions.has(scenario.motion))
      error(scenario.id, `Missing review motion ${scenario.motion}`);
    unique(scenario.cameras, "Review camera");
    for (const camera of scenario.cameras)
      if (Math.hypot(...camera.position.map((v, i) => v - camera.target[i])) < 1e-8)
        error(scenario.id, "Review camera must differ from its target");
  }
  return diagnostics;
}
