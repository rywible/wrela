import { cross, dot, type MeshData, sub, type Vec3 } from "@wrela/model";

type Face = { a: Vec3; ab: Vec3; ac: Vec3; min: Vec3; max: Vec3; center: Vec3 };
type Node = { min: Vec3; max: Vec3; faces?: Face[]; children?: [Node, Node] };
function build(faces: Face[]): Node {
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const face of faces)
    for (let axis = 0; axis < 3; axis++) {
      min[axis] = Math.min(min[axis], face.min[axis]);
      max[axis] = Math.max(max[axis], face.max[axis]);
    }
  if (faces.length <= 8) return { min, max, faces };
  const axis = max
    .map((value, index) => value - min[index])
    .reduce((largest, value, index, spans) => (value > spans[largest] ? index : largest), 0);
  faces.sort((a, b) => a.center[axis] - b.center[axis]);
  const middle = Math.floor(faces.length / 2);
  return { min, max, children: [build(faces.slice(0, middle)), build(faces.slice(middle))] };
}
/** Nearest opposing source face along the actual inward displacement direction. */
export function sourceThicknessQuery(mesh: MeshData): (position: Vec3, inward: Vec3) => number {
  const point = (index: number): Vec3 => [...mesh.positions.subarray(index * 3, index * 3 + 3)] as Vec3;
  const faces: Face[] = [];
  for (let i = 0; i < mesh.indices.length; i += 3) {
    const points = [point(mesh.indices[i]), point(mesh.indices[i + 1]), point(mesh.indices[i + 2])];
    faces.push({
      a: points[0],
      ab: sub(points[1], points[0]),
      ac: sub(points[2], points[0]),
      min: [0, 1, 2].map((axis) => Math.min(...points.map((p) => p[axis]))) as Vec3,
      max: [0, 1, 2].map((axis) => Math.max(...points.map((p) => p[axis]))) as Vec3,
      center: [0, 1, 2].map((axis) => points.reduce((sum, p) => sum + p[axis], 0) / 3) as Vec3,
    });
  }
  const root = build(faces);
  return (position, direction) => {
    let nearest = Infinity;
    const visit = (node: Node) => {
      let near = 0,
        far = nearest;
      for (let axis = 0; axis < 3; axis++) {
        if (Math.abs(direction[axis]) < 1e-12) {
          if (position[axis] < node.min[axis] - 1e-9 || position[axis] > node.max[axis] + 1e-9) return;
        } else {
          const a = (node.min[axis] - position[axis]) / direction[axis],
            b = (node.max[axis] - position[axis]) / direction[axis];
          near = Math.max(near, Math.min(a, b));
          far = Math.min(far, Math.max(a, b));
          if (near > far + 1e-9) return;
        }
      }
      if (node.children) {
        visit(node.children[0]);
        visit(node.children[1]);
        return;
      }
      for (const face of node.faces ?? []) {
        const p = cross(direction, face.ac),
          determinant = dot(face.ab, p);
        if (Math.abs(determinant) < 1e-14) continue;
        const offset = sub(position, face.a),
          u = dot(offset, p) / determinant;
        if (u < -1e-8 || u > 1 + 1e-8) continue;
        const q = cross(offset, face.ab),
          v = dot(direction, q) / determinant;
        if (v < -1e-8 || u + v > 1 + 1e-8) continue;
        const distance = dot(face.ac, q) / determinant;
        if (distance > 1e-8 && distance < nearest) nearest = distance;
      }
    };
    visit(root);
    return nearest;
  };
}
