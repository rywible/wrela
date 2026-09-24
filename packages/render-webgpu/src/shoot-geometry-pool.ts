import { contentKey, type MeshData } from "@wrela/model";

/** Identical immutable GPU streams share storage even after worker/cook cloning.
 * Bounds, source identities and occurrence transforms remain on each surface. */
export class ShootGeometryPool {
  private aliases = new WeakMap<MeshData, MeshData>();
  private templates = new Map<string, MeshData[]>();
  canonical(mesh: MeshData): MeshData {
    if (!mesh.shoots) return mesh;
    const found = this.aliases.get(mesh);
    if (found) return found;
    const arrays = (m: MeshData) => [
      m.positions,
      m.normals,
      m.indices,
      m.colors,
      m.wind,
      m.materialCoordinates,
      m.reliefCoordinates,
      m.reliefNormals,
      m.thinCoverage?.uv,
      m.thinCoverage?.layer,
    ];
    const streams = arrays(mesh),
      key = contentKey(streams);
    const same = (candidate: MeshData) =>
      arrays(candidate).every((a, i) => {
        const other = streams[i];
        return (
          a === other || (!!a && !!other && a.length === other.length && a.every((v, j) => v === other[j]))
        );
      });
    const bucket = this.templates.get(key) ?? [],
      existing = bucket.find(same);
    const canonical = existing ?? {
      ...mesh,
      shoots: undefined,
      sourceIds: undefined,
      thinCoverage: mesh.thinCoverage ? { ...mesh.thinCoverage, levels: [] } : undefined,
      bounds: mesh.shoots.templateBounds,
    };
    if (!existing) {
      bucket.push(canonical);
      this.templates.set(key, bucket);
    }
    this.aliases.set(mesh, canonical);
    // Registry ownership is bounded independently of GPU residency. Weak aliases
    // keep active stream identities stable after an old key leaves the registry.
    while (this.templates.size > 128) {
      const oldest = this.templates.keys().next().value;
      if (oldest === undefined) break;
      this.templates.delete(oldest);
    }
    return canonical;
  }
  clear() {
    this.aliases = new WeakMap();
    this.templates.clear();
  }
}
