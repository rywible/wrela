import type { Diagnostic } from "./contracts";
import {
  type Document,
  documentSchema,
  type Project,
  type RecipeDefinition,
  type RecipeInstance,
  recipeInstanceSchema,
  recipeSchema,
} from "./documents";
import { contentKey } from "./math";

/** Recipes are bounded numeric parameter bindings, not executable imported code.
 * Field targets use stable node IDs. Realization always returns ordinary editable
 * source; importing a project never silently regenerates an author's edits. */
export function instantiateRecipe(input: RecipeDefinition, requested: RecipeInstance): Document {
  const recipe = recipeSchema.parse(input),
    instance = recipeInstanceSchema.parse(requested);
  if (instance.recipe !== recipe.id || instance.version !== recipe.version)
    throw new Error(`Recipe ${instance.recipe} version ${instance.version} is unavailable`);
  const parameters: Record<string, number> = {};
  for (const id of Object.keys(instance.parameters ?? {}))
    if (!Object.hasOwn(recipe.parameters, id)) throw new Error(`Unknown recipe parameter ${id}`);
  for (const [id, range] of Object.entries(recipe.parameters)) {
    if (["__proto__", "prototype", "constructor"].includes(id))
      throw new Error("Unsafe recipe parameter identity");
    if (
      range.min > range.max ||
      range.default < range.min ||
      range.default > range.max ||
      (range.integer && !Number.isInteger(range.default))
    )
      throw new Error(`Invalid recipe parameter range ${id}`);
    const value = instance.parameters?.[id] ?? range.default;
    if (value < range.min || value > range.max || (range.integer && !Number.isInteger(value)))
      throw new Error(`Recipe parameter ${id} is outside its supported range`);
    parameters[id] = value;
  }
  const document = structuredClone(recipe.template),
    assigned = new Set<string>();
  function assign(target: { node?: string; path: (string | number)[] }, value: number | string | boolean) {
    if (
      target.path.some(
        (part) => typeof part === "string" && ["__proto__", "prototype", "constructor"].includes(part),
      )
    )
      throw new Error("Unsafe recipe path");
    if (["id", "kind", "schemaVersion", "generated", "dependencies"].includes(String(target.path[0])))
      throw new Error("Recipes cannot override stable identities or provenance");
    let owner: unknown = target.node
      ? document.kind === "object" || document.kind === "character"
        ? document.field.nodes.find((node) => node.id === target.node)
        : undefined
      : document;
    if (!owner) throw new Error(`Missing recipe node ${target.node}`);
    for (const part of target.path.slice(0, -1)) {
      if (typeof owner !== "object" || owner === null || !Object.hasOwn(owner, part))
        throw new Error("Recipe path does not exist");
      owner = (owner as Record<string | number, unknown>)[part];
    }
    const property = target.path[target.path.length - 1];
    if (typeof owner !== "object" || owner === null || !Object.hasOwn(owner, property))
      throw new Error("Recipe target does not exist");
    const record = owner as Record<string | number, unknown>;
    if (typeof record[property] !== typeof value)
      throw new Error("Recipe override must preserve the target type");
    record[property] = value;
  }
  for (const binding of recipe.bindings) {
    if (!Object.hasOwn(parameters, binding.parameter))
      throw new Error(`Missing recipe parameter ${binding.parameter}`);
    const target = contentKey({ node: binding.node, path: binding.path });
    if (assigned.has(target)) throw new Error("Recipe bindings cannot write the same target twice");
    assigned.add(target);
    const value = parameters[binding.parameter] * (binding.scale ?? 1) + (binding.offset ?? 0);
    if (!Number.isFinite(value)) throw new Error("Recipe binding produced a non-finite number");
    assign(binding, value);
  }
  for (const override of instance.overrides ?? []) assign(override, override.value);
  document.id = instance.id;
  document.name = instance.name;
  document.generated = {
    generator: "wrela-recipe-1",
    policy: "detached",
    recipe: {
      id: recipe.id,
      version: recipe.version,
      key: contentKey(recipe),
      parameters,
      overrides: structuredClone(instance.overrides ?? []),
    },
  };
  return documentSchema.parse(document);
}

export function instantiateRecipes(recipes: RecipeDefinition[], instances: RecipeInstance[]): Document[] {
  const registry = new Map(recipes.map((recipe) => [recipe.id, recipe]));
  if (
    registry.size !== recipes.length ||
    new Set(instances.map((instance) => instance.id)).size !== instances.length
  )
    throw new Error("Recipe and instance identities must be unique");
  return instances.map((instance) => {
    const recipe = registry.get(instance.recipe);
    if (!recipe) throw new Error(`Missing recipe ${instance.recipe}`);
    return instantiateRecipe(recipe, instance);
  });
}
/** Explicit regeneration refuses to replace edited source by default. Callers
 * must deliberately opt into replacing documents when applying recipe changes. */
export function realizeRecipeProject(project: Project, options: { replace?: boolean } = {}): Project {
  const generated = instantiateRecipes(project.recipes ?? [], project.recipeInstances ?? []);
  const documents = new Map(project.documents.map((document) => [document.id, document]));
  for (const document of generated) {
    const existing = documents.get(document.id);
    if (existing && contentKey(existing) !== contentKey(document) && !options.replace)
      throw new Error(`Recipe instance ${document.id} has existing source; explicit replacement is required`);
    documents.set(document.id, document);
  }
  return { ...project, documents: [...documents.values()] };
}
export function validateRecipeReferences(project: Project): Diagnostic[] {
  try {
    for (const recipe of project.recipes ?? [])
      instantiateRecipe(recipe, {
        id: recipe.template.id,
        name: recipe.template.name,
        recipe: recipe.id,
        version: recipe.version,
      });
    instantiateRecipes(project.recipes ?? [], project.recipeInstances ?? []);
    const documents = new Set(project.documents.map((document) => document.id));
    for (const instance of project.recipeInstances ?? [])
      if (!documents.has(instance.id))
        throw new Error(`Recipe instance ${instance.id} has no realized source document`);
    return [];
  } catch (error) {
    return [
      { severity: "error", code: "recipe", message: error instanceof Error ? error.message : String(error) },
    ];
  }
}
