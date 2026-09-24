import { resolve } from "node:path";
import { invokeAuthoringAgent } from "./authoring-agent";
import { withAuthoringReviewSession } from "./authoring-review-session";

/** Optional local transport for hosts without dynamic MCP registration. A random per-session token
 * gates requests; no shell execution endpoint. The owner closes the warm GPU session explicitly. */
export async function serveAuthoringSession(destination: string) {
  const startedAt = Date.now(),
    token = crypto.randomUUID(),
    ready = resolve(destination);
  let complete!: () => void;
  const stopped = new Promise<void>((r) => {
    complete = r;
  });
  await withAuthoringReviewSession(async () => {
    let tail: Promise<unknown> = Promise.resolve();
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      idleTimeout: 255,
      async fetch(request) {
        if (request.headers.get("authorization") !== `Bearer ${token}`)
          return new Response("Unauthorized", { status: 401 });
        if (request.method !== "POST") return new Response("POST required", { status: 405 });
        if (new URL(request.url).pathname === "/close") {
          void tail.finally(complete);
          return Response.json({ closing: true });
        }
        if (new URL(request.url).pathname !== "/invoke")
          return new Response("Unknown endpoint", { status: 404 });
        const receivedAt = Date.now();
        if (Number(request.headers.get("content-length") ?? 0) > 4 * 1024 * 1024)
          return new Response("Request too large", { status: 413 });
        const text = await request.text();
        if (text.length > 4 * 1024 * 1024) return new Response("Request too large", { status: 413 });
        let input: unknown;
        try {
          input = JSON.parse(text);
        } catch {
          return new Response("Invalid JSON", { status: 400 });
        }
        const run = tail.then(async () => {
          const executionStartedAt = Date.now();
          const result = await invokeAuthoringAgent(input as Parameters<typeof invokeAuthoringAgent>[0]);
          return {
            ...result,
            timeline: {
              receivedAt,
              executionStartedAt,
              completedAt: Date.now(),
              queueMs: executionStartedAt - receivedAt,
              sessionStartedAt: startedAt,
            },
          };
        });
        tail = run.catch(() => {});
        try {
          return Response.json(await run);
        } catch (error) {
          return Response.json({ error: String(error) }, { status: 400 });
        }
      },
    });
    await Bun.write(
      ready,
      JSON.stringify(
        {
          version: 1,
          url: `http://127.0.0.1:${server.port}`,
          token,
          startedAt,
          readyAt: Date.now(),
          pid: process.pid,
        },
        null,
        2,
      ),
    );
    process.stdout.write(JSON.stringify({ ready, startedAt, readyAt: Date.now() }) + "\n");
    const timeout = setTimeout(complete, 14 * 60 * 1000);
    try {
      await stopped;
      await tail;
    } finally {
      clearTimeout(timeout);
      await server.stop(true);
    }
  });
}
export async function invokeAuthoringSession(sessionPath: string, invocationPath: string) {
  const session = await Bun.file(sessionPath).json(),
    input = await Bun.file(invocationPath).json();
  const response = await fetch(`${session.url}/invoke`, {
    method: "POST",
    headers: { authorization: `Bearer ${session.token}`, "content-type": "application/json" },
    body: JSON.stringify(input),
    signal: AbortSignal.timeout(240000),
  });
  const result = await response.json();
  if (!response.ok) throw Error(JSON.stringify(result));
  return result;
}
if (import.meta.main) {
  if (process.argv[2] === "invoke")
    console.log(JSON.stringify(await invokeAuthoringSession(process.argv[3], process.argv[4])));
  else if (process.argv[2] === "close") {
    const session = await Bun.file(process.argv[3]).json();
    console.log(
      await (
        await fetch(`${session.url}/close`, {
          method: "POST",
          headers: { authorization: `Bearer ${session.token}` },
        })
      ).json(),
    );
  } else await serveAuthoringSession(process.argv[2]);
}
