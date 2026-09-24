/** Schema defaults may add fields; authored input must never silently lose one. */
export class SourceContractError extends Error {
  readonly code = "authoring.unknown-property";
  readonly nextAction =
    "Retrieve the schema for this operation or definition and correct the indicated property.";
  constructor(
    readonly path: string,
    message = `Unknown authored property: ${path}`,
  ) {
    super(message);
  }
}
export function assertSourceKeysPreserved(source: unknown, parsed: unknown, path = "source"): void {
  if (source === null || typeof source !== "object") return;
  if (parsed === null || typeof parsed !== "object")
    throw new SourceContractError(path, `Unsupported authored value at ${path}`);
  for (const [key, value] of Object.entries(source)) {
    if (value === undefined) continue;
    if (!Object.hasOwn(parsed, key)) throw new SourceContractError(`${path}.${key}`);
    assertSourceKeysPreserved(value, (parsed as Record<string, unknown>)[key], `${path}.${key}`);
  }
}
