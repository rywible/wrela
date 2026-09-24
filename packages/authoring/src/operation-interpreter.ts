import { contentKey, type Document, type Project, references } from "@wrela/model";
import type { Operation } from "./commands";
import { applyCreatureOperation } from "./creature";
import type { CreatureOperation } from "./creature-commands";

const clone = <T>(value: T): T => structuredClone(value);
/** Optional source properties need an explicit domain path; absent arbitrary
 * object keys remain rejected before schema parsing could silently strip them. */
function optionalProperty(document: Document, path: (string | number)[]): boolean {
  if (
    document.kind === "object" &&
    path.length === 4 &&
    path[0] === "assembly" &&
    path[1] === "parts" &&
    typeof path[2] === "number" &&
    ["grainOffset", "edgeWear", "endBevel", "shell"].includes(String(path[3]))
  )
    return true;
  if (path.length === 1) {
    const extensions: Partial<Record<Document["kind"], readonly string[]>> = {
      object: ["assembly"],
      character: ["performance"],
      vegetation: ["botanical"],
      terrain: ["geology"],
      world: ["composition"],
      material: ["appearance"],
      environment: ["sequence", "grade", "cloudscape", "cloudCover", "dayCycle"],
      lighting: ["zones"],
      water: ["flow"],
      stage: ["grade"],
    };
    if (extensions[document.kind]?.includes(String(path[0]))) return true;
    if (document.kind === "world" && path[0] === "water") return true;
    if (
      document.kind === "material" &&
      ["domain", "layers", "creature", "emission"].includes(String(path[0]))
    )
      return true;
    if (document.kind === "object" && path[0] === "colliders") return true;
  }
  if (document.kind === "environment") {
    const weatherPath =
      path[0] === "sequence" && path[1] === "keyframes" && typeof path[2] === "number" && path[3] === "state"
        ? path.slice(4)
        : path;
    if (weatherPath.length === 1 && ["cloudscape", "cloudCover"].includes(String(weatherPath[0])))
      return true;
    if (
      weatherPath.length === 2 &&
      weatherPath[0] === "cloudscape" &&
      [
        "background",
        "formations",
        "front",
        "highCloudYaw",
        "highCloudHeight",
        "midCloudCover",
        "midCloudHeight",
        "midCloudYaw",
      ].includes(String(weatherPath[1]))
    )
      return true;
    if (
      weatherPath.length === 4 &&
      weatherPath[0] === "cloudscape" &&
      weatherPath[1] === "formations" &&
      typeof weatherPath[2] === "number" &&
      ["maturity", "shear"].includes(String(weatherPath[3]))
    )
      return true;
  }
  if (document.kind === "character" && path.length === 2 && path[0] === "physics" && path[1] === "colliders")
    return true;
  if ("field" in document && path[0] === "field") {
    if (path[1] === "fidelity")
      return (
        path.length === 2 ||
        (path.length === 3 && ["maxError", "minimumFeatureSize", "strict"].includes(String(path[2])))
      );
    if (path.length === 4 && path[1] === "nodes" && typeof path[2] === "number" && path[3] === "material")
      return true;
  }
  return false;
}
export function applyOperation(project: Project, op: Operation, reads?: Set<string>) {
  if (op.kind === "document.create") {
    if (project.documents.some((d) => d.id === op.document.id))
      throw Error("A definition with that identity already exists");
    project.documents.push(op.document);
    return;
  }
  const doc = project.documents.find((d) => d.id === op.target);
  if (!doc) throw Error(`Definition ${op.target} does not exist`);
  if (doc.generated?.policy === "locked" && op.kind !== "document.detach")
    throw Error("Detach the generated definition before editing it");
  const previousTyped = new Set(references({ ...doc, dependencies: [] }));
  if (op.kind.startsWith("creature.")) {
    applyCreatureOperation(doc, op as CreatureOperation);
  } else
    switch (op.kind) {
      case "document.detach":
        if (doc.generated) doc.generated.policy = "detached";
        break;
      case "document.delete":
        if (project.entry === doc.id) throw Error("The project entry cannot be deleted");
        project.documents = project.documents.filter((d) => d.id !== doc.id);
        break;
      case "document.rename":
        doc.name = op.name;
        break;
      case "document.set": {
        if (["id", "kind", "schemaVersion", "generated", "dependencies"].includes(String(op.path[0])))
          throw Error("Identity and source policy require semantic operations");
        let cursor: any = doc;
        for (const p of op.path.slice(0, -1)) {
          if (["__proto__", "prototype", "constructor"].includes(String(p)) || !Object.hasOwn(cursor, p))
            throw Error("Unknown property path");
          cursor = cursor[p];
          if (cursor === null || typeof cursor !== "object") throw Error("Invalid property path");
        }
        const last = op.path[op.path.length - 1];
        if (last === "id") throw Error("Stable identities cannot be rewritten through property edits");
        const optional = optionalProperty(doc, op.path);
        if (
          ["__proto__", "prototype", "constructor"].includes(String(last)) ||
          (!Object.hasOwn(cursor, last) && !optional)
        )
          throw Error("Unknown property");
        cursor[last] = clone(op.value);
        break;
      }
      case "field.update": {
        if (!("field" in doc)) throw Error("Select a field definition");
        const node = doc.field.nodes.find((n) => n.id === op.node);
        if (!node) throw Error("Shape does not exist");
        Object.assign(node, op.changes);
        break;
      }
      case "field.add": {
        if (!("field" in doc)) throw Error("Select a field definition");
        if (doc.field.nodes.some((n) => n.id === op.node.id)) throw Error("Shape identity already exists");
        const root = doc.field.nodes.find((n) => n.id === doc.field.root);
        if (!root) throw new Error("Field root is missing");
        doc.field.nodes.push(op.node);
        if (["union", "smoothUnion"].includes(root.kind)) root.children.push(op.node.id);
        else {
          const base = `composition-${contentKey([doc.id, op.node.id, root.id]).slice(0, 12)}`;
          let id = base,
            suffix = 0;
          while (doc.field.nodes.some((node) => node.id === id)) id = `${base}-${++suffix}`;
          doc.field.nodes.push({
            ...clone(root),
            id,
            name: "Composition",
            kind: "smoothUnion",
            children: [root.id, op.node.id],
            position: [0, 0, 0],
            rotation: [0, 0, 0],
            size: [1, 1, 1],
            material: undefined,
          });
          doc.field.root = id;
        }
        break;
      }
      case "field.remove": {
        if (!("field" in doc)) throw Error("Select a field definition");
        if (doc.field.root === op.node) throw Error("The root shape cannot be deleted");
        if (!doc.field.nodes.some((node) => node.id === op.node)) throw Error("Shape does not exist");
        const removed = new Set([op.node]);
        let changed = true;
        while (changed) {
          changed = false;
          for (const node of doc.field.nodes) {
            node.children = node.children.filter((child) => !removed.has(child));
            if (
              ["union", "subtract", "intersect", "smoothUnion"].includes(node.kind) &&
              node.children.length === 0 &&
              !removed.has(node.id)
            ) {
              removed.add(node.id);
              changed = true;
            }
          }
        }
        if (removed.has(doc.field.root)) throw Error("A field must retain at least one shape");
        const remaining = new Map(
          doc.field.nodes.filter((node) => !removed.has(node.id)).map((node) => [node.id, node]),
        );
        const reachable = new Set<string>();
        const visit = (id: string) => {
          if (reachable.has(id)) return;
          reachable.add(id);
          for (const child of remaining.get(id)?.children ?? []) visit(child);
        };
        visit(doc.field.root);
        // Unary compositions retain their transform and material. Collapsing them
        // would move a surviving shape or alter its material, especially in DAGs.
        doc.field.nodes = doc.field.nodes.filter((node) => reachable.has(node.id));
        break;
      }
      case "material.assign":
        if (!("material" in doc)) throw Error("This definition has no surface material");
        doc.material = op.material;
        break;
      case "material.makeLocal": {
        if (!("material" in doc)) throw Error("This definition has no surface material");
        reads?.add(doc.material);
        const material = project.documents.find((d) => d.id === doc.material);
        if (!material || material.kind !== "material") throw Error("Material is missing");
        if (project.documents.some((d) => d.id === op.newId)) throw Error("Variant identity already exists");
        const variant = {
          ...clone(material),
          id: op.newId,
          name: `${material.name} · ${doc.name}`.slice(0, 120),
        };
        if (variant.generated) variant.generated.policy = "detached";
        project.documents.push(variant);
        doc.material = op.newId;
        break;
      }
      case "terrain.intervene":
        if (doc.kind !== "terrain") throw Error("Select terrain");
        {
          const index = doc.interventions.findIndex((i) => i.id === op.intervention.id);
          if (index < 0) doc.interventions.push(op.intervention);
          else doc.interventions[index] = op.intervention;
        }
        break;
      case "terrain.widenValley":
        if (doc.kind !== "terrain") throw Error("Select terrain");
        {
          const i = doc.interventions.find((i) => i.id === op.intervention);
          if (!i || i.kind !== "valley") throw Error("Valley intervention is missing");
          i.radius = op.width / 2;
        }
        break;
      case "world.placeForest":
        if (doc.kind !== "world") throw Error("Select a world");
        {
          const rule = {
            id: op.rule,
            definition: op.definition,
            spacing: op.spacing,
            density: op.density,
            seed: op.seed,
            minHeight: -0.4,
            maxHeight: 100,
            maxSlope: 0.8,
          };
          const index = doc.populations.findIndex((r) => r.id === op.rule);
          if (index < 0) doc.populations.push(rule);
          else doc.populations[index] = rule;
        }
        break;
      case "character.addJoint":
        if (doc.kind !== "character") throw Error("Select a character");
        doc.joints.push(op.joint);
        break;
      case "character.setJointLimit":
      case "character.pose": {
        if (doc.kind !== "character") throw Error("Select a character");
        const joint = doc.joints.find((j) => j.id === op.joint);
        if (!joint) throw Error("Joint is missing");
        if (op.kind === "character.pose") joint.rotation = op.rotation;
        else {
          joint.minimum = op.minimum;
          joint.maximum = op.maximum;
        }
        break;
      }
      case "character.addKey": {
        if (doc.kind !== "character") throw Error("Select a character");
        const motion = doc.motions.find((m) => m.id === op.motion);
        if (!motion) throw Error("Motion is missing");
        motion.keys = motion.keys.filter((k) => !(k.joint === op.joint && k.time === op.time));
        motion.keys.push({
          joint: op.joint,
          time: op.time,
          rotation: op.rotation,
          translation: op.translation,
        });
        motion.keys.sort((a, b) => a.time - b.time || a.joint.localeCompare(b.joint));
        break;
      }
    }
  // Preserve additional declared dependencies while removing obsolete copies
  // of references owned by a typed property (e.g. a replaced material).
  const nextTyped = new Set(references({ ...doc, dependencies: [] }));
  doc.dependencies = doc.dependencies.filter((id) => !previousTyped.has(id) || nextTyped.has(id));
}
/** One synchronous authority shared by human controls and agent clients.
 * Unchanged definitions retain identity; history only retains changed definitions. */
