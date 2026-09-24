import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { PacketSettings, ResultConstraint, ReviewPacket } from "@wrela/authoring";
import type { Project } from "@wrela/model";
import { currentAuthoringReviewSession } from "./authoring-review-session";
import { fixtureServer, waitFor, withBrowser } from "./browser";
/** Batch constraints and all views into a single hardware browser invocation. */
export async function runAuthoringPacket(
  baseline: Project,
  candidate: Project,
  constraints: ResultConstraint[],
  settings: PacketSettings,
  output: string,
): Promise<ReviewPacket> {
  const warm = currentAuthoringReviewSession();
  if (warm) return warm.packet(output, baseline, candidate, constraints, settings);
  const directory = resolve(output);
  await mkdir(directory, { recursive: true });
  return withBrowser(
    async (view, browserOutput, errors) => {
      const server = await fixtureServer(
        `import {reviewAuthoringPacket} from ${JSON.stringify(resolve("packages/review/src/index.ts"))};
  window.evaluatePacket=async(b,c,cs,s,prefix)=>{const artifacts=[];const packet=await reviewAuthoringPacket(b,c,cs,s,async(name,value)=>{if(value instanceof Blob){const data=await new Promise((r,j)=>{const f=new FileReader();f.onload=()=>r(f.result);f.onerror=()=>j(f.error);f.readAsDataURL(value)});artifacts.push({name,data})}else artifacts.push({name,value});return prefix+'/'+name});return {packet,artifacts}};window.ready=true;`,
        browserOutput,
      );
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready", 60000);
        const result = await view.evaluate<{
          packet: ReviewPacket;
          artifacts: { name: string; data?: string; value?: object }[];
        }>(
          `evaluatePacket(${JSON.stringify(baseline)},${JSON.stringify(candidate)},${JSON.stringify(constraints)},${JSON.stringify(settings)},${JSON.stringify(directory)})`,
        );
        if (errors.length) throw Error(errors.join("\n"));
        for (const a of result.artifacts) {
          if (!/^[a-zA-Z0-9_.-]+$/.test(a.name)) throw Error("Invalid packet artifact name");
          await Bun.write(
            join(directory, a.name),
            a.data ? Buffer.from(a.data.split(",")[1], "base64") : JSON.stringify(a.value, null, 2),
          );
        }
        await Bun.write(
          join(directory, "browser-evidence.json"),
          JSON.stringify({ browserOutput, errors }, null, 2),
        );
        return result.packet;
      } finally {
        server.stop(true);
      }
    },
    settings.width,
    settings.height,
    "chrome",
    600000,
  );
}
