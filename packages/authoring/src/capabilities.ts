import type { Document, Project } from "@wrela/model";
import { commandDescriptions, type Operation, operationSchema } from "./commands";
import { applyOperation } from "./operation-interpreter";

const targets: Record<string, readonly Document["kind"][]> = {
  creature: ["character"],
  field: ["object", "character"],
  material: ["object", "character", "terrain", "vegetation"],
  terrain: ["terrain"],
  world: ["world"],
  character: ["character"],
};
/** One catalog supplies schema, target applicability and effects for discovery and execution. */
export const authoringCapabilities = operationSchema.options.map((schema) => {
  const kind = schema.shape.kind.value;
  return {
    kind,
    schema,
    description: commandDescriptions[kind],
    execute(project: Project, operation: Operation, reads?: Set<string>) {
      if (operation.kind !== kind) throw Error("Operation does not match its capability");
      if (operation.kind !== "document.create") {
        const document = project.documents.find((d) => d.id === operation.target);
        if (document) assertCapabilityTarget(operation, document);
      }
      return applyOperation(project, operation, reads);
    },
    targetKinds: targets[kind.split(".")[0]] ?? null,
    effects:
      kind === "document.create"
        ? ["create"]
        : kind === "document.delete"
          ? ["delete"]
          : kind === "material.makeLocal"
            ? ["create", "update"]
            : ["update"],
  };
});
export function capabilityApplies(operation: string, document: Document) {
  const capability = authoringCapabilities.find((c) => c.kind === operation);
  return !!capability && (!capability.targetKinds || capability.targetKinds.includes(document.kind));
}
export function assertCapabilityTarget(operation: Operation, document: Document) {
  if (!capabilityApplies(operation.kind, document))
    throw Object.assign(Error(`${operation.kind} cannot edit a ${document.kind} definition`), {
      code: "authoring.inapplicable-operation",
      document: document.id,
      path: "target",
      nextAction: `Discover operations for ${document.id}.`,
    });
}

export function executeOperation(project: Project, operation: Operation, reads?: Set<string>) {
  const capability = authoringCapabilities.find((c) => c.kind === operation.kind);
  if (!capability) throw Error("Unknown authoring operation");
  return capability.execute(project, operation, reads);
}
