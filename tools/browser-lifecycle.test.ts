import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browserDeadline, ownedBrowserProcess } from "./browser-lifecycle";

test("a stalled browser operation times out and stops its owner", async () => {
  let stopped = false;
  await expect(
    browserDeadline(
      new Promise(() => {}),
      "capture",
      () => {
        stopped = true;
      },
      10,
    ),
  ).rejects.toThrow("stopped its process group");
  expect(stopped).toBe(true);
  expect(
    await browserDeadline(
      Promise.resolve(42),
      "read",
      () => {
        throw Error("unexpected timeout");
      },
      10,
    ),
  ).toBe(42);
});

test.skipIf(process.platform === "win32")("cleanup kills a hung browser and its worker process", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-lifecycle-test-"));
  const pidFile = join(directory, "worker.pid");
  const childScript = `await Bun.write(${JSON.stringify(pidFile)}, String(process.pid));process.on("SIGTERM",()=>{});setInterval(()=>{},1000);`;
  const script = `import {spawn} from "node:child_process";spawn(process.execPath,["-e",${JSON.stringify(childScript)}],{stdio:"ignore"});process.on("SIGTERM",()=>{});setInterval(()=>{},1000);`;
  const browser = ownedBrowserProcess(process.execPath, ["-e", script]);
  let workerPid = 0;
  try {
    const deadline = Date.now() + 2000;
    while (!workerPid && Date.now() < deadline) {
      workerPid = Number(await readFile(pidFile, "utf8").catch(() => "0"));
      if (!workerPid) await Bun.sleep(10);
    }
    expect(workerPid).toBeGreaterThan(0);
    browser.stop();
    await browser.exited;
    let alive = true;
    for (let attempt = 0; attempt < 100 && alive; attempt++) {
      try {
        process.kill(workerPid, 0);
        await Bun.sleep(10);
      } catch {
        alive = false;
      }
    }
    expect(alive).toBe(false);
    browser.stop();
  } finally {
    browser.stop();
    await rm(directory, { recursive: true, force: true });
  }
});
