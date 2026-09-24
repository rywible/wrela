import type { Vec3 } from "./math";

type RoomLayout = {
  size: [number, number];
  moduleWidth: number;
  doorSide: "north" | "east" | "south" | "west";
  doorWidth: number;
};
export type WorldRoomModule = { side: number; index: number; position: Vec3; yaw: number };

/** Fit complete wall modules between the exact corners and doorway edges. On a
 * fractional span the interior modules overlap; no module crosses the door gap
 * or changes its height. Validation rejects modules wider than a doorway pier. */
export function worldRoomModules(room: RoomLayout): WorldRoomModule[] {
  const modules: WorldRoomModule[] = [];
  const sides = ["north", "east", "south", "west"];
  for (let side = 0; side < 4; side++) {
    const length = room.size[side % 2];
    const intervals =
      sides[side] === room.doorSide
        ? [
            [-length / 2, -room.doorWidth / 2],
            [room.doorWidth / 2, length / 2],
          ]
        : [[-length / 2, length / 2]];
    let index = 0;
    for (const [start, end] of intervals) {
      const span = end - start;
      if (span + 1e-8 < room.moduleWidth)
        throw new RangeError("Wall modules must fit between the doorway and each corner");
      const count = Math.max(1, Math.ceil(span / room.moduleWidth));
      if (modules.length + count > 256) throw new RangeError("Room exceeds the 256 module sampling budget");
      for (let i = 0; i < count; i++) {
        const along =
          count === 1
            ? (start + end) / 2
            : start + room.moduleWidth / 2 + (i * (span - room.moduleWidth)) / (count - 1);
        const position: Vec3 =
          side === 0
            ? [along, 0, -room.size[1] / 2]
            : side === 1
              ? [room.size[0] / 2, 0, along]
              : side === 2
                ? [-along, 0, room.size[1] / 2]
                : [-room.size[0] / 2, 0, -along];
        modules.push({ side, index: index++, position, yaw: (side * Math.PI) / 2 });
      }
    }
  }
  return modules;
}
