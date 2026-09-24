import { join, resolve } from "node:path";
import { compileCharacter } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples";
import { type Camera, type CharacterDefinition, contentKey, type Project, parseProject } from "@wrela/model";
import { auditCreatureMotion, reviewCreature } from "@wrela/runtime";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createCreatureRenderFixture } from "./fixtures/creature-render";

export function creatureAtlasPlan(character: CharacterDefinition) {
  const biped = character.id === "reed-penitent",
    center: [number, number, number] = [0, biped ? 1.6 : 1.4, 0.2];
  const full = (position: [number, number, number]): Camera => ({ position, target: center, fov: 38 });
  const face: Camera = {
    position: [2.2, 2.35, 3.8],
    target: biped ? [0.1, 2.6, 0.1] : [0, 2.08, 1.35],
    fov: 30,
  };
  const frames: {
    id: string;
    motion: string;
    time: number;
    camera: Camera;
    light: "neutral" | "raking" | "backlight";
    reason: string;
  }[] = [
    {
      id: "front",
      motion: "idle",
      time: 0,
      camera: full([0, 1.8, 6]),
      light: "neutral",
      reason: "Frontal proportions and bilateral structure",
    },
    {
      id: "side",
      motion: "idle",
      time: 0,
      camera: full([6, 1.8, 0.2]),
      light: "neutral",
      reason: "Profile, back line, hock and foot silhouette",
    },
    {
      id: "three-quarter",
      motion: "idle",
      time: 0,
      camera: full([4, 2.4, 5]),
      light: "neutral",
      reason: "Connected primary forms",
    },
    {
      id: "face",
      motion: "idle",
      time: 0,
      camera: face,
      light: "raking",
      reason: "Eyelids, muzzle planes and jaw separation",
    },
    {
      id: "shoulder",
      motion: "idle",
      time: 0,
      camera: { position: [3, 2.1, 2.8], target: [0, 1.6, 0.65], fov: 28 },
      light: "raking",
      reason: "Shoulder volume and neck transitions",
    },
    {
      id: "feet",
      motion: "idle",
      time: 0,
      camera: { position: [2, 0.65, 2], target: [0, 0.3, 0.3], fov: 36 },
      light: "neutral",
      reason: "Paws, pads, toes and ground clearance",
    },
  ];
  const attack =
    character.motions.find((m) => m.id === "lunge") ?? character.motions.find((m) => m.id === "coil");
  if (attack)
    for (let i = 0; i < 6; i++)
      frames.push({
        id: `attack-${i}`,
        motion: attack.id,
        time: (attack.duration * i) / 5,
        camera: full([4, 2.4, 5]),
        light: "neutral",
        reason: `Attack interval ${i + 1}/6; assess weight and silhouette continuity`,
      });
  const reviews = (character.creature?.reviewScenarios ?? []).map((s) => reviewCreature(character, s));
  const worst = reviews
    .flatMap((review) =>
      review.diagnostics
        .filter((d) => d.time !== undefined && d.measured !== null && d.threshold !== null && d.threshold > 0)
        .map((d) => ({ review, d, ratio: (d.measured ?? 0) / (d.threshold ?? 1) })),
    )
    .sort((a, b) => b.ratio - a.ratio)
    .slice(0, 3);
  for (let i = 0; i < worst.length; i++) {
    const { review, d } = worst[i],
      motion = character.creature?.reviewScenarios.find((s) => s.id === review.scenario)?.motion;
    if (motion)
      frames.push({
        id: `worst-${i}`,
        motion,
        time: Math.round((d.time ?? 0) * 60) / 60,
        camera: full([4, 2.4, 5]),
        light: "neutral",
        reason: `Largest normalized ${d.kind} measurement: ${d.source} (${d.measured} / ${d.threshold})`,
      });
  }
  return {
    sourceKey: contentKey(character),
    frames,
    reviews,
    worst: worst.map(({ review, d, ratio }) => ({ scenario: review.scenario, ...d, ratio })),
    scope:
      "Fixed-tick clay views and sampled attack; worst frames are contact/joint metrics, not automatic anatomy or skin-intersection judgments.",
  };
}
type Capture = Awaited<ReturnType<Awaited<ReturnType<typeof createCreatureRenderFixture>>["capture"]>>;
if (import.meta.main) {
  const option = (name: string) =>
    process.argv
      .find((a) => a.startsWith(`--${name}=`))
      ?.split("=")
      .slice(1)
      .join("=");
  const id = process.argv.includes("--biped") ? "reed-penitent" : "ash-warden";
  const fixture = createCreatureFixture(id);
  const load = async (path: string | undefined): Promise<Project> =>
    path ? parseProject(JSON.parse(await Bun.file(path).text())) : fixture.project;
  const project = await load(option("project")),
    baseline = option("baseline") ? await load(option("baseline")) : undefined;
  const character = project.documents.find((d) => d.id === id) as CharacterDefinition;
  if (character?.kind !== "character") throw Error("Atlas project is missing its study character");
  const plan = creatureAtlasPlan(character);
  const runtimeAudit = await auditCreatureMotion(character, compileCharacter(character, "review"), "lunge");
  for (const [index, sample] of runtimeAudit.worst.entries())
    plan.frames.push({
      id: `runtime-worst-${index}`,
      motion: "lunge",
      time: sample.time,
      camera: plan.frames[2].camera,
      light: "neutral",
      reason: `Largest actual runtime contact residual at tick ${sample.tick}: ${sample.maximumContactResidual} m`,
    });
  const selected = process.argv.includes("--quick") ? plan.frames.slice(0, 6) : plan.frames;
  await withBrowser(
    async (view, output, errors) => {
      const server = await fixtureServer(
        `import {createCreatureRenderFixture} from ${JSON.stringify(resolve("tools/fixtures/creature-render.ts"))};window.make=async(project)=>{window.fixture?.dispose();window.fixture=await createCreatureRenderFixture(${JSON.stringify(id)},project);return true;};window.ready=true;`,
        output,
      );
      const captures: unknown[] = [],
        panels = new Map<string, string>();
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready");
        for (const [label, source] of [
          ...(baseline ? [["baseline", baseline] as const] : []),
          ["candidate", project] as const,
        ]) {
          await view.evaluate(`make(${JSON.stringify(source)})`);
          for (const frame of selected) {
            const capture = await view.evaluate<Capture>(
              `fixture.capture(${JSON.stringify(frame.motion)},${frame.time},"three-quarter","clay",true,false,${JSON.stringify({ camera: frame.camera, light: frame.light, hideAttachments: true })})`,
            );
            if (
              !capture.completeness.complete ||
              capture.diagnostics.some((d) => d.severity === "error") ||
              /software|swiftshader|llvmpipe/i.test(capture.measurements.adapter)
            )
              throw Error(`Incomplete hardware capture ${frame.id}`);
            const { image, ...metadata } = capture,
              name = `${label}-${frame.id}.png`;
            await Bun.write(join(output, name), Buffer.from(image.split(",")[1], "base64"));
            captures.push({ label, frame, ...metadata, file: name });
            panels.set(
              `${label}/${frame.id}`,
              `<figure><img src="${name}" width="640" height="360"><figcaption>${label} · ${frame.id} · ${frame.motion} · tick ${capture.tick}</figcaption></figure>`,
            );
          }
        }
        if (errors.length) throw Error(errors.join("\n"));
        await Bun.write(
          join(output, "atlas.json"),
          JSON.stringify({ plan, runtimeAudit, captures, visualApproval: "pending" }, null, 2),
        );
        await Bun.write(
          join(output, "index.html"),
          `<!doctype html><meta charset="utf-8"><title>Creature clay review</title><style>body{background:#171b20;color:#e4e4de;font:15px system-ui;margin:24px}main{display:grid;grid-template-columns:repeat(auto-fit,minmax(480px,1fr));gap:16px}figure{margin:0}img{width:100%;height:auto}figcaption{padding:8px}</style><h1>Creature clay review</h1><p>Exact source and fixed-tick evidence in atlas.json. Visual approval remains pending.</p><main>${selected.flatMap((frame) => (baseline ? ["baseline", "candidate"] : ["candidate"]).map((label) => panels.get(`${label}/${frame.id}`))).join("")}</main>`,
        );
        console.log(JSON.stringify({ output, source: plan.sourceKey, captures: captures.length }));
      } finally {
        await view.evaluate("(()=>{window.fixture?.dispose();return true})()").catch(() => {});
        server.stop(true);
      }
    },
    640,
    360,
    "chrome",
  );
}
