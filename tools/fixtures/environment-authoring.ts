import { referenceProject } from "@wrela/examples";
import { type Camera, environmentStateSchema, type Vec3 } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";

/** Exercises the same authored document → scene host → water shader path as Studio. */
export async function environmentAuthoringFixture() {
  const project = referenceProject();
  const water = project.documents.find((d) => d.kind === "water");
  const environment = project.documents.find((d) => d.kind === "environment");
  const stage = project.documents.find((d) => d.kind === "stage");
  if (water?.kind !== "water" || environment?.kind !== "environment" || stage?.kind !== "stage")
    throw Error("Environment fixture documents missing");
  water.level = 0;
  water.waves = [
    { amplitude: 0.12, wavelength: 3, speed: 0.3, direction: 0.6, phase: 0 },
    { amplitude: 0.05, wavelength: 1.6, speed: 0.2, direction: -0.8, phase: 1 },
  ];
  water.flow = {
    velocity: [1.5, 0],
    river: {
      points: [
        { position: [-9, 0], width: 5, depth: 2 },
        { position: [0, 0], width: 5, depth: 2 },
        { position: [7, 4], width: 7, depth: 3 },
      ],
      shoreWidth: 1,
      foam: 0.8,
    },
  };
  stage.environment = environment.id;
  stage.ground = false;
  const state = environmentStateSchema.parse({
    sunElevation: 0.6,
    sunAzimuth: 0.5,
    turbidity: 2,
    fogDensity: 0.001,
    wind: [0.5, 0, 0],
    ambient: 0.45,
    sunIntensity: 2.5,
    cloudscape: {
      development: 0.8,
      storminess: 0.1,
      highCloudCover: 0.2,
      background: 0.4,
      formations: [
        {
          id: "rebase-tower",
          kind: "tower",
          center: [3500, 6500],
          base: 1200,
          size: [2200, 4000, 1800],
          yaw: 0.3,
          density: 1,
          erosion: 0.35,
          seed: 31,
        },
      ],
    },
  });
  environment.sequence = {
    duration: 10,
    loop: false,
    interpolation: "linear",
    keyframes: [
      { time: 0, state },
      {
        time: 10,
        state: {
          ...state,
          sunElevation: 0.1,
          turbidity: 7,
          fogDensity: 0.01,
          wind: [3, 0, 1],
          wetness: 1,
          cloudCover: 0.8,
          grade: { exposureCompensation: 1, tint: [1, 0.8, 0.7] },
        },
      },
    ],
  };
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing environment canvas");
  const errors: string[] = [];
  const host = new BrowserSceneHost(project);
  await host.prepare(water.id, stage.id, "review");
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") errors.push(diagnostic.message);
    },
  });
  const camera: Camera = { position: [14, 14, 18], target: [0, 0, 1], fov: 45 };
  async function frame(time: number) {
    const scene = host.evaluate(time, camera);
    renderer.render(scene);
    const pixels = await linearImage(renderer);
    const image = await renderer.capture();
    const data = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(image);
    });
    return { pixels, image: data, scene };
  }
  return {
    async check() {
      const clear = await frame(0),
        storm = await frame(10);
      const delta = Math.sqrt(
        storm.pixels.reduce((sum, value, index) => sum + (value - clear.pixels[index]) ** 2, 0) /
          storm.pixels.length,
      );
      // Identical physical sky through a render-origin rebase must retain cloud
      // density, altitude, atmosphere transport and horizon without a seam.
      renderer.render({ ...storm.scene, surfaces: [] });
      const skyBeforeRebase = await linearImage(renderer);
      const shift: Vec3 = [256, 128, -512];
      const relative = (position: Vec3): Vec3 => position.map((value, axis) => value - shift[axis]) as Vec3;
      renderer.render({
        ...storm.scene,
        surfaces: [],
        origin: shift,
        camera: { ...camera, position: relative(camera.position), target: relative(camera.target) },
      });
      const skyAfterRebase = await linearImage(renderer);
      const rebaseDelta = Math.sqrt(
        skyBeforeRebase.reduce((sum, value, index) => sum + (value - skyAfterRebase[index]) ** 2, 0) /
          skyBeforeRebase.length,
      );
      const surface = storm.scene.surfaces.find((s) => s.water);
      return {
        delta,
        rebaseDelta,
        errors,
        complete: renderer.completeness.complete,
        adapter: renderer.measurements.adapter,
        clear: clear.image,
        storm: storm.image,
        wetness: storm.scene.environment.wetness,
        riverVertices: (surface?.mesh.positions.length ?? 0) / 3,
        resolvedFlow: surface?.water?.flow ?? null,
        exposure: storm.scene.environment.exposure,
      };
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
