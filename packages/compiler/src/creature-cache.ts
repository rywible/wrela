import { ProductCache } from "./cache";

const MB = 1024 * 1024;
const budgets = {
  attachments: 2 * MB,
  features: 12 * MB,
  body: 16 * MB,
  appearance: 24 * MB,
  groom: 24 * MB,
  binding: 8 * MB,
  correctives: 8 * MB,
  assembly: 24 * MB,
  anchors: 2 * MB,
  projection: 16 * MB,
} as const;
export type CreaturePreparationProduct = keyof typeof budgets;
const caches = Object.fromEntries(
  Object.entries(budgets).map(([kind, bytes]) => [kind, new ProductCache<unknown>(bytes, 24)]),
) as Record<CreaturePreparationProduct, ProductCache<unknown>>;
const builds = Object.fromEntries(Object.keys(budgets).map((kind) => [kind, 0])) as Record<
  CreaturePreparationProduct,
  number
>;

/** Conservative owned-size accounting includes JS provenance objects and strings,
 * not just GPU-ready arrays. Shared buffers are counted once per product. */
function ownedBytes(value: unknown, seen = new Set<object>()): number {
  if (typeof value === "string") return 24 + value.length * 2;
  if (typeof value === "number" || typeof value === "boolean") return 8;
  if (value === null || value === undefined) return 0;
  if (typeof value !== "object" || seen.has(value)) return 0;
  seen.add(value);
  if (value instanceof ArrayBuffer) return value.byteLength + 64;
  if (ArrayBuffer.isView(value)) return 64 + ownedBytes(value.buffer, seen);
  if (Array.isArray(value))
    return 64 + value.length * 8 + value.reduce((sum, item) => sum + ownedBytes(item, seen), 0);
  return (
    64 +
    Object.entries(value).reduce((sum, [key, item]) => sum + 24 + key.length * 2 + ownedBytes(item, seen), 0)
  );
}

/** The cache never shares mutable arrays with builders or callers. Byte budgets
 * and LRU entry limits apply independently so groom work cannot evict body work. */
export function cachedCreatureProduct<T>(kind: CreaturePreparationProduct, key: string, build: () => T): T {
  const cache = caches[kind],
    existing = cache.get(key);
  if (existing !== undefined) return structuredClone(existing) as T;
  const value = build();
  builds[kind]++;
  cache.set(key, structuredClone(value), ownedBytes(value));
  return value;
}
export function creaturePreparationCacheMetrics() {
  return Object.fromEntries(
    Object.keys(budgets).map((key) => {
      const kind = key as CreaturePreparationProduct;
      return [
        kind,
        { ...caches[kind].metrics, builds: builds[kind], maxBytes: budgets[kind], maxEntries: 24 },
      ];
    }),
  ) as Record<
    CreaturePreparationProduct,
    {
      entries: number;
      bytes: number;
      hits: number;
      misses: number;
      builds: number;
      maxBytes: number;
      maxEntries: number;
    }
  >;
}
export function clearCreaturePreparationCaches() {
  for (const kind of Object.keys(budgets) as CreaturePreparationProduct[]) {
    caches[kind].clear();
    builds[kind] = 0;
  }
}
