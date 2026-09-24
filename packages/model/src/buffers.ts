/** Shared ownership traversal for compiler transfer and runtime residency. Typed views
 * are leaves; aliases share ownership of their backing allocation. */
export function ownedBufferViews(value: unknown): { path: (string | number)[]; view: ArrayBufferView }[] {
  const result: { path: (string | number)[]; view: ArrayBufferView }[] = [],
    seen = new Set<object>();
  const visit = (entry: unknown, path: (string | number)[]) => {
    if (!entry || typeof entry !== "object" || seen.has(entry)) return;
    seen.add(entry);
    if (ArrayBuffer.isView(entry)) {
      result.push({ path, view: entry });
      return;
    }
    for (const [key, child] of Object.entries(entry))
      visit(child, [...path, Array.isArray(entry) ? Number(key) : key]);
  };
  visit(value, []);
  return result;
}
export function ownedBuffers(value: unknown): ArrayBuffer[] {
  return [...new Set(ownedBufferViews(value).map(({ view }) => view.buffer))].map((buffer) => {
    if (!(buffer instanceof ArrayBuffer)) throw Error("Compiler products must own transferable ArrayBuffers");
    return buffer;
  });
}
export function serializeBufferViews(value: unknown): unknown {
  if (ArrayBuffer.isView(value)) {
    if (value instanceof DataView) throw Error("DataView is not an authored numeric product");
    return Array.from(value as unknown as ArrayLike<number>);
  }
  if (Array.isArray(value)) return value.map(serializeBufferViews);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [key, serializeBufferViews(child)]),
    );
  return value;
}
