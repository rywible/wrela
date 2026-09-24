import { createAssemblyPart } from "@wrela/authoring";
import { referenceProject, shapedPineLookdevDefinition } from "@wrela/examples";
import {
  alpineConiferMaterials,
  botanicalPreset,
  paperBirchMaterials,
  parseProject,
  type VegetationDefinition,
} from "@wrela/model";
import { fastAuthoringProject } from "./fast-authoring";
export function transferTree(heldout = false) {
  const project = referenceProject();
  const tree: VegetationDefinition = shapedPineLookdevDefinition();
  tree.id = heldout ? "heldout-birch" : "study-tree";
  tree.name = heldout ? "Held-out birch" : "Exposed trail pine";
  tree.seed = heldout ? 971 : 127;
  tree.height = heldout ? 4 : 5;
  tree.radius = heldout ? 1.4 : 1.8;
  if (heldout) tree.botanical = botanicalPreset("birch");
  const palette = alpineConiferMaterials();
  if (heldout) {
    const birch = paperBirchMaterials();
    palette.bark = { ...birch.bark, id: palette.bark.id };
    palette.needles = { ...birch.leaves, id: palette.needles.id };
  }
  const materials = Object.values(palette);
  const ids = new Set([...materials.map((m) => m.id), tree.id]);
  project.documents = [...project.documents.filter((d) => !ids.has(d.id)), ...materials, tree];
  project.entry = tree.id;
  return parseProject(project);
}
export function transferTimber(heldout = false) {
  const project = fastAuthoringProject(2.55, 2.6);
  if (heldout) {
    const d = project.documents.find((d) => d.id === "timber-frame");
    if (d?.kind !== "object" || !d.assembly) throw Error("Missing timber");
    d.name = "Braced fence section";
    d.assembly.parts[0].path[1][1] = 1.2;
    d.assembly.parts[1].path[1][1] = 1.2;
    d.assembly.parts[2].position[1] = 1.325;
    const brace = createAssemblyPart("diagonal");
    brace.path = [
      [-1.2, 0.2, 0.18],
      [1.2, 1.05, 0.18],
    ];
    brace.profile = { kind: "rectangle", width: 0.08, height: 0.08 };
    d.assembly.parts.push(brace);
  }
  return parseProject(project);
}
