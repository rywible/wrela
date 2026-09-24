import { spawn } from "node:child_process";

/** A dedicated process group lets cleanup reach GPU children even if Chrome hangs. */
export function ownedBrowserProcess(executable: string, args: string[]) {
  const child = spawn(executable, args, {
    detached: process.platform !== "win32",
    stdio: "ignore",
  });
  const exited = new Promise<void>((resolve) => {
    child.once("exit", () => resolve());
    child.once("error", () => resolve());
  });
  let stopped = false;
  return {
    pid: child.pid,
    exited,
    stop() {
      if (stopped) return;
      stopped = true;
      if (!child.pid) return;
      try {
        if (process.platform === "win32") child.kill("SIGKILL");
        else process.kill(-child.pid, "SIGKILL");
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
      }
    },
  };
}

export async function browserDeadline<T>(
  operation: Promise<T>,
  label: string,
  onTimeout: () => void,
  milliseconds = 30_000,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          onTimeout();
          reject(new Error(`Browser ${label} exceeded ${milliseconds} ms; stopped its process group.`));
        }, milliseconds);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
