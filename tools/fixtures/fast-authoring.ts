import { createDoorwayAssembly } from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import { type ObjectDefinition, parseProject } from "@wrela/model";
export function fastAuthoringProject(width = 2, height = 2.6) {
  const project = referenceProject();
  const template = project.documents.find((d) => d.kind === "object");
  if (!template || template.kind !== "object") throw Error("Missing object template");
  const frame: ObjectDefinition = {
    ...structuredClone(template),
    id: "timber-frame",
    name: "Timber frame",
    material: "bark",
    dependencies: [],
    assembly: createDoorwayAssembly(width, height, 0.25),
  };
  project.documents.push(frame);
  project.entry = frame.id;
  return parseProject(project);
}
