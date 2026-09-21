import { dlopen, type Pointer, read } from "bun:ffi";
import { constants } from "node:fs";
import { lstat, open } from "node:fs/promises";
import { constants as osConstants } from "node:os";

function loadFlock() {
  if (process.platform !== "darwin" && process.platform !== "linux")
    throw new Error("Local workspace publication requires macOS or Linux with glibc file locking.");
  const errnoSymbol = process.platform === "darwin" ? "__error" : "__errno_location";
  const library = dlopen(process.platform === "darwin" ? "/usr/lib/libSystem.B.dylib" : "libc.so.6", {
    flock: { args: ["i32", "i32"], returns: "i32" },
    [errnoSymbol]: { args: [], returns: "ptr" },
  });
  return (fd: number): boolean => {
    // LOCK_EX | LOCK_NB: never block the JS thread while another process publishes.
    if (library.symbols.flock(fd, 2 | 4) === 0) return true;
    const pointer = library.symbols[errnoSymbol]();
    if (!pointer) throw new Error("Unable to read workspace lock error.");
    const errno = read.i32(pointer as Pointer);
    if (
      errno === osConstants.errno.EWOULDBLOCK ||
      errno === osConstants.errno.EAGAIN ||
      errno === osConstants.errno.EINTR
    )
      return false;
    throw new Error(`Unable to acquire workspace publication lock (OS error ${errno}).`);
  };
}

let tryLock: ReturnType<typeof loadFlock> | undefined;

/** The caller checks containment of the lock's parent before opening it. The
 * lock file must stay in place: unlinking it could give waiting writers different
 * inodes to lock. The OS releases this advisory lock on close or process death;
 * there are no stale time-based leases to steal from a slow or suspended writer. */
export async function withWorkspacePublicationLock<T>(
  path: string,
  publish: () => Promise<T>,
  timeoutMs = 10_000,
): Promise<T> {
  tryLock ??= loadFlock();
  const handle = await open(
    path,
    constants.O_RDWR | constants.O_CREAT | constants.O_NOFOLLOW | constants.O_NONBLOCK,
    0o600,
  );
  try {
    const info = await handle.stat();
    if (!info.isFile() || info.nlink !== 1) throw new Error("Invalid workspace publication lock file.");
    const deadline = performance.now() + timeoutMs;
    while (!tryLock(handle.fd)) {
      const remaining = deadline - performance.now();
      if (remaining <= 0) throw new Error("Workspace is busy publishing. Retry saving when it finishes.");
      await Bun.sleep(Math.min(25, remaining));
    }
    const current = await lstat(path);
    if (current.isSymbolicLink() || current.dev !== info.dev || current.ino !== info.ino)
      throw new Error("Workspace publication lock was replaced. Retry saving.");
    return await publish();
  } finally {
    await handle.close();
  }
}
