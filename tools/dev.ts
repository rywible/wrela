import { cookProject } from "@wrela/compiler";
import { referenceProject } from "@wrela/model";
import player from "../apps/player/index.html";
import studio from "../apps/studio/index.html";
import { WorkspaceBridge } from "./bridge";
import { compilerSourceFingerprint } from "./cook";

const port = Number(process.env.PORT ?? 4173),
  token = crypto.randomUUID();
const projectArg = process.argv.indexOf("--project");
const bridge =
  projectArg >= 0 && process.argv[projectArg + 1] ? new WorkspaceBridge(process.argv[projectArg + 1]) : null;
await bridge?.initialize();
const compilerSource = await compilerSourceFingerprint();
const compilerDefine = { WRELA_COMPILER_SOURCE: JSON.stringify(compilerSource) };
const cookedProject = cookProject(referenceProject(), "review", compilerSource);
const distributionNotice = `/*!\n${await Bun.file("LICENSE").text()}\n${await Bun.file("THIRD_PARTY_NOTICES.txt").text()}\n*/`;
async function bundlePlayer() {
  const result = await Bun.build({
    entrypoints: ["apps/player/src/main.ts"],
    target: "browser",
    minify: true,
    banner: distributionNotice,
    loader: { ".wgsl": "text" },
    define: compilerDefine,
  });
  if (!result.success) throw Error(result.logs.join("\n"));
  return result.outputs[0].text();
}
const server = Bun.serve({
  hostname: "127.0.0.1",
  port,
  maxRequestBodySize: 8 * 1024 * 1024,
  development: { hmr: true, console: true },
  routes: { "/": studio, "/player": player },
  async fetch(req) {
    const url = new URL(req.url);
    if (url.pathname.startsWith("/bridge/")) {
      const expectedOrigin = `http://127.0.0.1:${server.port}`;
      const origin = req.headers.get("origin");
      const site = req.headers.get("sec-fetch-site");
      if (
        url.origin !== expectedOrigin ||
        req.headers.get("host") !== `127.0.0.1:${server.port}` ||
        (origin && origin !== expectedOrigin) ||
        (site && site !== "same-origin" && site !== "none")
      )
        return new Response("Origin rejected", { status: 403 });
    }

    if (url.pathname === "/compile-worker.js") {
      const result = await Bun.build({
        entrypoints: ["packages/compiler/src/worker.ts"],
        target: "browser",
        minify: false,
        banner: distributionNotice,
        define: compilerDefine,
      });
      if (!result.success) return new Response(result.logs.join("\n"), { status: 500 });
      return new Response(await result.outputs[0].text(), {
        headers: { "Content-Type": "text/javascript", "Cache-Control": "no-store" },
      });
    }
    if (url.pathname === "/cooked-project.json") return Response.json(cookedProject);
    if (url.pathname === "/compiler-identity.json") return Response.json({ compilerSource });
    if (url.pathname === "/player-bundle.js")
      return new Response(await bundlePlayer(), { headers: { "Content-Type": "text/javascript" } });
    if (url.pathname === "/bridge/session") {
      if (req.method !== "GET") return new Response("Method not allowed", { status: 405 });
      if (req.headers.get("sec-fetch-site") === "cross-site")
        return new Response("Forbidden", { status: 403 });
      return Response.json(
        { available: !!bridge },
        { headers: { "Set-Cookie": `wrela-session=${token}; HttpOnly; SameSite=Strict; Path=/` } },
      );
    }
    if (url.pathname === "/bridge/project") {
      if (!bridge)
        return Response.json(
          { error: "Start with --project <directory> to open a workspace" },
          { status: 404 },
        );
      if (
        !req.headers
          .get("cookie")
          ?.split(";")
          .some((c) => c.trim() === `wrela-session=${token}`)
      )
        return new Response("Unauthorized", { status: 401 });
      if (req.method === "GET") return Response.json(await bridge.read());
      if (req.method === "PUT") {
        if (!req.headers.get("content-type")?.startsWith("application/json"))
          return new Response("JSON required", { status: 415 });
        if (req.headers.get("origin") !== url.origin) return new Response("Origin rejected", { status: 403 });
        if (Number(req.headers.get("content-length") ?? 0) > 8388608)
          return new Response("Project too large", { status: 413 });
        try {
          const raw = await req.text();
          if (raw.length > 8388608) return new Response("Project too large", { status: 413 });
          const { project, expectedKey } = JSON.parse(raw);
          return Response.json(await bridge.save(project, expectedKey));
        } catch (e) {
          return Response.json({ error: String(e) }, { status: 409 });
        }
      }
    }
    return new Response("Not found", { status: 404 });
  },
});
console.log(`Wrela Studio: ${server.url}${bridge ? " · workspace bridge enabled" : ""}`);
