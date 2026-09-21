export const CURRENT_SCHEMA_VERSION = 1;
export type MigrationReport = {
  from: number;
  to: typeof CURRENT_SCHEMA_VERSION;
  steps: string[];
  source: unknown;
};
/** The only accepted legacy layout is version 0: the same semantic document
 * vocabulary before per-document schema/dependency metadata was mandatory.
 * Unsupported future formats are rejected, never coerced or stripped. */
export function migrateProjectSource(input: unknown): MigrationReport {
  if (!input || typeof input !== "object" || Array.isArray(input))
    throw new Error("Project source must be an object");
  const source = input as Record<string, unknown>,
    version = source.schemaVersion;
  if (version === CURRENT_SCHEMA_VERSION)
    return { from: version, to: CURRENT_SCHEMA_VERSION, steps: [], source: input };
  if (version !== 0)
    throw new Error(
      `Unsupported project schema version ${String(version)}; supported versions are 0 and ${CURRENT_SCHEMA_VERSION}`,
    );
  if (!Array.isArray(source.documents)) throw new Error("Legacy project documents must be an array");
  const migrated = structuredClone(source);
  migrated.schemaVersion = CURRENT_SCHEMA_VERSION;
  migrated.documents = source.documents.map((inputDocument: unknown) => {
    if (!inputDocument || typeof inputDocument !== "object" || Array.isArray(inputDocument))
      throw new Error("Malformed legacy document");
    const document = structuredClone(inputDocument) as Record<string, unknown>;
    if (
      document.schemaVersion !== undefined &&
      document.schemaVersion !== 0 &&
      document.schemaVersion !== CURRENT_SCHEMA_VERSION
    )
      throw new Error(`Unsupported legacy document schema version ${String(document.schemaVersion)}`);
    document.schemaVersion = CURRENT_SCHEMA_VERSION;
    if (document.dependencies === undefined) document.dependencies = [];
    return document;
  });
  return {
    from: 0,
    to: CURRENT_SCHEMA_VERSION,
    steps: ["v0 → v1: add project/document schema metadata and explicit dependency lists"],
    source: migrated,
  };
}
