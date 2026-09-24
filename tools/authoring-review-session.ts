import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { PacketSettings, ResultConstraint, ReviewPacket, StudyReviewer } from "@wrela/authoring";
import type { Project } from "@wrela/model";
import { fixtureServer, waitFor, withBrowser } from "./browser";

export type WarmReviewSession = {
  study: (output: string, ...args: Parameters<StudyReviewer>) => ReturnType<StudyReviewer>;
  packet: (
    output: string,
    b: Project,
    c: Project,
    cs: ResultConstraint[],
    s: PacketSettings,
  ) => Promise<ReviewPacket>;
};
let active: WarmReviewSession | undefined;
export function currentAuthoringReviewSession() {
  return active;
}
/** A command/agent host owns the browser lifetime. Review packages stay independent of Bun and transport. */
export async function withAuthoringReviewSession<T>(run: () => Promise<T>): Promise<T> {
  if (active) return run();
  return withBrowser(
    async (view, browserOutput, errors) => {
      const server = await fixtureServer(
        `import {createAuthoringReviewSession} from ${JSON.stringify(resolve("packages/review/src/index.ts"))};
      const session=createAuthoringReviewSession();window.closeReview=()=>session.close();
      window.warmReview=async(kind,args,prefix)=>{const artifacts=[];const result=await session[kind](...args,async(name,value)=>{if(value instanceof Blob){const data=await new Promise((r,j)=>{const f=new FileReader();f.onload=()=>r(f.result);f.onerror=()=>j(f.error);f.readAsDataURL(value)});artifacts.push({name,data})}else artifacts.push({name,value});return prefix+'/'+name});return {result,artifacts}};window.ready=true;`,
        browserOutput,
      );
      let queue: Promise<unknown> = Promise.resolve();
      const review = <R>(kind: string, output: string, args: unknown[]): Promise<R> => {
        const task = queue.then(async () => {
          const dir = resolve(output);
          await mkdir(dir, { recursive: true });
          const data = await view.evaluate<{
            result: R;
            artifacts: { name: string; data?: string; value?: object }[];
          }>(`warmReview(${JSON.stringify(kind)},${JSON.stringify(args)},${JSON.stringify(dir)})`);
          for (const artifact of data.artifacts) {
            if (!/^[a-zA-Z0-9_.-]+$/.test(artifact.name)) throw Error("Invalid review artifact name");
            await Bun.write(
              join(dir, artifact.name),
              artifact.data
                ? Buffer.from(artifact.data.split(",")[1], "base64")
                : JSON.stringify(artifact.value, null, 2),
            );
          }
          await Bun.write(
            join(dir, "browser-evidence.json"),
            JSON.stringify({ browserOutput, errors, warmSession: true }, null, 2),
          );
          if (errors.length) throw Error(errors.join("\n"));
          return data.result;
        });
        queue = task.catch(() => {});
        return task;
      };
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready", 60000);
        active = {
          study: (output, ...args) => review("study", output, args),
          packet: (output, ...args) => review("packet", output, args),
        };
        return await run();
      } finally {
        active = undefined;
        await queue;
        await view.evaluate("closeReview()").catch(() => {});
        server.stop(true);
      }
    },
    1024,
    768,
    "chrome",
    900000,
  );
}
