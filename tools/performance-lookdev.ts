import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { compileCharacter } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples";
import { createCharacterLookdev } from "@wrela/examples/character-lookdev";
import { type Camera, parseProject } from "@wrela/model";
import { auditCreatureMotion } from "@wrela/runtime";
import { fixtureServer, waitFor, withBrowser } from "./browser";

const argument = (name: string, fallback: string) =>
  process.argv.find((value) => value.startsWith(`--${name}=`))?.slice(name.length + 3) ?? fallback;
const baseline = process.argv.includes("--baseline");
const skeleton = process.argv.includes("--skeleton");
const revision = argument("revision", baseline ? "baseline" : "v1");
const motion = argument("motion", "walk");
const count = Number(argument("frames", "24"));
if (
  !/^[a-z0-9-]+$/.test(revision) ||
  !["idle", "walk", "turn", "interact"].includes(motion) ||
  !Number.isInteger(count) ||
  count < 4 ||
  count > 60
)
  throw Error("Use a simple revision, idle/walk/turn/interact, and 4–60 frames");
const root = resolve("output/performance-lookdev", revision, motion);
await mkdir(root, { recursive: true });
const lookdev = createCharacterLookdev();
const character = lookdev.documents.find((document) => document.id === lookdev.character);
if (character?.kind !== "character" || !character.creature) throw Error("Missing lookdev character");
if (baseline) {
  const original = createCreatureFixture("ash-warden").project.documents.find(
    (document) => document.kind === "character",
  );
  if (!original || original.kind !== "character" || !original.creature) throw Error("Missing baseline");
  character.motions = original.motions;
  character.creature.contacts = original.creature.contacts;
  character.creature.reviewScenarios = original.creature.reviewScenarios;
  character.performance = undefined;
}
const clip = character.motions.find((value) => value.id === motion);
if (!clip) throw Error("Missing selected clip");
const project = parseProject({
  schemaVersion: 1,
  id: "performance-lookdev",
  name: "Warden movement study",
  documents: lookdev.documents,
  entry: "ash-warden-stage",
  recipes: [],
});
const physicalReview = await auditCreatureMotion(
  character,
  compileCharacter(character, "interactive"),
  motion,
);
await Bun.write(resolve(root, "motion-review.json"), JSON.stringify(physicalReview, null, 2));
const camera: Camera =
  motion === "interact" || motion === "turn"
    ? { position: [3.8, 2.45, 5.1], target: [0, 1.4, 0.55], fov: 40 }
    : { position: [5.9, 1.95, 0.65], target: [0, 1.22, 0.65], fov: 41 };
await withBrowser(
  async (view, _output, errors) => {
    const server = await fixtureServer(
      `import {createCreatureRenderFixture} from ${JSON.stringify(resolve("tools/fixtures/creature-render.ts"))}; createCreatureRenderFixture('ash-warden',${JSON.stringify(project)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      root,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const report: unknown[] = [];
      await view.evaluate("window.framesForVideo=[]");
      for (let frame = 0; frame < count; frame++) {
        const time = (frame / count) * clip.duration;
        const capture = await view.evaluate<{
          image: string;
          measurements: { adapter: string };
          completeness: { complete: boolean };
        }>(
          `fixture.capture(${JSON.stringify(motion)},${time},'side','beauty',false,${skeleton},{camera:${JSON.stringify(camera)}}).then(frame=>{window.framesForVideo.push(frame.image);return frame})`,
        );
        const { image, ...metadata } = capture;
        await Bun.write(
          resolve(root, `frame-${String(frame).padStart(3, "0")}.png`),
          Buffer.from(image.split(",")[1], "base64"),
        );
        report.push(metadata);
        if (
          !capture.completeness.complete ||
          /swiftshader|software|llvmpipe/i.test(capture.measurements.adapter)
        )
          throw Error("Incomplete or software-rendered capture");
      }
      const video = await view.evaluate<string>(`(async()=>{
      const images=await Promise.all(window.framesForVideo.map(src=>new Promise(resolve=>{const image=new Image();image.onload=()=>resolve(image);image.src=src})));
      const canvas=document.createElement('canvas');canvas.width=720;canvas.height=540;const context=canvas.getContext('2d');
      const stream=canvas.captureStream(30);const recorder=new MediaRecorder(stream,{mimeType:'video/webm;codecs=vp9'});const chunks=[];
      const done=new Promise(resolve=>{recorder.ondataavailable=e=>{if(e.data.size)chunks.push(e.data)};recorder.onstop=()=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.readAsDataURL(new Blob(chunks,{type:'video/webm'}))}});
      context.drawImage(images[0],0,0,720,540);recorder.start();
      for(const image of images){context.drawImage(image,0,0,720,540);await new Promise(resolve=>setTimeout(resolve,${(clip.duration * 1000) / count}))}
      recorder.stop();stream.getTracks().forEach(track=>track.stop());return await done;
    })()`);
      await Bun.write(resolve(root, `${motion}.webm`), Buffer.from(video.split(",")[1], "base64"));
      const sheet = await view.evaluate<string>(
        `(async()=>{const images=await Promise.all(window.framesForVideo.filter((_,i)=>i%Math.max(1,Math.floor(${count}/8))===0).slice(0,8).map(src=>new Promise(resolve=>{const image=new Image();image.onload=()=>resolve(image);image.src=src})));const canvas=document.createElement('canvas');canvas.width=1440;canvas.height=580;const ctx=canvas.getContext('2d');ctx.fillStyle='#111';ctx.fillRect(0,0,1440,580);images.forEach((image,i)=>{ctx.drawImage(image,(i%4)*360,Math.floor(i/4)*290,360,270);ctx.fillStyle='#ddd';ctx.font='13px sans-serif';ctx.fillText('${motion} · '+(i*Math.max(1,Math.floor(${count}/8))*${clip.duration}/${count}).toFixed(2)+'s',(i%4)*360+10,Math.floor(i/4)*290+284)});return canvas.toDataURL('image/png')})()`,
      );
      await Bun.write(resolve(root, "contact-sheet.png"), Buffer.from(sheet.split(",")[1], "base64"));
      await Bun.write(
        resolve(root, "capture.json"),
        JSON.stringify(
          {
            revision,
            motion,
            skeleton,
            camera,
            physicalReview: {
              maximumPlantedSlip: physicalReview.maximumPlantedSlip,
              maximumContactResidual: physicalReview.maximumContactResidual,
              frames: physicalReview.samples.length,
              issues: [...new Set(physicalReview.samples.flatMap((frame) => frame.issues))],
              evidence: "motion-review.json",
            },
            frames: report,
            errors,
            scope: "Actual runtime/render evidence; artistic acceptance remains a human judgement",
          },
          null,
          2,
        ),
      );
      await Bun.write(resolve(root, "source.json"), JSON.stringify(project, null, 2));
      console.log(JSON.stringify({ root, errors }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  720,
  540,
);
