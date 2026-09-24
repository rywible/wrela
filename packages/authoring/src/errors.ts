import { z } from "zod";
/** Shared browser/CLI failure envelope. Stable codes and paths are the recovery API;
 * prose explains the failure but is never necessary to identify a conflict. */
export function authoringError(error: unknown) {
  const value = error && typeof error === "object" ? (error as Record<string, unknown>) : {};
  const issues =
    error instanceof z.ZodError
      ? error.issues.map((issue) => ({
          code: issue.code,
          path: issue.path,
          message: issue.message,
          ...("keys" in issue ? { keys: issue.keys } : {}),
        }))
      : undefined;
  return {
    ok: false as const,
    error: {
      code:
        typeof value.code === "string"
          ? value.code
          : issues
            ? "authoring.invalid-request"
            : "authoring.failed",
      message: error instanceof Error ? error.message : String(error),
      path: value.path ?? issues?.[0]?.path,
      document: value.document,
      conflicts: value.conflicts,
      issues,
      nextAction:
        value.nextAction ??
        (value.conflicts
          ? "Inspect the conflicting definitions and propose against their current identities."
          : "Inspect source and retrieve the operation schema before retrying."),
    },
  };
}
export async function authoringResult<T>(operation: () => T | Promise<T>) {
  try {
    return { ok: true as const, result: await operation() };
  } catch (error) {
    return authoringError(error);
  }
}
