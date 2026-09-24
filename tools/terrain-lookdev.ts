import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { buildGeologyObjects, reviewGeology } from "@wrela/compiler";
import { createEnvironmentLookdev } from "@wrela/examples/environment-lookdev";
import { createLookdevMaterials } from "@wrela/examples/material-lookdev";
import { createGeologyAuthoringStudy, createTerrainLookdev } from "@wrela/examples/terrain-lookdev";
import { type Camera, parseProject, type WorldDefinition } from "@wrela/model";
import { fixtureServer, waitFor, withBrowser } from "./browser";

const revision = process.argv.find((arg) => arg.startsWith("--revision="))?.slice(11) ?? "v1";
if (!/^[a-z0-9-]+$/.test(revision)) throw Error("Use a simple revision name");
const root = resolve("output/terrain-lookdev", revision);
await mkdir(root, { recursive: true });
const terrain = createTerrainLookdev(),
  environment = createEnvironmentLookdev();
const world: WorldDefinition = {
  id: "terrain-lookdev-world",
  name: "Alpine landform study",
  schemaVersion: 1,
  kind: "world",
  dependencies: [],
  generatorVersion: "wrela-world-1",
  terrain: terrain.terrain,
  environment: environment.environment,
  lighting: environment.lighting,
  water: environment.water,
  populations: [],
  instances: terrain.placements,
};
const studyTerrain = createGeologyAuthoringStudy();
const studyFormations = buildGeologyObjects(studyTerrain);
if (!studyTerrain.geology) throw new Error("Study geology missing");
const studyPositions = studyTerrain.geology.formations.map((formation) => formation.position);
const studyWorld: WorldDefinition = {
  ...world,
  id: "geology-study-world",
  name: "Geology authoring tool study",
  terrain: studyTerrain.id,
  water: undefined,
  instances: studyFormations.map((object, index) => ({
    id: `placed-${object.id}`,
    definition: object.id,
    position: studyPositions[index],
    rotation: [0, 0, 0],
    scale: 1,
  })),
};
const heldOut = (["cross-bedded", "confined"] as const).map((variant) => {
  const source = createGeologyAuthoringStudy(variant);
  const objects = buildGeologyObjects(source);
  const positions = source.geology?.formations.map((formation) => formation.position) ?? [];
  const heldOutWorld: WorldDefinition = {
    ...studyWorld,
    id: `geology-${variant}-world`,
    name: `Held-out geology: ${variant}`,
    terrain: source.id,
    instances: objects.map((object, index) => ({
      id: `placed-${object.id}`,
      definition: object.id,
      position: positions[index],
      rotation: [0, 0, 0],
      scale: 1,
    })),
  };
  return { variant, source, objects, world: heldOutWorld };
});
const project = parseProject({
  schemaVersion: 1,
  id: "terrain-lookdev",
  name: "Alpine landform study",
  documents: [
    ...createLookdevMaterials(),
    ...terrain.documents,
    ...environment.documents,
    world,
    studyTerrain,
    ...studyFormations,
    studyWorld,
    ...heldOut.flatMap((entry) => [entry.source, ...entry.objects, entry.world]),
  ],
  entry: world.id,
});
const cameras: Record<string, Camera> = {
  landscape: { position: [14, 7, -24], target: [-6, 2, 4], fov: 48 },
  gameplay: { position: [-4, 2.15, -15], target: [-8, 2, 10], fov: 59 },
  bedrock: { position: [-16, 5.5, 8], target: [-20, 6.5, 18], fov: 49 },
};
const studyCameras: Record<string, Camera> = {
  formations: { position: [15, 8, -22], target: [-1, 3.7, 1], fov: 48 },
  passage: { position: [-6.8, 2.2, -10], target: [-5, 2.1, 6], fov: 58 },
  protectedTrail: { position: [-16, 13, 28], target: [-5, 1, 12], fov: 52 },
};
await Bun.write(resolve(root, "source.json"), JSON.stringify(project, null, 2));
await Bun.write(resolve(root, "traversal.json"), JSON.stringify(reviewGeology(studyTerrain), null, 2));
await Bun.write(
  resolve(root, "held-out-traversal.json"),
  JSON.stringify(
    heldOut.map((entry) => ({ variant: entry.variant, review: reviewGeology(entry.source) })),
    null,
    2,
  ),
);
await withBrowser(
  async (view, _output, errors) => {
    const server = await fixtureServer(
      `import {createTerrainLookdevRenderer} from ${JSON.stringify(resolve("tools/fixtures/terrain-lookdev-render.ts"))};createTerrainLookdevRenderer(${JSON.stringify(project)},${JSON.stringify(world.id)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      root,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const reports: unknown[] = [];
      for (const [worldId, views] of [
        [world.id, cameras],
        [studyWorld.id, studyCameras],
        ...heldOut.map(
          (entry) =>
            [
              entry.world.id,
              {
                [`${entry.variant}-formations`]: studyCameras.formations,
                [`${entry.variant}-passage`]: studyCameras.passage,
              },
            ] as const,
        ),
      ] as const) {
        await view.evaluate(`fixture.prepare(${JSON.stringify(worldId)})`);
        for (const [name, camera] of Object.entries(views)) {
          const capture = await view.evaluate<{
            image: string;
            completeness: { complete: boolean };
            measurements: { adapter: string };
          }>(`fixture.capture(${JSON.stringify(camera)})`);
          const { image, ...report } = capture;
          await Bun.write(resolve(root, `${name}.png`), Buffer.from(image.split(",")[1], "base64"));
          reports.push({ name, ...report });
          if (!capture.completeness.complete) throw Error("Incomplete capture");
          if (/swiftshader|software|llvmpipe/i.test(capture.measurements.adapter))
            throw Error("Hardware rendering required");
        }
      }
      // A normal view separates geometry/collision defects from a dark floor
      // caused by lighting. It uses the exact same authored passage and camera.
      await view.evaluate(`fixture.prepare(${JSON.stringify(studyWorld.id)})`);
      const normalCapture = await view.evaluate<{ image: string; completeness: { complete: boolean } }>(
        `fixture.capture(${JSON.stringify(studyCameras.passage)},"normals")`,
      );
      const { image: normalImage, ...normalReport } = normalCapture;
      await Bun.write(resolve(root, "passage-normals.png"), Buffer.from(normalImage.split(",")[1], "base64"));
      reports.push({ name: "passage-normals", ...normalReport });
      if (!normalCapture.completeness.complete) throw Error("Incomplete passage normal capture");
      await Bun.write(resolve(root, "capture.json"), JSON.stringify({ revision, reports, errors }, null, 2));
      console.log(JSON.stringify({ root, errors }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  960,
  640,
);
