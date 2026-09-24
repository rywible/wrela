import {
  type Project,
  type Vec3,
  type WorldComposition,
  type WorldDefinition,
  worldPathPolyline,
} from "@wrela/model";

import { defaultWorldReview, realizeWorldComposition, reviewWorldComposition } from "@wrela/world";
import { useMemo, useState } from "react";

type Handle = {
  id: string;
  label: string;
  point: Vec3;
  write: (source: WorldComposition, point: Vec3) => void;
};
/** An editable plan view shares the same source transaction as numeric controls. */
export function WorldSpatialPanel({
  world,
  project,
  composition,
  onChange,
}: {
  world: WorldDefinition;
  project: Project;
  composition: WorldComposition;
  onChange: (composition: WorldComposition, label: string) => void;
}) {
  const [selectedId, setSelectedId] = useState("");
  const terrain = project.documents.find(
    (document) => document.id === world.terrain && document.kind === "terrain",
  );
  const review = useMemo(() => {
    if (terrain?.kind !== "terrain") return undefined;
    try {
      return {
        result: reviewWorldComposition(world, terrain, project.documents),
        instances: realizeWorldComposition(world, terrain).world.instances,
      };
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) };
    }
  }, [world, terrain, project.documents]);
  const handles: Handle[] = [
    ...composition.paths.flatMap((path, i) =>
      path.points.map((point, j) => ({
        id: `path:${path.id}:${j}`,
        label: `${path.id} point ${j + 1}`,
        point,
        write: (source: WorldComposition, value: Vec3) => {
          source.paths[i].points[j] = value;
        },
      })),
    ),
    ...composition.biomes.map((biome, i) => ({
      id: `biome:${biome.id}`,
      label: `Biome ${biome.id}`,
      point: biome.center,
      write: (source: WorldComposition, value: Vec3) => {
        source.biomes[i].center = value;
      },
    })),
    ...composition.rooms.map((room, i) => ({
      id: `room:${room.id}`,
      label: `Room ${room.id}`,
      point: room.center,
      write: (source: WorldComposition, value: Vec3) => {
        source.rooms[i].center = value;
      },
    })),
    ...composition.spaces.map((space, i) => ({
      id: `space:${space.id}`,
      label: `${space.kind} ${space.id}`,
      point: space.center,
      write: (source: WorldComposition, value: Vec3) => {
        source.spaces[i].center = value;
      },
    })),
    ...composition.streaming.map((region, i) => ({
      id: `streaming:${region.id}`,
      label: `Streaming ${region.id}`,
      point: region.center,
      write: (source: WorldComposition, value: Vec3) => {
        source.streaming[i].center = value;
      },
    })),
    ...composition.placements.map((placement, i) => ({
      id: `placement:${placement.id}`,
      label: `Assembly ${placement.id}`,
      point: placement.position,
      write: (source: WorldComposition, value: Vec3) => {
        source.placements[i].position = value;
      },
    })),
  ];
  const selected = handles.find((handle) => handle.id === selectedId) ?? handles[0];
  const points = [
    ...handles.map((handle) => handle.point),
    ...[...composition.biomes, ...composition.spaces, ...composition.streaming].flatMap((zone) => [
      [zone.center[0] - zone.radius, 0, zone.center[2] - zone.radius],
      [zone.center[0] + zone.radius, 0, zone.center[2] + zone.radius],
    ]),
    ...composition.rooms.flatMap((room) => {
      const radius = Math.hypot(...room.size) / 2;
      return [
        [room.center[0] - radius, 0, room.center[2] - radius],
        [room.center[0] + radius, 0, room.center[2] + radius],
      ];
    }),
  ];
  const minX = Math.min(-10, ...points.map((point) => point[0])) - 5,
    minZ = Math.min(-10, ...points.map((point) => point[2])) - 5;
  const size = Math.max(
    30,
    Math.max(10, ...points.map((point) => point[0])) - minX + 5,
    Math.max(10, ...points.map((point) => point[2])) - minZ + 5,
  );
  const move = (point: Vec3) => {
    if (!selected) return;
    const next = structuredClone(composition);
    selected.write(next, point);
    onChange(next, `Move ${selected.label}`);
  };
  const settings = composition.review ?? defaultWorldReview();
  const updateReview = (key: "actorRadius" | "actorHeight" | "maxStepHeight", value: number) => {
    if (Number.isFinite(value))
      onChange({ ...composition, review: { ...settings, [key]: value } }, "Edit traversal review");
  };
  return (
    <section aria-label="Spatial layout review">
      <h4>Layout map and clearance</h4>
      <p>
        Select a control point, then click the map to move it. Arrow keys move it one meter. Elevation remains
        unchanged.
      </p>
      <label className="row-control">
        Selected point{" "}
        <select value={selected?.id ?? ""} onChange={(event) => setSelectedId(event.target.value)}>
          {review?.result?.network.destinations.map((destination) => (
            <circle
              key={`${destination.kind}-${destination.id}`}
              cx={destination.position[0]}
              cy={destination.position[2]}
              r={size / 80}
              fill="none"
              stroke={destination.reachable ? "#92ca87" : "#ef7974"}
              strokeWidth={size / 300}
            />
          ))}
          {handles.map((handle) => (
            <option value={handle.id} key={handle.id}>
              {handle.label}
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        aria-label="Move selected point on world map"
        style={{ display: "block", width: "100%", padding: 0 }}
        onClick={(event) => {
          if (event.detail === 0) return;
          const bounds = event.currentTarget.getBoundingClientRect();
          move([
            minX + ((event.clientX - bounds.left) / bounds.width) * size,
            selected?.point[1] ?? 0,
            minZ + ((event.clientY - bounds.top) / bounds.height) * size,
          ]);
        }}
        onKeyDown={(event) => {
          if (!selected || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
          event.preventDefault();
          move([
            selected.point[0] + (event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0),
            selected.point[1],
            selected.point[2] + (event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0),
          ]);
        }}
      >
        <svg
          role="img"
          aria-label="World layout in the X Z plane"
          viewBox={`${minX} ${minZ} ${size} ${size}`}
          style={{ width: "100%", display: "block", background: "#151f24" }}
        >
          <title>World layout map</title>
          {composition.paths.map((path) => (
            <polyline
              key={path.id}
              points={worldPathPolyline(path)
                .map((point) => `${point[0]},${point[2]}`)
                .join(" ")}
              fill="none"
              stroke="#b9a786"
              strokeWidth={path.width}
              opacity="0.7"
            />
          ))}
          {composition.rooms.map((room) => (
            <rect
              key={room.id}
              x={room.center[0] - room.size[0] / 2}
              y={room.center[2] - room.size[1] / 2}
              width={room.size[0]}
              height={room.size[1]}
              transform={`rotate(${(-room.yaw * 180) / Math.PI} ${room.center[0]} ${room.center[2]})`}
              fill="none"
              stroke="#d9d0ac"
              strokeWidth={size / 300}
            />
          ))}
          {composition.biomes.map((biome) => (
            <circle
              key={biome.id}
              cx={biome.center[0]}
              cy={biome.center[2]}
              r={biome.radius}
              fill="#77b988"
              fillOpacity="0.08"
              stroke="#77b988"
              strokeWidth={size / 400}
            />
          ))}
          {[...composition.spaces, ...composition.streaming].map((region, index) => (
            <circle
              key={`${region.id}-${index}`}
              cx={region.center[0]}
              cy={region.center[2]}
              r={region.radius}
              fill="none"
              stroke={"kind" in region ? "#cfaa79" : "#759bbb"}
              strokeWidth={size / 300}
              strokeDasharray={"kind" in region ? undefined : `${size / 100}`}
            />
          ))}
          {review?.result?.routes.flatMap((route) =>
            route.samples
              .filter((sample) => !sample.traversable)
              .map((sample, index) => (
                <circle
                  key={`${route.id}-${index}`}
                  cx={sample.position[0]}
                  cy={sample.position[2]}
                  r={size / 150}
                  fill="#ef7974"
                />
              )),
          )}
          {review?.result?.network.destinations.map((destination) => (
            <circle
              key={`${destination.kind}-${destination.id}`}
              cx={destination.position[0]}
              cy={destination.position[2]}
              r={size / 80}
              fill="none"
              stroke={destination.reachable ? "#92ca87" : "#ef7974"}
              strokeWidth={size / 300}
            />
          ))}
          {handles.map((handle) => (
            <circle
              key={handle.id}
              cx={handle.point[0]}
              cy={handle.point[2]}
              r={size / (handle.id === selected?.id ? 70 : 110)}
              fill={handle.id === selected?.id ? "#f9da79" : "#85b6c8"}
            />
          ))}
          <line
            x1={settings.sightline.from[0]}
            y1={settings.sightline.from[2]}
            x2={settings.sightline.to[0]}
            y2={settings.sightline.to[2]}
            stroke={review?.result?.sightline.clear ? "#92ca87" : "#ef7974"}
            strokeWidth={size / 300}
          />
        </svg>
      </button>
      {selected && (
        <fieldset>
          <legend>{selected.label}</legend>
          {(["X", "Y", "Z"] as const).map((axis, index) => (
            <label className="row-control" key={axis}>
              {axis}{" "}
              <input
                type="number"
                step="0.25"
                value={selected.point[index]}
                onChange={(event) => {
                  if (!Number.isFinite(event.target.valueAsNumber)) return;
                  const point = [...selected.point] as Vec3;
                  point[index] = event.target.valueAsNumber;
                  move(point);
                }}
              />
            </label>
          ))}
        </fieldset>
      )}
      <label className="row-control">
        Entry route
        <select
          value={settings.entryPath ?? ""}
          onChange={(event) =>
            onChange(
              { ...composition, review: { ...settings, entryPath: event.target.value || undefined } },
              "Set entry route",
            )
          }
        >
          <option value="">First route by identity</option>
          {composition.paths.map((path) => (
            <option key={path.id} value={path.id}>
              {path.id}
            </option>
          ))}
        </select>
      </label>
      {(["actorRadius", "actorHeight", "maxStepHeight"] as const).map((key) => (
        <label className="row-control" key={key}>
          {key.replace(/([A-Z])/g, " $1")}{" "}
          <input
            type="number"
            step="0.05"
            min="0.01"
            value={settings[key]}
            onChange={(event) => updateReview(key, event.target.valueAsNumber)}
          />
        </label>
      ))}
      {(["from", "to"] as const).map((end) => (
        <fieldset key={end}>
          <legend>Sightline {end}</legend>
          {["X", "Y", "Z"].map((axis, i) => (
            <label className="row-control" key={axis}>
              {axis}{" "}
              <input
                type="number"
                step="0.25"
                value={settings.sightline[end][i]}
                onChange={(event) => {
                  if (!Number.isFinite(event.target.valueAsNumber)) return;
                  const next = structuredClone(settings);
                  next.sightline[end][i] = event.target.valueAsNumber;
                  onChange({ ...composition, review: next }, "Edit sightline");
                }}
              />
            </label>
          ))}
        </fieldset>
      ))}
      {review?.error && <output>{review.error}</output>}
      {review?.result && (
        <>
          <p>
            Review uses sampled terrain and conservative object bounds; hollow objects can produce false
            positives.
          </p>
          {review.result.routes.map((route) => (
            <p key={route.id}>
              {route.id}: {route.blockedSamples} blocked or steep samples; spacing{" "}
              {route.sampleSpacing.toFixed(2)} m{route.narrow ? "; narrower than the actor" : ""}.
            </p>
          ))}
          <p>
            From {review.result.network.entryPath ?? "no entry route"}:{" "}
            {review.result.network.reachablePaths.length} of {composition.paths.length} routes accessible.
          </p>
          {review.result.network.destinations.map((destination) => (
            <p key={`${destination.kind}-${destination.id}`}>
              {destination.kind} {destination.id}:{" "}
              {destination.reachable
                ? "connected to the entry route"
                : destination.reason?.replaceAll("-", " ")}
              .
            </p>
          ))}
          <p>
            Route access is conservative: a blocked route is unavailable as a whole. It does not establish
            free-space navigation.
          </p>
          <p>
            Sightline:{" "}
            {review.result.sightline.clear
              ? "clear at the sampled terrain resolution"
              : `blocked by ${review.result.sightline.obstruction?.id}`}
            .
          </p>
        </>
      )}
      <h4>Instance exceptions</h4>
      <p>Choose an instance to expose its position, rotation, scale, and removal controls below.</p>
      <select
        aria-label="Create an exception for an instance"
        value=""
        onChange={(event) => {
          const instance = review?.instances?.find((item) => item.id === event.target.value);
          if (!instance) return;
          if (composition.overrides.some((item) => item.id === instance.id)) return;
          onChange(
            {
              ...composition,
              overrides: [
                ...composition.overrides,
                {
                  id: instance.id,
                  position: instance.position,
                  yaw: instance.rotation[1],
                  scale: instance.scale,
                  removed: false,
                },
              ],
            },
            "Create instance exception",
          );
        }}
      >
        <option value="">Select an instance</option>
        {review?.instances?.map((instance) => (
          <option key={instance.id} value={instance.id}>
            {instance.id}
          </option>
        ))}
      </select>
    </section>
  );
}
