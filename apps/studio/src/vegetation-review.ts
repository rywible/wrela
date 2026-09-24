import {
  type CompiledVegetation,
  type MeshData,
  type ThinCoverage,
  type Vec3,
  type VegetationDefinition,
  type VegetationMotionWeights,
  type VegetationStand,
  vegetationMotionOffset,
  vegetationWindResponse,
} from "@wrela/model";

export type VegetationReviewOptions = {
  distance: number;
  stand: boolean;
  angle: number;
  time: number;
  silhouette: boolean;
};
export type ReviewTriangle = {
  points: [number, number][];
  depth: number;
  color: string;
  coverage?: ThinCoverage;
  uv?: [number, number][];
};
/** The same bounded horizontal bend used by scene.wgsl, at a five m/s review wind. */
export function vegetationReviewPosition(
  position: Vec3,
  response: number,
  time: number,
  offset: Vec3,
  weights?: VegetationMotionWeights,
  instanceScale = 1,
): Vec3 {
  const phase = time * 1.4 + (position[0] + offset[0]) * 0.17 + (position[2] + offset[2]) * 0.23;
  const bend =
    Math.min(Math.max(position[1] / instanceScale, 0) ** 2 * 0.012, 0.6) *
    response *
    0.5 *
    Math.sin(phase) *
    instanceScale;
  const world: Vec3 = [position[0] + offset[0] + bend, position[1] + offset[1], position[2] + offset[2]];
  const motion = weights
    ? vegetationMotionOffset(weights, world, time, [5, 0, 0], response, 0, instanceScale)
    : [0, 0, 0];
  return [world[0] + motion[0], world[1] + motion[1], world[2] + motion[2]];
}
export function vegetationReviewTriangles(
  doc: VegetationDefinition,
  artifact: CompiledVegetation,
  options: VegetationReviewOptions,
  width: number,
  height: number,
  stand?: VegetationStand & { artifacts: Map<string, CompiledVegetation> },
): ReviewTriangle[] {
  const count = options.stand ? (doc.botanical?.review.standCount ?? 9) : 1;
  const spacing = doc.botanical?.review.spacing ?? doc.radius * 2;
  const columns = Math.ceil(Math.sqrt(count));
  const targetY = (artifact.bounds.max[1] + artifact.bounds.min[1]) * 0.5;
  const focal = height / (2 * Math.tan(Math.PI / 6));
  const cosine = Math.cos(options.angle),
    sine = Math.sin(options.angle);
  const triangles: ReviewTriangle[] = [];
  for (let instance = 0; instance < count; instance++) {
    const member = options.stand ? stand?.instances[instance] : undefined;
    const memberDocument = stand?.documents.find((document) => document.id === member?.definition) ?? doc;
    const memberArtifact = member ? (stand?.artifacts.get(member.definition) ?? artifact) : artifact;
    const offset: Vec3 =
      member?.position ??
      (options.stand
        ? [
            ((instance % columns) - (columns - 1) * 0.5) * spacing,
            0,
            Math.floor(instance / columns) * spacing,
          ]
        : [0, 0, 0]);
    const yaw = member?.rotation?.[1] ?? 0,
      instanceScale = member?.scale ?? 1;
    for (let surfaceIndex = 0; surfaceIndex < memberArtifact.surfaces.length; surfaceIndex++) {
      const surface = memberArtifact.surfaces[surfaceIndex];
      const mesh: MeshData =
        options.distance > 25 ? (surface.details?.[0]?.mesh ?? surface.mesh) : surface.mesh;
      const points: { x: number; y: number; depth: number }[] = [];
      for (let i = 0; i < mesh.positions.length; i += 3) {
        const weights = mesh.wind?.subarray((i / 3) * 4, (i / 3) * 4 + 4);
        const p = vegetationReviewPosition(
          [
            (mesh.positions[i] * Math.cos(yaw) + mesh.positions[i + 2] * Math.sin(yaw)) * instanceScale,
            mesh.positions[i + 1] * instanceScale,
            (-mesh.positions[i] * Math.sin(yaw) + mesh.positions[i + 2] * Math.cos(yaw)) * instanceScale,
          ],
          vegetationWindResponse(memberDocument),
          options.time,
          offset,
          weights ? [weights[0], weights[1], weights[2], weights[3]] : undefined,
          instanceScale,
        );
        const x = p[0] * cosine - p[2] * sine;
        const depth = options.distance + p[0] * sine + p[2] * cosine;
        points.push({
          x: width / 2 + (x * focal) / depth,
          y: height / 2 - ((p[1] - targetY) * focal) / depth,
          depth,
        });
      }
      for (let index = 0; index < mesh.indices.length; index += 3) {
        const ids = [mesh.indices[index], mesh.indices[index + 1], mesh.indices[index + 2]];
        const vertices = ids.map((id) => points[id]);
        if (vertices.some((p) => p.depth < 0.1)) continue;
        const normal = mesh.normals[ids[0] * 3 + 1];
        const tint = (mesh.colors?.[ids[0] * 3] ?? 1) * (0.72 + 0.28 * Math.abs(normal));
        const rgb = surfaceIndex === 0 ? [113, 81, 57] : [70, 117, 60];
        const coverage = mesh.thinCoverage;
        triangles.push({
          points: vertices.map((p) => [p.x, p.y]),
          depth: vertices.reduce((sum, p) => sum + p.depth, 0) / 3,
          color: options.silhouette ? "#182522" : `rgb(${rgb.map((c) => Math.round(c * tint)).join(",")})`,
          ...(coverage
            ? {
                coverage,
                uv: ids.map((id) => [coverage.uv[id * 2], coverage.uv[id * 2 + 1]] as [number, number]),
              }
            : {}),
        });
      }
    }
  }
  return triangles.sort((a, b) => b.depth - a.depth);
}
