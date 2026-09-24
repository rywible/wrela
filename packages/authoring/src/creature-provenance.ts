import { type CharacterDefinition, type CompiledCharacter, contentKey, type Vec3 } from "@wrela/model";

export type CreatureControlRef = { domain: string; id: string };
export type CreatureFieldUnit =
  | "metres"
  | "radians"
  | "seconds"
  | "kilograms"
  | "per-square-metre"
  | "metres-per-second-squared"
  | "unitless"
  | "count"
  | "hertz";
export type CreatureFieldSpace =
  | "character-rest"
  | "region-local"
  | "joint-local"
  | "field-local"
  | "world"
  | "chart"
  | "none";
export type CreatureControlDescriptor = CreatureControlRef & {
  path: (string | number)[];
  operation?: string;
  stage: "compile" | "pose" | "simulation" | "review";
  support:
    | { kind: "region"; region: string }
    | { kind: "sphere"; region: string; center: Vec3; radius: number }
    | {
        kind: "sculpt-field";
        region: string;
        center: Vec3;
        path?: Vec3[];
        radii: Vec3;
        rotation: Vec3;
        mirror: boolean;
        nodeIds?: string[];
      }
    | { kind: "global" };
  parameters: {
    path: (string | number)[];
    value: unknown;
    unit: CreatureFieldUnit;
    space: CreatureFieldSpace;
  }[];
  dependencies: (CreatureControlRef & { reason: string })[];
};
const key = (ref: CreatureControlRef) => `${ref.domain}:${ref.id}`;
const ref = (domain: string, id: string): CreatureControlRef => ({ domain, id });
const operation: Record<string, string> = {
  regions: "creature.region",
  landmarks: "creature.landmark",
  charts: "creature.chart",
  anchors: "creature.anchor",
  sculpts: "creature.sculpt",
  appearance: "creature.appearance",
  grooms: "creature.groom",
  influenceRules: "creature.influence",
  correctives: "creature.corrective",
  contacts: "creature.contact",
  ikChains: "creature.ik",
  secondaryChains: "creature.secondary",
  expressions: "creature.expression",
  reviewScenarios: "creature.review",
  attachments: "creature.attachment",
  cloth: "creature.cloth",
  articulation: "creature.articulation",
  pelvis: "creature.pelvis",
};
type ParameterSpec = [string, CreatureFieldUnit, CreatureFieldSpace];
const parameters: Record<string, ParameterSpec[]> = {
  regions: [
    ["frame.position", "metres", "character-rest"],
    ["frame.rotation", "radians", "character-rest"],
    ["extent", "metres", "region-local"],
  ],
  landmarks: [["position", "metres", "region-local"]],
  charts: [
    ["realization", "unitless", "none"],
    ["points", "metres", "region-local"],
    ["radii", "metres", "region-local"],
    ["crossSections", "metres", "region-local"],
    ["twists", "radians", "region-local"],
    ["thickness", "metres", "region-local"],
    ["revision", "count", "none"],
  ],
  anchors: [
    ["coordinates", "unitless", "chart"],
    ["offset", "metres", "region-local"],
    ["tolerance", "metres", "character-rest"],
  ],
  sculpts: [
    ["support.radii", "metres", "region-local"],
    ["support.rotation", "radians", "region-local"],
    ["path", "metres", "region-local"],
    ["mode", "unitless", "none"],
    ["nodeIds", "unitless", "none"],
    ["detail.maxEdgeLength", "metres", "region-local"],
    ["center", "metres", "region-local"],
    ["radius", "metres", "region-local"],
    ["displacement", "metres", "region-local"],
    ["strength", "unitless", "none"],
    ["falloff", "unitless", "none"],
  ],
  appearance: [
    ["color", "unitless", "none"],
    ["roughness", "unitless", "none"],
    ["metallic", "unitless", "none"],
    ["subsurface", "unitless", "none"],
    ["transmission", "unitless", "none"],
    ["anisotropy", "unitless", "none"],
    ["direction", "unitless", "region-local"],
    ["mask.center", "metres", "region-local"],
    ["mask.radius", "metres", "region-local"],
    ["displacement", "metres", "region-local"],
    ["growthSuppression", "unitless", "none"],
  ],
  grooms: [
    ["rootProjection.maxDistance", "metres", "character-rest"],
    ["rootProjection.direction", "unitless", "none"],
    ["density", "per-square-metre", "region-local"],
    ["length", "metres", "region-local"],
    ["width", "metres", "region-local"],
    ["direction", "unitless", "region-local"],
    ["maxCards", "count", "none"],
    ["lodFractions", "unitless", "none"],
  ],
  correctives: [
    ["center", "metres", "region-local"],
    ["radius", "metres", "region-local"],
    ["displacement", "metres", "region-local"],
    ["angle", "radians", "joint-local"],
  ],
  contacts: [
    ["start", "seconds", "none"],
    ["end", "seconds", "none"],
    ["target", "metres", "character-rest"],
    ["tolerance", "metres", "world"],
    ["weight", "unitless", "none"],
  ],
  ikChains: [
    ["target", "metres", "character-rest"],
    ["pole", "metres", "character-rest"],
    ["weight", "unitless", "none"],
    ["iterations", "count", "none"],
    ["tolerance", "metres", "character-rest"],
  ],
  secondaryChains: [
    ["gravity", "metres-per-second-squared", "world"],
    ["wind", "metres-per-second-squared", "world"],
    ["maxAngle", "radians", "joint-local"],
    ["collisionRadius", "metres", "character-rest"],
    ["weight", "unitless", "none"],
  ],
  cloth: [
    ["simulationResolution", "count", "chart"],
    ["gravity", "metres-per-second-squared", "world"],
    ["wind", "metres-per-second-squared", "world"],
    ["collisionRadius", "metres", "character-rest"],
    ["maxStretch", "unitless", "none"],
    ["stiffness", "unitless", "none"],
    ["bendStiffness", "unitless", "none"],
  ],
  attachments: [
    ["offset", "metres", "region-local"],
    ["minimumClearance", "metres", "character-rest"],
  ],
  joints: [
    ["position", "metres", "character-rest"],
    ["rotation", "radians", "joint-local"],
    ["radius", "metres", "character-rest"],
    ["minimum", "radians", "joint-local"],
    ["maximum", "radians", "joint-local"],
  ],
  motions: [["duration", "seconds", "none"]],
  "field.nodes": [
    ["position", "metres", "character-rest"],
    ["rotation", "radians", "character-rest"],
    ["size", "metres", "character-rest"],
    ["radius", "metres", "character-rest"],
  ],
  pelvis: [
    ["maxOffset", "metres", "joint-local"],
    ["weight", "unitless", "none"],
    ["iterations", "count", "none"],
  ],
  reviewScenarios: [
    ["duration", "seconds", "none"],
    ["sampleRate", "hertz", "none"],
  ],
};
function valueAt(value: unknown, path: string[]): unknown {
  let current = value;
  for (const part of path) {
    if (!current || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}
/** Build a source-specific graph, with exact public edit paths and physical units.
 * Relationships are declared control dependencies, never inferred pixel sensitivities. */
export function inspectCreatureFields(character: CharacterDefinition): {
  sourceKey: string;
  controls: CreatureControlDescriptor[];
} {
  const source = character.creature;
  if (!source) throw new Error("Character has no creature source");
  const controls: CreatureControlDescriptor[] = [];
  const add = (domain: string, id: string, value: unknown, path: (string | number)[]) => {
    const record = value as Record<string, unknown>,
      region = typeof record.region === "string" ? record.region : domain === "regions" ? id : undefined;
    const dependencies: CreatureControlDescriptor["dependencies"] = [];
    const dep = (domain: string, id: unknown, reason: string) => {
      if (typeof id === "string") dependencies.push({ ...ref(domain, id), reason });
    };
    if (region && domain !== "regions") dep("regions", region, "uses the anatomical frame and support");
    dep("charts", record.chart, "uses persistent surface correspondence");
    dep("anchors", record.anchor, "follows an authored surface attachment");
    dep("landmarks", record.landmark, "follows the fitted anatomical landmark");
    dep("materials", record.material, "uses the material definition");
    if (domain === "regions") dep("regions", record.parent, "belongs to the parent anatomy");
    if (domain === "field.nodes")
      for (const child of record.children as string[]) dep("field.nodes", child, "evaluates child field");
    if (domain === "joints") dep("joints", record.parent, "inherits parent pose");
    if (domain === "grooms")
      for (const appearance of source.appearance)
        if (appearance.region === region && appearance.growthSuppression > 0)
          dep("appearance", appearance.id, "shares a growth-suppression mask");
    if (domain === "grooms") {
      const groom = source.grooms.find((item) => item.id === id);
      if (groom?.rootProjection)
        for (const node of groom.rootProjection.nodeIds ??
          source.regions.find((item) => item.id === region)?.nodeIds ??
          [])
          dep("field.nodes", node, "projects growth roots to the realized anatomical surface");
    }
    if (domain === "attachments")
      for (const node of record.nodeIds as string[])
        dep("field.nodes", node, "mounts authored primitive geometry");
    if (domain === "cloth") {
      const cloth = source.cloth.find((c) => c.id === id);
      for (const pin of cloth?.pins ?? []) dep("anchors", pin.anchor, "pins cloth to the anatomical surface");
      for (const landmark of cloth?.fittingLandmarks ?? [])
        dep("landmarks", landmark, "fits a persistent garment corner");
    }
    if (domain === "influenceRules")
      for (const joint of [...(record.allowedJoints as string[]), ...(record.excludedJoints as string[])])
        dep("joints", joint, "constrains anatomical binding");
    if (domain === "ikChains" || domain === "secondaryChains")
      for (const joint of record.joints as string[]) dep("joints", joint, "controls joint motion");
    if (domain === "expressions")
      for (const item of source.expressions.find((x) => x.id === id)?.weights ?? [])
        dep("joints", item.joint, "layers joint expression");
    if (domain === "motions")
      for (const item of character.motions.find((x) => x.id === id)?.keys ?? [])
        dep("joints", item.joint, "animates this joint");
    if (domain === "articulation")
      for (const body of source.articulation?.bodies ?? [])
        dep("joints", body.joint, "physically owns this body joint");
    dep("joints", record.joint, "uses this skeleton joint");
    dep("joints", record.rigidJoint, "uses rigid skin binding");
    dep("motions", record.motion, "uses clip timing");
    const specs = (parameters[domain] ?? [])
      .filter(
        ([name]) =>
          domain !== "field.nodes" ||
          ((name !== "radius" || ["sphere", "capsule", "torus"].includes(String(record.kind))) &&
            (name !== "size" || ["ellipsoid", "box", "capsule", "torus"].includes(String(record.kind)))),
      )
      .map(([name, unit, space]) => {
        if (domain === "field.nodes") space = "field-local";
        if (domain === "anchors" && name === "coordinates" && !record.chart) {
          unit = "metres";
          space = "region-local";
        }
        if (domain === "contacts" && name === "target")
          space = record.space === "world" ? "world" : "character-rest";
        const selectedPath: (string | number)[] = name.split(".");
        if (domain === "field.nodes" && name === "size" && ["capsule", "torus"].includes(String(record.kind)))
          selectedPath.push(record.kind === "capsule" ? 1 : 0);
        return {
          path: [...path, ...selectedPath],
          value: valueAt(value, selectedPath.map(String)),
          unit,
          space,
        };
      })
      .filter((p) => p.value !== undefined);
    const parameter = (
      suffix: (string | number)[],
      value: unknown,
      unit: CreatureFieldUnit,
      space: CreatureFieldSpace,
    ) => {
      if (value !== undefined)
        specs.push({ path: [...path, ...suffix], value: structuredClone(value), unit, space });
    };
    if (domain === "articulation") {
      source.articulation?.bodies.forEach((body, index) => {
        parameter(["bodies", index, "mass"], body.mass, "kilograms", "none");
        parameter(["bodies", index, "radius"], body.radius, "metres", "joint-local");
        parameter(["bodies", index, "halfHeight"], body.halfHeight, "metres", "joint-local");
        parameter(["bodies", index, "offset"], body.offset, "metres", "joint-local");
      });
      source.articulation?.joints.forEach((joint, index) => {
        parameter(["joints", index, "axis"], joint.axis, "unitless", "joint-local");
        parameter(["joints", index, "minimum"], joint.minimum, "radians", "joint-local");
        parameter(["joints", index, "maximum"], joint.maximum, "radians", "joint-local");
      });
    }
    if (domain === "motions")
      character.motions
        .find((m) => m.id === id)
        ?.keys.forEach((item, index) => {
          parameter(["keys", index, "time"], item.time, "seconds", "none");
          parameter(["keys", index, "rotation"], item.rotation, "radians", "joint-local");
          parameter(["keys", index, "translation"], item.translation, "metres", "joint-local");
        });
    if (domain === "grooms")
      source.grooms
        .find((g) => g.id === id)
        ?.guides.forEach((guide, index) => {
          parameter(["guides", index, "points"], guide.points, "metres", "region-local");
        });
    if (domain === "cloth")
      source.cloth
        .find((c) => c.id === id)
        ?.pins.forEach((pin, index) => {
          parameter(["pins", index, "coordinates"], pin.coordinates, "unitless", "chart");
          parameter(["pins", index, "weight"], pin.weight ?? 1, "unitless", "none");
        });
    const center = record.center as Vec3 | undefined,
      radius = record.radius;
    const sculpt = domain === "sculpts" ? source.sculpts.find((s) => s.id === id) : undefined;
    const support: CreatureControlDescriptor["support"] = sculpt
      ? {
          kind: "sculpt-field",
          region: sculpt.region,
          center: [...sculpt.center],
          path: structuredClone(sculpt.path),
          radii: sculpt.support?.radii ?? [sculpt.radius, sculpt.radius, sculpt.radius],
          rotation: sculpt.support?.rotation ?? [0, 0, 0],
          mirror: sculpt.mirror,
          nodeIds: sculpt.nodeIds,
        }
      : region
        ? center && typeof radius === "number"
          ? { kind: "sphere", region, center: [...center], radius }
          : { kind: "region", region }
        : { kind: "global" };
    controls.push({
      domain,
      id,
      path,
      operation: operation[domain],
      stage: ["contacts", "ikChains", "expressions", "correctives", "pelvis"].includes(domain)
        ? "pose"
        : ["cloth", "secondaryChains", "articulation"].includes(domain)
          ? "simulation"
          : domain === "reviewScenarios"
            ? "review"
            : "compile",
      support,
      parameters: specs,
      dependencies: [...new Map(dependencies.map((d) => [key(d), d])).values()],
    });
  };
  for (const [domain, value] of Object.entries(source)) {
    if (Array.isArray(value))
      value.forEach((entry, index) => {
        add(domain, entry.id, entry, ["creature", domain, index]);
      });
    else if (value && (domain === "articulation" || domain === "pelvis"))
      add(domain, domain, value, ["creature", domain]);
  }
  character.field.nodes.forEach((value, index) => {
    add("field.nodes", value.id, value, ["field", "nodes", index]);
  });
  character.joints.forEach((value, index) => {
    add("joints", value.id, value, ["joints", index]);
  });
  character.motions.forEach((value, index) => {
    add("motions", value.id, value, ["motions", index]);
  });
  return structuredClone({ sourceKey: contentKey(character), controls });
}
export function traceCreatureDependencies(
  character: CharacterDefinition,
  start: CreatureControlRef,
  direction: "dependencies" | "dependents" = "dependents",
) {
  const index = inspectCreatureFields(character),
    byKey = new Map(index.controls.map((c) => [key(c), c]));
  if (!byKey.has(key(start))) throw new Error(`Unknown creature control ${key(start)}`);
  const edges = index.controls.flatMap((control) =>
    control.dependencies.map((input) => ({
      from: ref(control.domain, control.id),
      to: ref(input.domain, input.id),
      reason: input.reason,
    })),
  );
  const seen = new Set([key(start)]),
    queue = [start],
    traversed: typeof edges = [];
  while (queue.length) {
    const next = queue.shift();
    if (!next) break;
    for (const edge of edges) {
      const from = direction === "dependencies" ? edge.from : edge.to,
        to = direction === "dependencies" ? edge.to : edge.from;
      if (key(from) !== key(next)) continue;
      traversed.push(edge);
      if (!seen.has(key(to))) {
        seen.add(key(to));
        queue.push(to);
      }
    }
  }
  return {
    sourceKey: index.sourceKey,
    direction,
    origin: start,
    controls: index.controls.filter((c) => seen.has(key(c))),
    edges: traversed,
    basis: "declared-source-dependencies" as const,
  };
}

export type CreatureHitRequest = {
  sourceKey: string;
  artifactKey: string;
  realizationKey: string;
  triangle: number;
  barycentric: Vec3;
  sourceRevision: number;
  captureId?: string;
};
export function creatureGroundingStamp(
  character: CharacterDefinition,
  artifact: CompiledCharacter,
  sourceRevision: number,
) {
  return {
    sourceKey: contentKey(character),
    artifactKey: artifact.key,
    realizationKey: contentKey([
      artifact.mesh.positions,
      artifact.mesh.indices,
      artifact.mesh.sourceIds,
      artifact.creatureGroomDetail,
    ]),
    sourceRevision,
  };
}
/** Ground an actual realized triangle hit. The host provides the artifact currently
 * drawn, including selected LOD. No nearest-region guess is substituted for a hit. */
export function groundCreatureHit(
  character: CharacterDefinition,
  artifact: CompiledCharacter,
  request: CreatureHitRequest,
) {
  const source = character.creature;
  if (!source) throw new Error("Character has no creature source");
  const sourceKey = contentKey(character);
  if (request.sourceKey !== sourceKey || artifact.creatureSourceKey !== sourceKey)
    throw new Error("Stale creature source; capture and compile the current revision before grounding");
  if (
    request.realizationKey !==
    creatureGroundingStamp(character, artifact, request.sourceRevision).realizationKey
  )
    throw new Error("Hit belongs to a different realized mesh or detail level");
  if (artifact.id !== character.id || request.artifactKey !== artifact.key)
    throw new Error("Hit belongs to a different compiled artifact");
  if (!Number.isSafeInteger(request.sourceRevision) || request.sourceRevision < 0)
    throw new Error("Invalid grounding source revision");
  if (
    !Number.isSafeInteger(request.triangle) ||
    request.triangle < 0 ||
    request.triangle * 3 + 2 >= artifact.mesh.indices.length
  )
    throw new Error("Hit triangle is outside the realized mesh");
  if (
    request.barycentric.length !== 3 ||
    request.barycentric.some((v) => !Number.isFinite(v) || v < -1e-7 || v > 1 + 1e-7) ||
    Math.abs(request.barycentric.reduce((a, b) => a + b, 0) - 1) > 1e-6
  )
    throw new Error("Hit barycentric weights must sum to one");
  const bary = request.barycentric.map((v) => Math.max(0, v)) as Vec3,
    total = bary.reduce((a, b) => a + b, 0);
  for (let i = 0; i < 3; i++) bary[i] /= total;
  const vertices = Array.from(artifact.mesh.indices.slice(request.triangle * 3, request.triangle * 3 + 3));
  const regions = new Map<string, number>(),
    nodes = new Map<string, number>(),
    joints = new Map<string, number>();
  const restPoint: Vec3 = [0, 0, 0];
  for (let corner = 0; corner < 3; corner++) {
    const vertex = vertices[corner],
      weight = bary[corner],
      region = artifact.creatureRegions?.[vertex],
      node = artifact.mesh.sourceIds?.[vertex];
    if (region) regions.set(region, (regions.get(region) ?? 0) + weight);
    if (node) nodes.set(node, (nodes.get(node) ?? 0) + weight);
    for (let axis = 0; axis < 3; axis++)
      restPoint[axis] += artifact.mesh.positions[vertex * 3 + axis] * weight;
    for (let i = 0; i < 4; i++) {
      const joint = artifact.joints[artifact.jointIndices[vertex * 4 + i]]?.id;
      if (joint) joints.set(joint, (joints.get(joint) ?? 0) + artifact.weights[vertex * 4 + i] * weight);
    }
  }
  const coordinates = vertices.map((v) => artifact.creatureCoordinates?.[v]);
  const first = coordinates[0];
  let chartCoordinate:
    | { region: string; chart: string; chartRevision: number; coordinates: Vec3 }
    | undefined;
  const status =
    first &&
    coordinates.every(
      (c) =>
        c && c.chart === first.chart && c.region === first.region && c.chartRevision === first.chartRevision,
    )
      ? "resolved"
      : coordinates.some(Boolean)
        ? "ambiguous"
        : "unavailable";
  if (status === "resolved" && first) {
    const chart = source.charts.find((c) => c.id === first.chart);
    if (!chart || chart.revision !== first.chartRevision)
      throw new Error("Hit chart correspondence is stale");
    const value: Vec3 = [0, 0, 0];
    coordinates.forEach((c, i) => {
      if (!c) return;
      for (let axis = 0; axis < 3; axis++) {
        let coordinate = c.coordinates[axis];
        if (chart.kind === "sweep" && axis === 1) {
          const difference = coordinate - first.coordinates[1];
          if (difference > 0.5) coordinate -= 1;
          else if (difference < -0.5) coordinate += 1;
        }
        value[axis] += coordinate * bary[i];
      }
    });
    if (chart.kind === "sweep") value[1] = ((value[1] % 1) + 1) % 1;
    chartCoordinate = {
      region: first.region,
      chart: first.chart,
      chartRevision: first.chartRevision,
      coordinates: value,
    };
  }
  const index = inspectCreatureFields(character),
    selectedRegions = new Set(regions.keys()),
    selectedNodes = new Set(nodes.keys());
  const controls = index.controls.filter(
    (c) =>
      (c.support.kind !== "global" && selectedRegions.has(c.support.region)) ||
      (c.domain === "field.nodes" && selectedNodes.has(c.id)) ||
      (c.domain === "joints" && (joints.get(c.id) ?? 0) > 1e-8) ||
      c.dependencies.some(
        (input) =>
          (input.domain === "joints" && (joints.get(input.id) ?? 0) > 1e-8) ||
          (input.domain === "field.nodes" && selectedNodes.has(input.id)),
      ),
  );
  const material =
    artifact.mesh.materialGroups?.find(
      (group) => request.triangle * 3 >= group.start && request.triangle * 3 < group.start + group.count,
    )?.material ?? artifact.material;
  const weighted = (values: Map<string, number>) =>
    [...values]
      .filter(([, weight]) => weight > 1e-8)
      .map(([id, weight]) => ({ id, weight }))
      .sort((a, b) => b.weight - a.weight || a.id.localeCompare(b.id));
  return {
    sourceRevision: request.sourceRevision,
    sourceKey,
    artifactKey: artifact.key,
    realizationKey: request.realizationKey,
    captureId: request.captureId,
    triangle: request.triangle,
    barycentric: bary,
    restPoint,
    space: "character-rest" as const,
    correspondence: { status, coordinate: chartCoordinate, corners: structuredClone(coordinates) },
    regions: weighted(regions),
    geometrySources: weighted(nodes),
    skinInfluences: weighted(joints),
    material,
    groomRoots: [...new Set(coordinates.flatMap((c) => (c?.id ? [c.id] : [])))],
    controls,
    limitations: [
      "Triangle barycentric support and skin weights identify source provenance, not measured lighting contribution or solver sensitivity.",
      ...(status !== "resolved"
        ? [
            "This hit has no unique chart coordinate; choose a compatible surface before authoring a chart-bound detail.",
          ]
        : []),
    ],
  };
}
