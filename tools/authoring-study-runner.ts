import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { StudyReviewer } from "@wrela/authoring";
import { currentAuthoringReviewSession } from "./authoring-review-session";
import { fixtureServer, waitFor, withBrowser } from "./browser";

export function studyBrowserReviewer(output: string): StudyReviewer {
  return async (baseline, candidates, constraints, settings) => {
    const warm = currentAuthoringReviewSession();
    if (warm) return warm.study(output, baseline, candidates, constraints, settings);
    const directory = resolve(output);
    await mkdir(directory, { recursive: true });
    return withBrowser(
      async (view, browserOutput, errors) => {
        const server = await fixtureServer(
          `import {reviewAuthoringStudy} from ${JSON.stringify(resolve("packages/review/src/index.ts"))};
        window.runStudy=async(b,c,cs,s,prefix)=>{const artifacts=[]; const result=await reviewAuthoringStudy(b,c,cs,s,async(name,value)=>{if(value instanceof Blob){const data=await new Promise((r,j)=>{const f=new FileReader();f.onload=()=>r(f.result);f.onerror=()=>j(f.error);f.readAsDataURL(value)});artifacts.push({name,data})}else artifacts.push({name,value});return prefix+'/'+name});return {result,artifacts}};window.ready=true;`,
          browserOutput,
        );
        try {
          await view.navigate(String(server.url));
          await waitFor(view, "window.ready", 60000);
          const response = await view.evaluate<{
            result: Awaited<ReturnType<StudyReviewer>>;
            artifacts: { name: string; data?: string; value?: object }[];
          }>(
            `runStudy(${JSON.stringify(baseline)},${JSON.stringify(candidates)},${JSON.stringify(constraints)},${JSON.stringify(settings)},${JSON.stringify(directory)})`,
          );
          for (const a of response.artifacts) {
            if (!/^[a-zA-Z0-9_.-]+$/.test(a.name)) throw Error("Invalid study artifact name");
            await Bun.write(
              join(directory, a.name),
              a.data ? Buffer.from(a.data.split(",")[1], "base64") : JSON.stringify(a.value, null, 2),
            );
          }
          await Bun.write(
            join(directory, "browser-evidence.json"),
            JSON.stringify({ browserOutput, errors }, null, 2),
          );
          if (errors.length) throw Error(errors.join("\n"));
          return response.result;
        } finally {
          server.stop(true);
        }
      },
      settings.width,
      settings.height,
      "chrome",
      600000,
    );
  };
}
