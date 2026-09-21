import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { referenceProject } from "@wrela/model";

test("loopback bridge authenticates exact origin and refuses stale external writes", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-http-"));
  const child = Bun.spawn(["bun", "tools/dev.ts", "--project", directory], {
    env: { ...process.env, PORT: "0" },
    stdout: "pipe",
    stderr: "pipe",
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const url = await Promise.race([
      (async () => {
        const reader = child.stdout.getReader();
        let output = "";
        try {
          while (true) {
            const { value, done } = await reader.read();
            if (done) {
              throw Error("Bridge server exited before readiness");
            }
            output += new TextDecoder().decode(value);
            const match = output.match(/http:\/\/127\.0\.0\.1:\d+/);
            if (match) {
              return match[0];
            }
          }
        } finally {
          reader.releaseLock();
        }
      })(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(Error("Bridge server timed out")), 10000);
      }),
    ]);
    if (timer) clearTimeout(timer);
    expect(
      (await fetch(`${url}/bridge/session`, { headers: { Origin: "https://untrusted.example" } })).status,
    ).toBe(403);
    expect((await fetch(`${url}/bridge/session`, { headers: { Host: "untrusted.example" } })).status).toBe(
      403,
    );
    expect(
      (await fetch(`${url}/bridge/session`, { headers: { "Sec-Fetch-Site": "same-site" } })).status,
    ).toBe(403);
    expect((await fetch(`${url}/bridge/session`, { method: "POST" })).status).toBe(405);
    expect((await fetch(`${url}/bridge/project`)).status).toBe(401);
    const connection = await fetch(`${url}/bridge/session`),
      cookie = connection.headers.get("set-cookie")?.split(";")[0];
    expect(cookie).toBeDefined();
    const headers = { "Content-Type": "application/json", Cookie: cookie ?? "", Origin: url };
    const body = JSON.stringify({ project: referenceProject(), expectedKey: null });
    expect(
      (
        await fetch(`${url}/bridge/project`, {
          method: "PUT",
          headers: { ...headers, Origin: "https://untrusted.example" },
          body,
        })
      ).status,
    ).toBe(403);
    expect(
      (
        await fetch(`${url}/bridge/project`, {
          method: "PUT",
          headers: { ...headers, "Content-Type": "text/plain" },
          body,
        })
      ).status,
    ).toBe(415);
    expect(
      (
        await fetch(`${url}/bridge/project`, {
          method: "PUT",
          headers,
          body: " ".repeat(8 * 1024 * 1024 + 1),
        })
      ).status,
    ).toBe(413);
    const saved = await fetch(`${url}/bridge/project`, { method: "PUT", headers, body });
    expect(saved.status).toBe(200);
    expect((await fetch(`${url}/bridge/project`, { method: "PUT", headers, body })).status).toBe(409);
    const read = await fetch(`${url}/bridge/project`, { headers: { Cookie: cookie ?? "" } });
    expect((await read.json()).project.id).toBe("winter-garden");
  } finally {
    if (timer) clearTimeout(timer);
    child.kill();
    await child.exited;
    await rm(directory, { recursive: true, force: true });
  }
}, 15000);
