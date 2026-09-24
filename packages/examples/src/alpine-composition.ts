import { createArchitectureLookdev } from "@wrela/examples/architecture-lookdev";
import type { Project, Vec3 } from "@wrela/model";
import { alpineSurfaceHistory, worldPathClearance } from "@wrela/model";

/** Structurally compatible with authoring operations, without a model -> authoring dependency. */
export type AlpineCompositionEdit = {
  kind: "document.set";
  target: string;
  path: (string | number)[];
  value: unknown;
};

/** A second composition changes the spatial problem as well as the construction source.
 * Existing document identities let normal authoring history, undo and saving own the edit. */
export function createAlpineCompositionEdits(
  project: Project,
  variant: "river-bend" = "river-bend",
): AlpineCompositionEdit[] {
  if (variant !== "river-bend") throw new Error("Unknown alpine composition");
  const world = project.documents.find((document) => document.id === project.entry);
  if (world?.kind !== "world" || !world.composition)
    throw new Error("Alpine composition requires a composed world");
  const gateway = project.documents.find((document) => document.id === "alpine-lookdev-gateway");
  const terrain = project.documents.find((document) => document.id === world.terrain);
  const stage = project.documents.find(
    (document) => document.kind === "stage" && document.subjects.includes(world.id),
  );
  if (
    gateway?.kind !== "object" ||
    terrain?.kind !== "terrain" ||
    !terrain.geology ||
    stage?.kind !== "stage"
  )
    throw new Error("Alpine construction, terrain and review stage are required");
  const heldout = createArchitectureLookdev({
    variant: "wayside",
    seed: 281,
    history: alpineSurfaceHistory({ seed: 281, ageYears: 230, prevailingWetness: 0.28, damage: 0.55 }),
  });
  const shelter = heldout.documents.find((document) => document.id === heldout.object);
  if (shelter?.kind !== "object" || !shelter.assembly) throw new Error("Missing held-out construction");
  for (const part of shelter.assembly.parts)
    part.material = part.material?.replace("wayside-history-", "gateway-history-");
  const dependencies = [...new Set(shelter.assembly.parts.map((part) => part.material ?? gateway.material))];
  for (const id of dependencies)
    if (!project.documents.some((document) => document.id === id))
      throw new Error(`Missing shared construction material ${id}`);
  const composition = structuredClone(world.composition);
  const landmark: Vec3 = [-9.8, 1, 12.2];
  const placement = composition.placements.find((entry) => entry.id === "ruin-on-terrace");
  if (!placement) throw new Error("Missing alpine landmark placement");
  placement.position = landmark;
  placement.yaw = 0.65;
  const approach = composition.paths.find((path) => path.id === "approach"),
    branch = composition.paths.find((path) => path.id === "gate-branch");
  if (!approach || !branch) throw new Error("Missing alpine approach paths");
  approach.points = [
    [-3.8, 0.5, -21],
    [-3.7, 0.5, -13],
    [-2.8, 0.5, -6],
    [-3.8, 0.5, 1],
    [-4.5, 0.7, 8],
  ];
  branch.points = [
    [-4.5, 0.7, 8],
    [-5.8, 0.88, 11.5],
    [-6.6, 1, 14],
    [-7.8, 1, 14.7],
  ];
  approach.width = 2.1;
  branch.width = 1.8;
  for (const space of composition.spaces)
    if (space.id === "old-mountain-gate") {
      space.center = landmark;
      space.radius = 4.2;
    }
  if (composition.review) composition.review.sightline = { from: [-14, 2.7, 20], to: [-9.8, 2.2, 12.2] };
  const instances = structuredClone(world.instances).filter((instance) => {
    const definition = project.documents.find((document) => document.id === instance.definition);
    return (
      definition?.kind !== "vegetation" ||
      worldPathClearance(instance.position, composition.paths) >
        (definition.botanical?.species === "shrub" ? 1.1 : 0.4)
    );
  });
  for (const instance of instances) {
    if (instance.id.startsWith("grove-pine-")) instance.rotation[1] += 0.6;
    if (instance.id === "alpine-debris-gate-contact") instance.position = [-12.1, 0, 12.6];
    if (instance.id === "alpine-debris-gate-collapse") instance.position = [-7.7, 0, 11.8];
  }
  const geology = structuredClone(terrain.geology);
  const route = [...approach.points, ...branch.points.slice(1)];
  geology.corridors = [{ id: "wayside-approach", points: route, halfWidth: 1.15, shoulder: 1.5 }];
  geology.review.route = route.map((point) => [point[0], point[2]]);
  const interventions = structuredClone(terrain.interventions);
  const terrace = interventions.find((intervention) => intervention.id === "ruin-terrace");
  if (terrace) {
    terrace.center = [landmark[0], landmark[2]];
    terrace.radius = 6.5;
    terrace.targetHeight = 1;
  }
  const set = (target: string, path: string, value: unknown): AlpineCompositionEdit => ({
    kind: "document.set",
    target,
    path: [path],
    value,
  });
  const materialEdits = heldout.documents.flatMap((document) =>
    document.kind === "material"
      ? [set(document.id.replace("wayside-history-", "gateway-history-"), "appearance", document.appearance)]
      : [],
  );
  return [
    ...materialEdits,
    set(gateway.id, "name", "Roofless wayside shelter at the river bend"),
    set(gateway.id, "assembly", shelter.assembly),
    set(gateway.id, "field", shelter.field),
    set(world.id, "name", "River bend and abandoned wayside shelter"),
    set(world.id, "composition", composition),
    set(world.id, "instances", instances),
    set(terrain.id, "geology", geology),
    set(terrain.id, "interventions", interventions),
    set(stage.id, "camera", { position: [-14, 4, 20], target: [-9.8, 1.7, 12.2], fov: 49 }),
  ];
}
