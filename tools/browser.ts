import { mkdir, mkdtemp, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { browserDeadline, ownedBrowserProcess } from "./browser-lifecycle";
import { sourceManifest, writeEvidence } from "./evidence";

/** Bun 1.4's experimental WebView API is newer than the stable @types/bun release. */
export interface BrowserView {
  navigate(url: string): Promise<void>;
  evaluate<T = unknown>(expression: string): Promise<T>;
  screenshot(): Promise<Blob>;
  click(selector: string, options?: { timeout?: number }): Promise<void>;
  type(text: string): Promise<void>;
  scrollTo(selector: string): Promise<void>;
  press(key: string, options?: { modifiers: string[] }): Promise<void>;
  resize(width: number, height: number): Promise<void>;
  reload(): Promise<void>;
  cdp(method: string, params?: Record<string, unknown>): Promise<unknown>;
  close(): void;
}
type ViewConstructor = new (options: {
  backend: { type: "chrome"; url: string } | { type: "webkit" };
  width: number;
  height: number;
  dataStore: { directory: string };
  console: (type: string, ...args: unknown[]) => void;
}) => BrowserView;
const View = (Bun as unknown as { WebView: ViewConstructor }).WebView;

/** Cooperative machine-wide GPU lease; never relies on origin-scoped Web Locks. */
export async function acquireGpuLease(timeoutMs = 30000): Promise<() => Promise<void>> {
  const path = join(tmpdir(), "wrela-gpu-lease");
  const token = crypto.randomUUID();
  const deadline = Date.now() + timeoutMs;
  while (true) {
    try {
      await mkdir(path);
      await writeFile(
        join(path, "owner.pending"),
        JSON.stringify({ pid: process.pid, token, started: Date.now(), checkout: process.cwd() }),
      );
      await rename(join(path, "owner.pending"), join(path, "owner.json"));
      return async () => {
        const owner = JSON.parse(await readFile(join(path, "owner.json"), "utf8").catch(() => "{}"));
        if (owner.token === token) await rm(path, { recursive: true, force: true });
      };
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
      const owner = JSON.parse(await readFile(join(path, "owner.json"), "utf8").catch(() => "{}"));
      if (!owner.pid && Date.now() - (await stat(path)).mtimeMs > 10000) {
        await rm(path, { recursive: true, force: true });
        continue;
      }
      if (owner.pid) {
        let alive = true;
        try {
          process.kill(owner.pid, 0);
        } catch (e) {
          alive = (e as NodeJS.ErrnoException).code !== "ESRCH";
        }
        if (!alive) {
          await rm(path, { recursive: true, force: true });
          continue;
        }
      }
      if (Date.now() > deadline)
        throw new Error(
          `GPU lease held by process ${owner.pid ?? "initializing"} in ${owner.checkout ?? path}`,
        );
      await Bun.sleep(250);
    }
  }
}
export async function withBrowser<T>(
  run: (view: BrowserView, output: string, errors: string[]) => Promise<T>,
  width = 1280,
  height = 800,
  engine: "chrome" | "webkit" = "chrome",
  timeoutMs = 600_000,
  purpose: "measurement" | "visual-review" = "measurement",
): Promise<T> {
  if (!View) throw new Error("Browser verification requires Bun 1.4.2 or newer.");
  // Art review may share the GPU with an interactive app or another task.
  // Such captures are visual evidence only, never comparable timing evidence.
  const release = purpose === "measurement" ? await acquireGpuLease() : async () => {};
  const profile = await mkdtemp(join(tmpdir(), "wrela-browser-"));
  const output = resolve("output", `browser-${Date.now()}-${process.pid}`);
  await mkdir(output, { recursive: true });
  if (purpose === "visual-review") {
    const concurrentLease = await readFile(join(tmpdir(), "wrela-gpu-lease", "owner.json"), "utf8").catch(
      () => "null",
    );
    await writeFile(
      join(output, "visual-review.json"),
      JSON.stringify({ performanceComparable: false, concurrentLease: JSON.parse(concurrentLease) }, null, 2),
    );
  }
  const manifest = await writeEvidence(output, process.argv[1] ?? "browser");
  const errors: string[] = [];
  let view: BrowserView | undefined;
  let chrome: ReturnType<typeof ownedBrowserProcess> | undefined;
  let timedOut = false;
  const stopTimedOutBrowser = () => {
    timedOut = true;
    chrome?.stop();
  };
  const stopOnExit = () => chrome?.stop();
  const stopOnInterrupt = () => {
    chrome?.stop();
    process.exit(130);
  };
  const stopOnTerminate = () => {
    chrome?.stop();
    process.exit(143);
  };
  process.once("exit", stopOnExit);
  process.once("SIGINT", stopOnInterrupt);
  process.once("SIGTERM", stopOnTerminate);
  try {
    let endpoint = "";
    if (engine === "chrome") {
      const executable =
        process.env.BUN_CHROME_PATH ??
        (process.platform === "darwin"
          ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
          : (Bun.which("google-chrome") ?? Bun.which("chromium")));
      if (!executable)
        throw new Error("Install Chrome or set BUN_CHROME_PATH for hardware GPU verification.");
      chrome = ownedBrowserProcess(executable, [
        "--headless=new",
        "--remote-debugging-port=0",
        `--user-data-dir=${profile}`,
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-extensions",
        "--disable-background-networking",
        "--disable-background-timer-throttling",
        "--disable-renderer-backgrounding",
        ...(process.env.WRELA_COLD_SHADERS === "1" ? ["--disable-gpu-shader-disk-cache"] : []),
        "about:blank",
      ]);
      const deadline = Date.now() + 15000;
      while (!endpoint) {
        const portFile = await readFile(join(profile, "DevToolsActivePort"), "utf8").catch(() => "");
        const [port, path] = portFile.trim().split("\n");
        if (port && path) endpoint = `ws://127.0.0.1:${port}${path}`;
        else {
          if (Date.now() > deadline) throw new Error("Chrome did not expose a debugging endpoint.");
          await Bun.sleep(100);
        }
      }
    }
    view = new View({
      backend: engine === "chrome" ? { type: "chrome", url: endpoint } : { type: "webkit" },
      width,
      height,
      dataStore: { directory: profile },
      console: (type, ...args) => {
        if (type === "error") errors.push(args.map(String).join(" "));
      },
    });
    // Bound every browser round trip. A hung capture must never prevent cleanup.
    const rawView = view;
    view = new Proxy(rawView, {
      get(target, property) {
        const member = Reflect.get(target, property);
        if (typeof member !== "function" || property === "close")
          return typeof member === "function" ? member.bind(target) : member;
        return (...args: unknown[]) => {
          if (timedOut) return Promise.reject(new Error("Browser stopped after a timeout."));
          return browserDeadline(
            Promise.resolve(member.apply(target, args)),
            property === "click" || property === "scrollTo"
              ? `${String(property)} ${String(args[0]).slice(0, 160)}`
              : String(property),
            stopTimedOutBrowser,
          );
        };
      },
    });
    if (engine === "chrome") {
      await view.navigate("about:blank");
      await setViewport(view, width, height);
    }
    await Bun.write(
      join(output, "environment.json"),
      JSON.stringify(
        {
          sourceFingerprint: manifest.sourceFingerprint,
          engine,
          shaderDiskCache: process.env.WRELA_COLD_SHADERS !== "1",
          viewport: { width, height },
          userAgent: await view.evaluate("navigator.userAgent"),
          hardwareConcurrency: await view.evaluate("navigator.hardwareConcurrency"),
        },
        null,
        2,
      ),
    );
    const result = await browserDeadline(
      run(view, output, errors),
      "verification",
      stopTimedOutBrowser,
      timeoutMs,
    );
    const end = await sourceManifest(manifest.scenario);
    await Bun.write(
      join(output, "run-manifest.json"),
      JSON.stringify(
        {
          startedAt: manifest.startedAt,
          finishedAt: new Date().toISOString(),
          sourceFingerprint: manifest.sourceFingerprint,
          finalSourceFingerprint: end.sourceFingerprint,
          sourceStable: end.sourceFingerprint === manifest.sourceFingerprint,
        },
        null,
        2,
      ),
    );
    // Visual captures keep their frozen fixture bundle and both manifests.
    // Unrelated edits in a shared checkout need not interrupt an art iteration.
    if (end.sourceFingerprint !== manifest.sourceFingerprint && purpose === "measurement")
      throw new Error(
        "Source changed during verification; rerun against the completed checkout for reproducible evidence.",
      );
    return result;
  } catch (error) {
    if (view && !timedOut)
      await Promise.resolve()
        .then(() => view?.screenshot())
        .then((shot) => (shot ? Bun.write(join(output, "failure.png"), shot) : undefined))
        .catch(() => {});
    await Bun.write(
      join(output, "failure.json"),
      JSON.stringify(
        {
          status: "failed",
          sourceFingerprint: manifest.sourceFingerprint,
          error: String(error),
          consoleErrors: errors,
        },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    chrome?.stop();
    process.off("exit", stopOnExit);
    process.off("SIGINT", stopOnInterrupt);
    process.off("SIGTERM", stopOnTerminate);
    try {
      view?.close();
    } catch {
      /* The terminated browser may already be disconnected. */
    }
    if (chrome) await chrome.exited;
    await rm(profile, { recursive: true, force: true });
    await release();
  }
}
/** Chrome's native window bounds include browser chrome; pin the actual content viewport explicitly. */
export async function setViewport(view: BrowserView, width: number, height: number) {
  await view.resize(width, height);
  await view.cdp("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
    screenWidth: width,
    screenHeight: height,
  });
}
export async function waitFor(view: BrowserView, expression: string, timeoutMs = 30000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await view.evaluate<boolean>(expression))) {
    if (Date.now() > deadline) throw new Error(`Browser timeout waiting for ${expression}`);
    await Bun.sleep(100);
  }
}
export async function fixtureServer(source: string, output: string) {
  const entry = join(output, "entry.ts");
  await Bun.write(entry, source);
  const result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    minify: false,
    sourcemap: "inline",
  });
  if (!result.success) throw new Error(result.logs.join("\n"));
  const code = await result.outputs[0].text();
  // Preserve the exact bundled program alongside its source fingerprint and measurements.
  await Bun.write(join(output, "fixture.js"), code);
  return Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(req) {
      if (new URL(req.url).pathname === "/compile-worker.js") {
        const worker = await Bun.build({
          entrypoints: ["packages/compiler/src/worker.ts"],
          target: "browser",
          minify: true,
        });
        if (!worker.success) return new Response(worker.logs.join("\n"), { status: 500 });
        return new Response(await worker.outputs[0].text(), {
          headers: { "Content-Type": "text/javascript" },
        });
      }
      return new URL(req.url).pathname === "/fixture.js"
        ? new Response(code, { headers: { "Content-Type": "text/javascript" } })
        : new Response(
            '<!doctype html><html><head><meta charset="utf-8"><title>Wrela GPU verification</title><style>html,body{margin:0;width:100%;height:100%;background:#151b22}canvas{width:100%;height:100%;display:block}</style></head><body><canvas id="viewport"></canvas><script type="module" src="/fixture.js"></script></body></html>',
            { headers: { "Content-Type": "text/html" } },
          );
    },
  });
}
