import { MAX_WORLD_COORDINATE, type Vec3 } from "@wrela/model";

export { MAX_WORLD_COORDINATE } from "@wrela/model";
export const CELL_SIZE = 256;
export type WorldPosition = { cell: [number, number, number]; offset: Vec3 };
export function worldPosition(position: Vec3): WorldPosition {
  if (position.some((v) => !Number.isFinite(v) || Math.abs(v) > MAX_WORLD_COORDINATE))
    throw new RangeError("World coordinate outside supported ±1,000,000,000 metres");
  const cell = position.map((v) => Math.floor(v / CELL_SIZE)) as Vec3;
  return { cell, offset: position.map((v, i) => v - cell[i] * CELL_SIZE) as Vec3 };
}
export function absolutePosition(position: WorldPosition): Vec3 {
  return position.cell.map((v, i) => v * CELL_SIZE + position.offset[i]) as Vec3;
}
export function relativePosition(position: WorldPosition, origin: WorldPosition): Vec3 {
  return position.cell.map(
    (v, i) => (v - origin.cell[i]) * CELL_SIZE + position.offset[i] - origin.offset[i],
  ) as Vec3;
}
