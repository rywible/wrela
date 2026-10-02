// The main-thread shim: hands the canvas to the render worker, then forwards resizes and basic
// input to it and shows what it reports. Nothing here runs the program or touches the GPU.
//
// The page's URL decides the mode: ending in `#run` is test mode (a fixed 1920×1080 screen,
// 60 frames, results saved beside the page); anything else plays until the page closes.

import { testMode } from "./abi.ts";
import type { FromWorker, InputEvent, ToWorker } from "./messages.ts";

const TEST_HASH = "#run";

function element<T extends HTMLElement>(selector: string, create: () => T): T {
  const found = document.querySelector<T>(selector);
  if (found !== null) {
    return found;
  }
  const made = create();
  document.body.append(made);
  return made;
}

function main(): void {
  const test = location.hash === TEST_HASH;
  const canvas = element("canvas#wrela-screen", () =>
    Object.assign(document.createElement("canvas"), { id: "wrela-screen" }),
  );
  const statusLine = element("#wrela-status", () =>
    Object.assign(document.createElement("pre"), { id: "wrela-status" }),
  );

  const show = (text: string, state: "ok" | "error") => {
    statusLine.textContent = text;
    statusLine.dataset.state = state;
  };
  // Errors go to the console and the page, never only one of them.
  const fail = (text: string) => {
    console.error(`wrela: ${text}`);
    show(text, "error");
  };

  if (typeof canvas.transferControlToOffscreen !== "function") {
    fail("this browser can't render from a worker: it has no OffscreenCanvas");
    return;
  }
  if (!("gpu" in navigator)) {
    fail("WebGPU isn't available in this browser");
    return;
  }

  const physicalSize = () => {
    const rect = canvas.getBoundingClientRect();
    return {
      width: Math.max(1, Math.round(rect.width * devicePixelRatio)),
      height: Math.max(1, Math.round(rect.height * devicePixelRatio)),
    };
  };
  const size = test ? { width: testMode.WIDTH, height: testMode.HEIGHT } : physicalSize();
  canvas.width = size.width;
  canvas.height = size.height;
  const offscreen = canvas.transferControlToOffscreen();

  const worker = new Worker(new URL("./wrela-worker.js", import.meta.url), {
    type: "module",
    name: "wrela render",
  });
  const send = (message: ToWorker, transfer: Transferable[] = []) =>
    worker.postMessage(message, transfer);
  worker.addEventListener("message", (event: MessageEvent<FromWorker>) => {
    const message = event.data;
    switch (message.type) {
      case "status":
        if (message.text !== "") {
          console.log(`wrela: ${message.text}`);
        }
        show(message.text, "ok");
        break;
      case "error":
        fail(message.text);
        break;
      case "done":
        if (message.ok) {
          console.log(`wrela: ${message.text}`);
          show(message.text, "ok");
        } else {
          fail(`the run failed: ${message.text}`);
        }
        break;
    }
  });
  worker.addEventListener("error", (event) => {
    fail(
      `the render worker failed: ${event.message || "it couldn't start"} (${event.filename}:${event.lineno})`,
    );
  });
  worker.addEventListener("messageerror", () =>
    fail("a message from the render worker couldn't be read"),
  );
  send({ type: "start", canvas: offscreen, base: document.baseURI, test, ...size }, [offscreen]);

  if (test) {
    return; // test mode's screen and inputs are fixed
  }

  const observer = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const box = entry.devicePixelContentBoxSize?.[0];
      const width = box?.inlineSize ?? Math.round(entry.contentRect.width * devicePixelRatio);
      const height = box?.blockSize ?? Math.round(entry.contentRect.height * devicePixelRatio);
      send({ type: "resize", width, height });
    }
  });
  try {
    observer.observe(canvas, { box: "device-pixel-content-box" });
  } catch {
    observer.observe(canvas); // browsers without device-pixel boxes
  }

  const forward = (input: InputEvent) => send({ type: "input", input });
  const pointer = (action: "move" | "down" | "up") => (event: PointerEvent) => {
    forward({
      kind: "pointer",
      action,
      x: event.offsetX * devicePixelRatio,
      y: event.offsetY * devicePixelRatio,
      buttons: event.buttons,
    });
  };
  canvas.addEventListener("pointermove", pointer("move"));
  canvas.addEventListener("pointerdown", pointer("down"));
  canvas.addEventListener("pointerup", pointer("up"));
  window.addEventListener("keydown", (event) => {
    if (!event.repeat) {
      forward({ kind: "key", action: "down", code: event.code });
    }
  });
  window.addEventListener("keyup", (event) =>
    forward({ kind: "key", action: "up", code: event.code }),
  );
  window.addEventListener("focus", () => forward({ kind: "focus", focused: true }));
  window.addEventListener("blur", () => forward({ kind: "focus", focused: false }));
}

main();
