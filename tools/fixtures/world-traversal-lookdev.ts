import { type Camera, parseProject, sampleWorldPolyline, type Vec3 } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import { compositionInterests } from "@wrela/world";
import type { WorldTraversalStudy } from "../world-traversal-study";

const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** Keep the real host, physics world, caches and renderer alive along the whole
 * trajectory. Capture uses the displayed canvas, preserving interactive LOD history. */
export async function createWorldTraversalFixture(input: WorldTraversalStudy) {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw new Error("Traversal requires a canvas");
  const host = new BrowserSceneHost(parseProject(input.project), { maxInstalledBytes: 192 * 1024 * 1024 });
  host.setViewportHeight(canvas.clientHeight || canvas.height);
  host.updateView(input.cameras[0]);
  await host.prepare(input.subject, undefined, "review");
  if (!host.world || !host.runtime) throw new Error("Traversal requires a live world and physics");
  const world = host.world,
    runtime = host.runtime;
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    antialiasing: "spatial",
  });
  const stepCount = Math.ceil(input.duration * 60);
  const journey = sampleWorldPolyline(input.points, stepCount + 1);
  let currentStep = 0,
    currentFrame = -1;
  let physicalInterest: Vec3 = [...input.points[0]];
  world.setInterest({
    id: "player",
    position: physicalInterest,
    visualRadius: 48,
    collisionRadius: 24,
    priority: 2,
  });
  const ready = await world.prepare();
  if (!ready.ready) throw new Error(`Initial traversal residency failed: ${ready.missing.join(", ")}`);
  host.advance(1 / 60);
  const allProbes: ReturnType<typeof probe>[] = [];
  const completeness = () => ({
    complete: renderer.completeness.complete,
    rendered: renderer.completeness.rendered.length,
    culled: renderer.completeness.culled.length,
    uploading: [...renderer.completeness.uploading],
    rejected: structuredClone(renderer.completeness.rejected),
  });

  function probe(point: Vec3, time: number) {
    const ground = world.queryGround(point[0], point[2]);
    if (ground.status !== "ready")
      return {
        position: point,
        time,
        ready: false as const,
        reason: ground.reason,
        blockedBy: [] as string[],
      };
    const position: Vec3 = [point[0], ground.height, point[2]];
    const radius = input.actor.radius;
    const levels = [
      input.actor.maxStepHeight + radius + 0.02,
      input.actor.height / 2,
      input.actor.height - radius,
    ];
    const blockedBy = [
      ...new Set(
        levels.flatMap((level) =>
          runtime.physics.overlapSphere([position[0], position[1] + level, position[2]], radius),
        ),
      ),
    ];
    const ray = runtime.physics.raycast([position[0], position[1] + 2, position[2]], [0, -1, 0], 4);
    if (ray && ray.point[1] - ground.height > input.actor.maxStepHeight && !blockedBy.includes(ray.id))
      blockedBy.push(ray.id);
    return {
      position,
      time,
      ready: true as const,
      blockedBy,
      groundHeight: ground.height,
      groundSpacing: ground.sampleSpacing,
      collisionRay: ray,
      heightError: Math.abs(ground.analyticHeight - ground.height),
    };
  }

  return {
    async frame(index: number) {
      if (index !== currentFrame + 1 || index >= input.cameras.length)
        throw new RangeError("Traversal frames must run in order");
      const lastStep = Math.round((index * stepCount) / (input.cameras.length - 1));
      const probes: ReturnType<typeof probe>[] = [];
      const authoredCamera = input.cameras[index];
      while (currentStep < lastStep) {
        currentStep++;
        const point = journey[currentStep].position;
        const next = journey[Math.min(stepCount, currentStep + 12)].position;
        if (Math.hypot(point[0] - physicalInterest[0], point[2] - physicalInterest[2]) >= 6) {
          physicalInterest = [...point];
          world.setInterest({
            id: "player",
            position: point,
            visualRadius: 48,
            collisionRadius: 24,
            priority: 2,
          });
          world.update();
        }
        host.advance(1 / 60, {
          position: [point[0], point[1] + 1.65, point[2]],
          target: [next[0], next[1] + 1.6, next[2]],
          fov: 62,
        });
        probes.push(probe(point, currentStep / 60));
      }
      if (!probes.length) probes.push(probe(journey[currentStep].position, currentStep / 60));
      allProbes.push(...probes);
      const ground = world.queryGround(authoredCamera.position[0], authoredCamera.position[2]);
      const camera: Camera = structuredClone(authoredCamera);
      if (ground.status === "ready") camera.position[1] = ground.height + 1.65;
      host.updateView(camera);
      const scene = host.extract(camera);
      scene.grid = false;
      renderer.render(scene);
      await renderer.flushGpuTimings();
      currentFrame = index;
      return {
        id: `frame-${String(index).padStart(3, "0")}`,
        index,
        time: currentStep / 60,
        simulationTime: runtime.clock.time,
        camera,
        groundReady: probes.every((probe) => probe.ready && probe.collisionRay !== null),
        blockedBy: [...new Set(probes.flatMap((probe) => probe.blockedBy))],
        probes,
        initialComplete: renderer.completeness.complete,
        initialStreamingReady: !world.metrics.pending && !world.metrics.collisionPending,
        completeness: completeness(),
        streamingBefore: world.metrics,
        initialMeasurements: { ...renderer.measurements },
      };
    },
    async settle() {
      const started = performance.now();
      const readiness = await world.prepare({ timeoutMs: 10000 });
      host.advance(0);
      const camera = structuredClone(input.cameras[currentFrame]);
      const ground = world.queryGround(camera.position[0], camera.position[2]);
      if (ground.status === "ready") camera.position[1] = ground.height + 1.65;
      let attempts = 0;
      for (; attempts < 12; attempts++) {
        const scene = host.extract(camera);
        scene.grid = false;
        renderer.render(scene);
        await renderer.flushGpuTimings();
        if (renderer.completeness.complete) break;
        await nextFrame();
      }
      return {
        complete: renderer.completeness.complete,
        streamingReady: readiness.ready,
        streamingZoneActive: compositionInterests(world.world.composition, camera.position).some(
          (interest) => interest.id === `composition:${input.streamingRegion}`,
        ),
        settleMs: performance.now() - started,
        uploadFrames: attempts,
        completeness: completeness(),
        streamingAfter: world.metrics,
        measurements: { ...renderer.measurements },
        resources: host.resourceUsage,
        diagnostics: [...host.diagnostics, ...renderer.diagnostics],
      };
    },
    probes() {
      return allProbes;
    },
    async contactSheet(frames: { id: string; image: string }[]) {
      const sheet = document.createElement("canvas");
      sheet.width = 1280;
      sheet.height = 520;
      const context = sheet.getContext("2d");
      if (!context) throw new Error("Contact sheet requires a 2D canvas");
      context.fillStyle = "#131818";
      context.fillRect(0, 0, sheet.width, sheet.height);
      for (const [index, frame] of frames.entries()) {
        const picture = await new Promise<HTMLImageElement>((resolve, reject) => {
          const image = new Image();
          image.onload = () => resolve(image);
          image.onerror = reject;
          image.src = frame.image;
        });
        const x = (index % 4) * 320,
          y = Math.floor(index / 4) * 260;
        context.drawImage(picture, x, y, 320, 240);
        context.fillStyle = "#ecede9";
        context.font = "12px sans-serif";
        context.fillText(frame.id, x + 8, y + 255);
      }
      return sheet.toDataURL("image/png");
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
