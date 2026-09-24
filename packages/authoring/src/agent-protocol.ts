import { z } from "zod";
import { authoringResult } from "./errors";

const requestSchema = z.strictObject({
  method: z.string().min(1).max(120),
  args: z.array(z.unknown()).max(8).default([]),
});
/** Only explicitly installed own methods are callable; object/prototype traversal is forbidden. */
export function executeAgentRequest(api: object, input: unknown) {
  return authoringResult(async () => {
    const request = requestSchema.parse(input),
      path = request.method.split(".");
    let owner: unknown = api,
      value: unknown = api;
    for (const key of path) {
      if (
        ["__proto__", "prototype", "constructor", "execute"].includes(key) ||
        !value ||
        typeof value !== "object" ||
        !Object.hasOwn(value, key)
      )
        throw Error("Unknown agent method; discover the API first");
      owner = value;
      value = (value as Record<string, unknown>)[key];
    }
    if (typeof value !== "function") throw Error("Agent method is not callable");
    return Reflect.apply(value, owner, request.args);
  });
}
