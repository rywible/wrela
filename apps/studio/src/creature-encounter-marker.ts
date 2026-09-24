import { type MeshData, type RenderSurface, transformMatrix, type Vec3 } from "@wrela/model";

/** A deliberately simple player proxy keeps the creature, contacts and attack timing legible. */
const markerMesh: MeshData = (() => {
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [];
  const rings = [
    [0.16, 0.05],
    [0.3, 0.3],
    [0.15, 0.9],
    [0.02, 1.15],
  ];
  for (const [radius, y] of rings)
    for (let side = 0; side < 12; side++) {
      const angle = (side * Math.PI) / 6;
      positions.push(Math.cos(angle) * radius, y, Math.sin(angle) * radius);
      normals.push(Math.cos(angle), 0.2, Math.sin(angle));
    }
  for (let ring = 0; ring < rings.length - 1; ring++)
    for (let side = 0; side < 12; side++) {
      const a = ring * 12 + side,
        b = ring * 12 + ((side + 1) % 12),
        c = a + 12,
        d = b + 12;
      indices.push(a, c, b, b, c, d);
    }
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    indices: new Uint32Array(indices),
    bounds: { min: [-0.3, 0, -0.3], max: [0.3, 1.15, 0.3] },
  };
})();

export function creaturePlayerMarker(position: Vec3, dodging: boolean): RenderSurface {
  return {
    id: "creature-encounter-player",
    source: "creature-encounter-player",
    mesh: markerMesh,
    matrix: transformMatrix(position, dodging ? 0.65 : 1),
    material: {
      color: dodging ? [0.95, 0.85, 0.45] : [0.25, 0.85, 0.95],
      secondary: [0.08, 0.2, 0.25],
      roughness: 0.3,
      metallic: 0.2,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
  };
}
