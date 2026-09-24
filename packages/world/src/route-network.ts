import { type Vec3, type WorldComposition, worldPathPolyline } from "@wrela/model";

export type WorldNetworkReview = {
  entryPath?: string;
  reachablePaths: string[];
  disconnectedPaths: string[];
  connections: { from: string; to: string; position: Vec3; traversable: boolean }[];
  destinations: {
    id: string;
    kind: "room" | "landmark" | "encounter";
    position: Vec3;
    routeIds: string[];
    reachable: boolean;
    reason?: "no-route" | "disconnected-route" | "blocked-entrance" | "narrow-entrance";
  }[];
};
type Route = { id: string; blockedSamples: number };
type Clearance = (position: Vec3) => boolean;
const lerp = (a: Vec3, b: Vec3, t: number): Vec3 => a.map((value, i) => value + (b[i] - value) * t) as Vec3;
function nearest(point: Vec3, a: Vec3, b: Vec3): Vec3 {
  const dx = b[0] - a[0],
    dz = b[2] - a[2];
  const t = Math.max(
    0,
    Math.min(1, ((point[0] - a[0]) * dx + (point[2] - a[2]) * dz) / (dx * dx + dz * dz || 1)),
  );
  return lerp(a, b, t);
}
const separation = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[2] - b[2]);
function routeApproach(points: readonly Vec3[], target: Vec3): Vec3 {
  let closest = points[0],
    distance = Infinity;
  for (let i = 1; i < points.length; i++) {
    const p = nearest(target, points[i - 1], points[i]),
      d = separation(p, target);
    if (d < distance) {
      closest = p;
      distance = d;
    }
  }
  return closest;
}
/** Find an actual intersection or a short gap between route centerlines. AABB
 * rejection keeps distant segments cheap; interior crossings do not depend on sampling phase. */
function junction(a: readonly Vec3[], b: readonly Vec3[], tolerance: number): Vec3 | undefined {
  for (let i = 1; i < a.length; i++)
    for (let j = 1; j < b.length; j++) {
      const p = a[i - 1],
        q = a[i],
        r = b[j - 1],
        s = b[j];
      if (
        Math.max(p[0], q[0]) + tolerance < Math.min(r[0], s[0]) ||
        Math.max(r[0], s[0]) + tolerance < Math.min(p[0], q[0]) ||
        Math.max(p[2], q[2]) + tolerance < Math.min(r[2], s[2]) ||
        Math.max(r[2], s[2]) + tolerance < Math.min(p[2], q[2])
      )
        continue;
      const dx = q[0] - p[0],
        dz = q[2] - p[2],
        ex = s[0] - r[0],
        ez = s[2] - r[2];
      const cross = dx * ez - dz * ex;
      if (Math.abs(cross) > 1e-9) {
        const t = ((r[0] - p[0]) * ez - (r[2] - p[2]) * ex) / cross;
        const u = ((r[0] - p[0]) * dz - (r[2] - p[2]) * dx) / cross;
        if (t >= 0 && t <= 1 && u >= 0 && u <= 1) return lerp(p, q, t);
      }
      for (const [point, start, end] of [
        [p, r, s],
        [q, r, s],
        [r, p, q],
        [s, p, q],
      ]) {
        const other = nearest(point, start, end);
        if (separation(point, other) <= tolerance) return lerp(point, other, 0.5);
      }
    }
}

/** A conservative route graph for authored corridors, not a free-space navmesh.
 * A blocked route is unavailable as a whole. Door approaches additionally inspect
 * the final connector, preventing a path at the back wall from counting as access. */
export function reviewRouteNetwork(
  composition: WorldComposition,
  routes: readonly Route[],
  actorRadius: number,
  clear: Clearance,
): WorldNetworkReview {
  const paths = [...composition.paths].sort((a, b) => a.id.localeCompare(b.id));
  const geometry = new Map(paths.map((path) => [path.id, worldPathPolyline(path)]));
  const usable = new Map(routes.map((route) => [route.id, route.blockedSamples === 0]));
  const connections: WorldNetworkReview["connections"] = [];
  for (let i = 0; i < paths.length; i++)
    for (let j = i + 1; j < paths.length; j++) {
      const from = paths[i].id,
        to = paths[j].id;
      const position = junction(geometry.get(from) ?? [], geometry.get(to) ?? [], actorRadius * 2);
      if (position)
        connections.push({
          from,
          to,
          position,
          traversable: !!usable.get(from) && !!usable.get(to) && clear(position),
        });
    }
  const entryPath = composition.review?.entryPath ?? paths[0]?.id;
  const reached = new Set<string>();
  if (entryPath && usable.get(entryPath)) reached.add(entryPath);
  let changed = true;
  while (changed) {
    changed = false;
    for (const link of connections) {
      if (!link.traversable || (!reached.has(link.from) && !reached.has(link.to))) continue;
      if (!reached.has(link.from) || !reached.has(link.to)) changed = true;
      reached.add(link.from);
      reached.add(link.to);
    }
  }
  const destinations: WorldNetworkReview["destinations"] = [];
  for (const space of composition.spaces) {
    const routeIds = paths
      .filter(
        (path) =>
          separation(routeApproach(geometry.get(path.id) ?? [], space.center), space.center) <= space.radius,
      )
      .map((path) => path.id);
    const reachable = routeIds.some((id) => reached.has(id));
    destinations.push({
      id: space.id,
      kind: space.kind,
      position: space.center,
      routeIds,
      reachable,
      reason: reachable ? undefined : routeIds.length ? "disconnected-route" : "no-route",
    });
  }
  for (const room of composition.rooms) {
    const local: Vec3 =
      room.doorSide === "north"
        ? [0, 0, -room.size[1] / 2]
        : room.doorSide === "south"
          ? [0, 0, room.size[1] / 2]
          : room.doorSide === "east"
            ? [room.size[0] / 2, 0, 0]
            : [-room.size[0] / 2, 0, 0];
    const c = Math.cos(room.yaw),
      s = Math.sin(room.yaw);
    const position: Vec3 = [
      room.center[0] + c * local[0] + s * local[2],
      room.center[1],
      room.center[2] - s * local[0] + c * local[2],
    ];
    const approaches = paths
      .map((path) => ({ path, point: routeApproach(geometry.get(path.id) ?? [], position) }))
      .filter(({ path, point }) => separation(point, position) <= path.width / 2 + actorRadius);
    const routeIds = approaches.map(({ path }) => path.id);
    const narrow = room.doorWidth < actorRadius * 2;
    const clearApproaches = clear(position)
      ? approaches.filter(({ point }) => {
          const count = Math.max(1, Math.ceil(separation(point, position) / Math.max(0.1, actorRadius)));
          return Array.from({ length: count + 1 }, (_, i) => clear(lerp(point, position, i / count))).every(
            Boolean,
          );
        })
      : [];
    const entranceClear = clearApproaches.length > 0;
    const reachable = !narrow && clearApproaches.some(({ path }) => reached.has(path.id));
    destinations.push({
      id: room.id,
      kind: "room",
      position,
      routeIds,
      reachable,
      reason: reachable
        ? undefined
        : narrow
          ? "narrow-entrance"
          : !routeIds.length
            ? "no-route"
            : !entranceClear
              ? "blocked-entrance"
              : "disconnected-route",
    });
  }
  return {
    entryPath,
    reachablePaths: [...reached].sort(),
    disconnectedPaths: paths.filter((path) => !reached.has(path.id)).map((path) => path.id),
    connections,
    destinations,
  };
}
