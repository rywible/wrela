import { buildGeologyObject, compileDocument } from "@wrela/compiler";
import { createLookdevMaterials, referenceProject, shapedPineLookdevDefinition } from "@wrela/examples";
import {
  type Camera,
  type CompiledSurface,
  geologyFormationSchema,
  identityMatrix,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";
import { renderMaterial } from "@wrela/runtime";
import { box } from "./indirect-scenes";
import { lightingRoomFixture } from "./lighting-room";

export const LIGHTING_ROUTE_VIEWS: Record<string, Camera> = {
  exterior: { position: [7.8, 2.1, 11], target: [-0.7, 1.4, 0], fov: 58 },
  entrance: { position: [2.25, 1.65, 5.5], target: [-0.6, 1.2, -1.6], fov: 65 },
  room: { position: [2.6, 1.65, 1.8], target: [-0.8, 1.05, -1.65], fov: 67 },
  cave: { position: [0.15, 1.65, -7.9], target: [0, 1.45, -2.6], fov: 65 },
  deep: { position: [0.2, 1.65, -13.1], target: [0, 1.45, -4.5], fov: 65 },
};

/** Connected lighting stress scene. Production procedural foliage, geology and
 * materials surround the scale-controlled furnished room. This is an acceptance
 * fixture, not a claim that the deliberately simple building is finished art. */
export function lightingRouteScene(night = false) {
  const scene = lightingRoomFixture(night);
  const materials = new Map(createLookdevMaterials().map((m) => [m.id, renderMaterial(m)]));
  const material = (name: string) => {
    const value = materials.get(`alpine-lookdev-${name}`);
    if (!value) throw Error(`Missing route material: ${name}`);
    return structuredClone(value);
  };
  scene.surfaces = scene.surfaces.filter((s) => !["back-wall", "front-right"].includes(s.id));
  for (const surface of scene.surfaces) {
    if (/^(table$|shelf)/.test(surface.id)) surface.material = material("wood");
    if (/^(floor|left-wall)/.test(surface.id)) surface.material = material("masonry");
    // Leave the main plaster receiver neutral: its gradients reveal leakage.
    if (surface.id.startsWith("sofa")) {
      surface.matrix[12] = 0.25;
      surface.matrix[14] = 4.7;
    }
  }
  const add = (id: string, min: Vec3, max: Vec3, name = "masonry") => {
    const surface = box(id, min, max, [0.3, 0.3, 0.3]);
    surface.material = material(name);
    scene.surfaces.push(surface);
    return surface;
  };
  add("entrance-left", [0.5, 0, 3], [1.5, 3, 3.18]);
  add("entrance-right", [2.8, 0, 3], [3, 3, 3.18]);
  add("entrance-header", [1.5, 2.55, 3], [2.8, 3, 3.18]);
  add("cave-portal-left", [-3, 0, -3.45], [-0.9, 3.1, -3]);
  add("cave-portal-right", [0.9, 0, -3.45], [3, 3.1, -3]);
  add("cave-portal-header", [-0.9, 2.55, -3.45], [0.9, 3.1, -3]);
  add("courtyard", [-14, -0.25, 3], [14, -0.025, 21], "ground");
  add("cave-floor", [-3, -0.25, -15.8], [3, 0, -3], "rock-dark");
  add("cave-end", [-3.2, 0, -15.8], [3.2, 7, -14.9], "rock");
  // Roof and back seal the finite outcrop. Geometry, rather than an ambient
  // override, must make the deep end dark when its only entrance is closed.
  add("cave-cap", [-3.2, 6.4, -15.8], [3.2, 6.8, -3], "rock");
  const door = add("cave-door", [-1.84, 0, -0.07], [0, 2.62, 0.07], "wood");
  door.lightingMobility = "dynamic";
  const setDoor = (angle: number, dynamic = true) => {
    door.matrix = identityMatrix();
    door.matrix[0] = door.matrix[10] = Math.cos(angle);
    door.matrix[2] = -Math.sin(angle);
    door.matrix[8] = Math.sin(angle);
    door.matrix[12] = 0.92;
    door.matrix[14] = -3.26;
    door.lightingMobility = dynamic ? "dynamic" : undefined;
    // Caches attach cloned packets; replace by stable source identity each time.
    scene.surfaces = scene.surfaces.map((s) => (s.id === door.id ? { ...door } : s));
  };
  setDoor(1.5);
  const append = (surface: CompiledSurface, id: string, position: Vec3, wind?: number) => {
    const matrix = identityMatrix();
    matrix.set(position, 12);
    const groups = surface.mesh.materialGroups ?? [
      { material: surface.material, start: 0, count: surface.mesh.indices.length },
    ];
    for (const [index, group] of groups.entries()) {
      const packet: RenderSurface = {
        id: `${id}-${index}`,
        source: surface.id,
        mesh: surface.mesh,
        matrix,
        material: structuredClone(materials.get(group.material) ?? material("rock")),
        drawRange: surface.mesh.materialGroups ? { start: group.start, count: group.count } : undefined,
        opaqueVisibility: surface.opaqueVisibility,
        renderProducts: surface.renderProducts,
        details: surface.mesh.materialGroups ? undefined : surface.details,
        wind,
      };
      scene.surfaces.push(packet);
    }
  };
  const terrain = referenceProject().documents.find((d) => d.kind === "terrain");
  if (!terrain || terrain.kind !== "terrain") throw Error("Missing reference terrain");
  const formation = geologyFormationSchema.parse({
    id: "lighting-cave",
    kind: "cave",
    material: "alpine-lookdev-rock",
    position: [0, 0, -9.2],
    size: [6, 7, 12],
    opening: 0.6,
    rock: { seed: 47, layers: 5, fracture: 0.5 },
    resolution: 48,
  });
  const cave = compileDocument(buildGeologyObject(terrain, formation), "review");
  if (cave?.kind !== "surface") throw Error("Unexpected cave artifact");
  append(cave, "geological-cave", formation.position);
  const tree = compileDocument(shapedPineLookdevDefinition(), "review");
  if (tree?.kind !== "vegetation") throw Error("Unexpected vegetation artifact");
  for (const [index, position] of (
    [
      [6, 0, 4],
      [-5.3, 0, 6.2],
    ] as Vec3[]
  ).entries())
    for (const surface of tree.surfaces)
      append(surface, `pine-${index}-${surface.id}`, position, tree.windResponse);
  scene.camera = structuredClone(LIGHTING_ROUTE_VIEWS.exterior);
  return { scene, setDoor };
}

export type LightingRouteScene = ReturnType<typeof lightingRouteScene>;
