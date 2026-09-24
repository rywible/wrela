import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { compileCharacter } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples";
import { createCharacterLookdev } from "@wrela/examples/character-lookdev";
import { type Camera, type Project, parseProject } from "@wrela/model";
import { auditCreatureMotion } from "@wrela/runtime";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import {
  CREATURE_FITTING_VARIANTS,
  type CreatureFittingVariant,
  createCreatureFittingLookdev,
} from "./fixtures/creature-fitting";

const baseline = process.argv.includes("--baseline");
const variant = process.argv.find((arg) => arg.startsWith("--variant="))?.slice(10) ?? "pilgrim";
if (!CREATURE_FITTING_VARIANTS.includes(variant as CreatureFittingVariant))
  throw Error("Unknown fitting variant");
const fitting = process.argv.includes("--fitting")
  ? createCreatureFittingLookdev(variant as CreatureFittingVariant)
  : undefined;
if (baseline && fitting) throw Error("Choose either baseline or fitting review");
const revision =
  process.argv.find((arg) => arg.startsWith("--revision="))?.slice(11) ?? (baseline ? "baseline" : "v1");
if (!/^[a-z0-9-]+$/.test(revision)) throw Error("Revision must be a simple directory name");
const root = resolve("output/character-lookdev", revision);
await mkdir(root, { recursive: true });
const character = createCharacterLookdev();
const project: Project = fitting
  ? fitting.project
  : baseline
    ? createCreatureFixture("ash-warden").project
    : parseProject({
        schemaVersion: 1,
        id: "character-lookdev",
        name: "Alpine sentinel",
        documents: character.documents,
        entry: "ash-warden-stage",
        recipes: [],
      });
const subject = project.documents.find(
  (document) => document.id === (fitting?.characterId ?? character.character),
);
if (fitting && subject?.kind === "character") {
  const audit = await auditCreatureMotion(subject, compileCharacter(subject, "interactive"), "walk");
  await Bun.write(resolve(root, "motion-review.json"), JSON.stringify(audit, null, 2));
}
const walkDuration =
  subject?.kind === "character" ? (subject.motions.find((clip) => clip.id === "walk")?.duration ?? 2.4) : 2.4;
const cameras: Record<string, Camera> = {
  "three-quarter": { position: [4.0, 2.65, 5.5], target: [0, 1.32, 0.18], fov: 39 },
  side: { position: [6, 2.0, 0.25], target: [0, 1.32, 0.18], fov: 39 },
  face: { position: [2.05, 2.36, 3.3], target: [0, 2.05, 1.35], fov: 31 },
  gameplay: { position: [10.5, 3.1, 11.5], target: [0, 1.25, 0.1], fov: 46 },
};
if (fitting)
  Object.assign(cameras, {
    "three-quarter": { position: [4, 2.7, -5], target: [0, 1.65, -0.05], fov: 39 },
    side: { position: [6, 2.1, 0], target: [0, 1.65, 0], fov: 39 },
    face: { position: [1.45, 2.75, 2.7], target: [0.1, 2.57, 0.08], fov: 31 },
    garment: { position: [1.4, 2.3, -3], target: [0, 1.65, -0.25], fov: 34 },
    gameplay: { position: [8, 3, -10], target: [0, 1.65, 0], fov: 46 },
  });
await withBrowser(
  async (view, _output, errors) => {
    const server = await fixtureServer(
      `import {createCreatureRenderFixture} from ${JSON.stringify(resolve("tools/fixtures/creature-render.ts"))}; createCreatureRenderFixture(${JSON.stringify(fitting?.characterId ?? "ash-warden")},${JSON.stringify(project)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      root,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const frames: {
        id: string;
        mode: string;
        camera: Camera;
        groom: boolean;
        motion?: string;
        time?: number;
        hideGarments?: boolean;
      }[] = [
        { id: "clay-side", mode: "clay", camera: cameras.side, groom: true },
        { id: "clay-three-quarter", mode: "clay", camera: cameras["three-quarter"], groom: true },
        { id: "beauty-three-quarter", mode: "beauty", camera: cameras["three-quarter"], groom: false },
        { id: "beauty-face", mode: "beauty", camera: cameras.face, groom: false },
        { id: "gameplay", mode: "beauty", camera: cameras.gameplay, groom: false },
        {
          id: "walk-contact",
          mode: "beauty",
          camera: cameras["three-quarter"],
          groom: false,
          motion: "walk",
          time: 0.45,
        },
      ];
      if (fitting)
        frames.push(
          { id: "garment-closeup", mode: "beauty", camera: cameras.garment, groom: false },
          { id: "silhouette-back", mode: "silhouette", camera: cameras["three-quarter"], groom: true },
          {
            id: "garment-walk",
            mode: "beauty",
            camera: cameras.garment,
            groom: false,
            motion: "walk",
            time: 0.8,
          },
          {
            id: "silhouette-front",
            mode: "silhouette",
            camera: { position: [0, 1.8, 6], target: [0, 1.6, 0], fov: 38 },
            groom: true,
          },
          {
            id: "clay-anatomy-front",
            mode: "clay",
            camera: { position: [2.8, 2.4, 5.3], target: [0, 1.6, 0], fov: 38 },
            groom: true,
            hideGarments: true,
          },
          {
            id: "walk-weight-left",
            mode: "clay",
            camera: cameras["three-quarter"],
            groom: true,
            motion: "walk",
            time: walkDuration * 0.25,
          },
          {
            id: "walk-weight-right",
            mode: "clay",
            camera: cameras["three-quarter"],
            groom: true,
            motion: "walk",
            time: walkDuration * 0.75,
          },
        );
      const report: unknown[] = [];
      for (const frame of frames) {
        const result = await view.evaluate<{
          image: string;
          measurements: { adapter: string };
          completeness: { complete: boolean };
          diagnostics: { severity: string }[];
        }>(
          `fixture.capture(${JSON.stringify(frame.motion ?? "idle")},${frame.time ?? 0},'three-quarter',${JSON.stringify(frame.mode)},${frame.groom},false,{camera:${JSON.stringify(frame.camera)},hideGarments:${frame.hideGarments ?? false}})`,
        );
        const { image, ...metadata } = result;
        await Bun.write(resolve(root, `${frame.id}.png`), Buffer.from(image.split(",")[1], "base64"));
        report.push({ ...metadata, frame: frame.id });
        if (
          !result.completeness.complete ||
          /swiftshader|software|llvmpipe/i.test(result.measurements.adapter)
        )
          throw Error("Capture lacks complete hardware-rendered evidence");
      }
      await Bun.write(
        resolve(root, "capture.json"),
        JSON.stringify({ revision, fitting: fitting?.fitting, frames: report, errors }, null, 2),
      );
      await Bun.write(resolve(root, "source.json"), JSON.stringify(project, null, 2));
      console.log(JSON.stringify({ root, errors }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  960,
  720,
);
