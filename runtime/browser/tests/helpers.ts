// Small helpers shared by the tests.

import { readFile } from "node:fs/promises";
import { join } from "node:path";

/** What `body` throws; fails the test if it returns. */
export function thrown(body: () => unknown): unknown {
  try {
    body();
  } catch (e) {
    return e;
  }
  throw new Error("expected it to throw, but it returned");
}

/** What `body` rejects with; fails the test if it resolves. */
export async function rejected(body: () => Promise<unknown>): Promise<unknown> {
  try {
    await body();
  } catch (e) {
    return e;
  }
  throw new Error("expected it to reject, but it resolved");
}

/** The package's root, runtime/browser. */
export const PACKAGE = join(import.meta.dir, "..");

/** The first-light fixture's directory. */
export const FIXTURE = join(PACKAGE, "tests/first-light-fixture");

/** runtime/command-stream.md, the contract. */
export function contract(): Promise<string> {
  return readFile(join(PACKAGE, "../command-stream.md"), "utf8");
}

/** The ```json block of runtime/command-stream.md: first light's manifest. */
export async function contractManifest(): Promise<string> {
  const text = await contract();
  const start = text.indexOf("```json\n") + "```json\n".length;
  return text.slice(start, text.indexOf("```", start));
}
