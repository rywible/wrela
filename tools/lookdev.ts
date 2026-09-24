import { join, resolve } from "node:path";
import { referenceProject } from "@wrela/examples";
import type { Camera, Document, Project } from "@wrela/model";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLookdevFixture, LookdevStudy } from "./fixtures/lookdev";

const studyName = process.argv.find((a) => a.startsWith("--study="))?.split("=")[1] ?? "vegetation";
const composition = process.argv.find((a) => a.startsWith("--composition="))?.slice(14) ?? "primary";
if (!["primary", "river-bend"].includes(composition)) throw new Error("Unknown scene composition");
const size = process.argv.includes("--small") ? [640, 480] : [1024, 768];
const filter = process.argv.find((a) => a.startsWith("--frame="))?.split("=")[1];
const sampleTime = Number(process.argv.find((a) => a.startsWith("--time="))?.split("=")[1] ?? 0);
if (!Number.isFinite(sampleTime)) throw new Error("Lookdev time must be finite");
const base = (id: string, name: string) => ({ id, name, schemaVersion: 1 as const, dependencies: [] });
function install(project: Project, documents: Document[]) {
  const replacements = new Map(documents.map((d) => [d.id, d]));
  project.documents = [...project.documents.filter((d) => !replacements.has(d.id)), ...documents];
}
async function study(): Promise<LookdevStudy> {
  if (studyName === "scene") {
    const { createAlpineSliceStudy } = await import("./alpine-slice-study");
    const authored = createAlpineSliceStudy(composition as "primary" | "river-bend", "balanced");
    const project = authored.project;
    if (process.argv.includes("--omit-vegetation")) {
      const vegetation = new Set(
        project.documents.filter((document) => document.kind === "vegetation").map((document) => document.id),
      );
      const world = project.documents.find((document) => document.id === project.entry);
      if (world?.kind === "world")
        world.instances = world.instances.filter((instance) => !vegetation.has(instance.definition));
    }
    return {
      project,
      subject: project.entry,
      frames: authored.cameras.map((frame) => ({ ...frame, time: sampleTime })),
    };
  }
  const project = referenceProject();
  const stage = {
    ...base("lookdev-stage", "Neutral look development"),
    kind: "stage" as const,
    environment: "lookdev-sky",
    lighting: "lookdev-light",
    ground: true,
    exposure: 1,
    subjects: [] as string[],
    camera: { position: [8, 5, 14], target: [0, 4, 0], fov: 42 } as Camera,
  };
  install(project, [
    stage,
    {
      ...base("lookdev-sky", "Neutral daylight"),
      kind: "environment",
      model: "analytic-sky",
      sunElevation: 0.7,
      sunAzimuth: -1.3,
      turbidity: 2,
      fogDensity: 0,
      skyColor: [0.3, 0.44, 0.62],
      horizonColor: [0.72, 0.77, 0.8],
      groundColor: [0.2, 0.21, 0.2],
      wind: [2, 0, 0.5],
    },
    {
      ...base("lookdev-light", "Soft daylight"),
      kind: "lighting",
      ambient: 0.75,
      lights: [
        { id: "key", type: "directional", position: [-4, 8, 5], color: [1, 0.94, 0.84], intensity: 2.5 },
      ],
    },
  ]);
  if (studyName === "vegetation") {
    const { alpinePineLookdevDefinition } = await import("@wrela/examples/alpine-lookdev");
    const { alpineConiferMaterials } = await import("@wrela/model/vegetation-materials");
    const plant = alpinePineLookdevDefinition(),
      materials = alpineConiferMaterials();
    install(project, [plant, materials.needles, materials.bark]);
    return {
      project,
      subject: plant.id,
      stage: stage.id,
      frames: [
        { id: "plant-beauty", camera: stage.camera },
        { id: "plant-silhouette", camera: stage.camera, mode: "silhouette" },
        { id: "plant-detail", camera: { position: [2.8, 4.8, 4.8], target: [0, 4.1, 0], fov: 40 } },
        { id: "plant-gameplay", camera: { position: [14, 3.2, 20], target: [0, 4, 0], fov: 48 } },
        {
          id: "plant-backlit",
          camera: { position: [8, 5, 14], target: [0, 4, 0], fov: 42 },
          sunDirection: [-0.5, 0.22, -0.84],
        },
        { id: "plant-wind", camera: stage.camera, time: 1.5 },
      ],
    };
  }
  if (studyName === "architecture") {
    const { createArchitectureLookdev } = await import("@wrela/examples/architecture-lookdev");
    const { createLookdevMaterials } = await import("@wrela/examples/material-lookdev");
    const content = createArchitectureLookdev();
    const daylight = project.documents.find((d) => d.id === "lookdev-sky");
    if (daylight?.kind === "environment") daylight.sunAzimuth = -2.35;
    const light = project.documents.find((d) => d.id === "lookdev-light");
    if (light?.kind === "lighting") light.ambient = 1.55;
    install(project, [...createLookdevMaterials(), ...content.documents]);
    return {
      project,
      subject: content.object,
      stage: stage.id,
      frames: [
        { id: "architecture-beauty", camera: { position: [6, 4.2, -9], target: [0, 1.75, 0], fov: 38 } },
        { id: "architecture-detail", camera: { position: [3, 2.8, -4], target: [0.7, 2, 0], fov: 40 } },
        {
          id: "architecture-clay",
          camera: { position: [6, 4.2, -9], target: [0, 1.75, 0], fov: 38 },
          mode: "clay",
        },
        {
          id: "architecture-motion",
          camera: { position: [6, 4.2, -9], target: [0, 1.75, 0], fov: 38 },
          time: 1.5,
        },
      ],
    };
  }
  throw new Error(`Unknown look development study: ${studyName}`);
}
const input = await study();
if (filter) input.frames = input.frames.filter((frame) => frame.id === filter);
if (!input.frames.length) throw new Error("No matching review frames");
type FrameResult = Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>;
await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "authored-project.json"), JSON.stringify(input.project, null, 2));
    const server = await fixtureServer(
      `import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(input)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      const frames = [];
      for (let index = 0; index < input.frames.length; index++) {
        const { image, ...result } = await view.evaluate<FrameResult>(`fixture.frame(${index})`);
        if (/swiftshader|llvmpipe|software/i.test(result.measurements.adapter))
          throw new Error("Hardware adapter required");
        await Bun.write(join(output, `${result.id}.png`), Buffer.from(image.split(",")[1], "base64"));
        frames.push(result);
        console.log(JSON.stringify({ output, frame: result.id, triangles: result.triangles }));
      }
      if (errors.length) throw new Error(errors.join("\n"));
      await Bun.write(
        join(output, "lookdev.json"),
        JSON.stringify(
          { study: studyName, composition, frames, visualAcceptance: "requires-image-review" },
          null,
          2,
        ),
      );
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  size[0],
  size[1],
);
