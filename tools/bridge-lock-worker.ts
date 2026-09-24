/** Separate-process fixture for bridge-lock.test.ts. */
import { join } from "node:path";
import { referenceProject } from "@wrela/examples";
import { WorkspaceBridge } from "./bridge";
import { withWorkspacePublicationLock } from "./bridge-lock";

const [mode, root, name, expectedKey, pause] = process.argv.slice(2);
function receive(command: string) {
  return new Promise<void>((resolve) => {
    const listener = (message: unknown) => {
      if (message !== command) return;
      process.off("message", listener);
      resolve();
    };
    process.on("message", listener);
  });
}

try {
  if (mode === "hold") {
    await withWorkspacePublicationLock(join(root, ".wrela", "PUBLISH.lock"), async () => {
      const release = receive("release");
      process.send?.({ event: "locked" });
      await release;
    });
    process.send?.({ event: "released" });
  } else if (mode === "save" || mode === "probe-save") {
    const bridge = new WorkspaceBridge(root);
    await bridge.initialize();
    const read = bridge.read.bind(bridge);
    let reads = 0;
    bridge.read = async () => {
      const value = await read();
      if (++reads === 2) {
        const release = pause === "pause" ? receive("release") : Promise.resolve();
        process.send?.({ event: "final-read" });
        await release;
      }
      return value;
    };
    const start = receive("start");
    process.send?.({ event: "ready" });
    await start;
    if (mode === "probe-save") {
      const resume = receive("continue");
      const blocked = await withWorkspacePublicationLock(
        join(root, ".wrela", "PUBLISH.lock"),
        async () => false,
        50,
      ).catch((error) => {
        if (String(error).includes("busy publishing")) return true;
        throw error;
      });
      process.send?.({ event: "probe", status: blocked ? "held" : "free" });
      await resume;
    }
    try {
      const result = await bridge.save({ ...referenceProject(), name }, expectedKey || null);
      process.send?.({ event: "result", status: "fulfilled", ...result });
    } catch (error) {
      process.send?.({ event: "result", status: "rejected", error: String(error) });
    }
  } else {
    throw new Error("Unknown bridge test worker mode");
  }
} catch (error) {
  process.send?.({ event: "error", error: String(error) });
  process.exitCode = 1;
} finally {
  process.disconnect?.();
}
