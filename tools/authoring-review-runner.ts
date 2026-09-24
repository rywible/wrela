import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { AuthoringReview, ResultConstraint } from "@wrela/authoring";
import type { Project } from "@wrela/model";

import { type captureAuthoring, reviewAuthoring } from "@wrela/review";
import { fixtureServer, waitFor, withBrowser } from "./browser";

type Artifact = { name: string; data?: string; value?: object };
async function saveArtifacts(directory: string, artifacts: Artifact[]) {
  await mkdir(directory, { recursive: true });
  for (const artifact of artifacts) {
    if (!/^[a-zA-Z0-9_.-]+$/.test(artifact.name)) throw Error("Invalid artifact name");
    await Bun.write(
      join(directory, artifact.name),
      artifact.data
        ? Buffer.from(artifact.data.split(",")[1], "base64")
        : JSON.stringify(artifact.value, null, 2),
    );
  }
}
export async function runAuthoringReview(
  baseline: Project,
  candidate: Project,
  constraints: ResultConstraint[],
  output: string,
  hardware = false,
): Promise<AuthoringReview> {
  const directory = resolve(output);
  await mkdir(directory, { recursive: true });
  if (!hardware)
    return reviewAuthoring(baseline, candidate, constraints, {
      saveArtifact: async (name, value) => {
        const path = join(directory, name);
        await Bun.write(path, value instanceof Blob ? value : JSON.stringify(value, null, 2));
        return path;
      },
    });
  return withBrowser(
    async (view, browserOutput, errors) => {
      const server = await fixtureServer(
        `import {reviewAuthoring} from ${JSON.stringify(resolve("packages/review/src/index.ts"))};
      window.review=async(b,c,cs,prefix)=>{const artifacts=[];const report=await reviewAuthoring(b,c,cs,{saveArtifact:async(name,value)=>{
        if(value instanceof Blob){const data=await new Promise((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(r.result);r.onerror=()=>reject(r.error);r.readAsDataURL(value)});artifacts.push({name,data})}
        else artifacts.push({name,value});return prefix+'/'+name;
      }});return {report,artifacts}};window.ready=true;`,
        browserOutput,
      );
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready", 60000);
        const result = await view.evaluate<{ report: AuthoringReview; artifacts: Artifact[] }>(
          `review(${JSON.stringify(baseline)},${JSON.stringify(candidate)},${JSON.stringify(constraints)},${JSON.stringify(directory)})`,
        );
        await saveArtifacts(directory, result.artifacts);
        await Bun.write(
          join(directory, "browser-evidence.json"),
          JSON.stringify({ browserOutput, errors }, null, 2),
        );
        if (errors.length) throw Error(errors.join("\n"));
        return result.report;
      } finally {
        server.stop(true);
      }
    },
    640,
    480,
    "chrome",
    600000,
  );
}
export async function runAuthoringCapture(
  project: Project,
  settings: Parameters<typeof captureAuthoring>[1],
  output: string,
) {
  return withBrowser(
    async (view, browserOutput, errors) => {
      const server = await fixtureServer(
        `import {captureAuthoring} from ${JSON.stringify(resolve("packages/review/src/index.ts"))};
      window.capture=async(p,s)=>{const result=await captureAuthoring(p,s);const data=await new Promise((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(r.result);r.onerror=()=>reject(r.error);r.readAsDataURL(result.blob)});return {data,metadata:result.metadata}};window.ready=true;`,
        browserOutput,
      );
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready", 60000);
        const result = await view.evaluate<{ data: string; metadata: object }>(
          `capture(${JSON.stringify(project)},${JSON.stringify(settings)})`,
        );
        if (errors.length) throw Error(errors.join("\n"));
        await saveArtifacts(output, [
          { name: "capture.png", data: result.data },
          { name: "capture.json", value: { ...result.metadata, browserOutput } },
        ]);
        return { image: join(resolve(output), "capture.png"), metadata: result.metadata, browserOutput };
      } finally {
        server.stop(true);
      }
    },
    settings.width ?? 640,
    settings.height ?? 480,
    "chrome",
    600000,
  );
}
