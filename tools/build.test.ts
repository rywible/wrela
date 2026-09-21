import { expect, test } from "bun:test";
import { snapshotKey } from "./build";

const bytes = (source: string) => new TextEncoder().encode(source);
test("offline snapshot updates when only the fixed-name worker changes", () => {
  const page = { path: "index.html", bytes: bytes('<script src="app-unchanged.js"></script>') };
  expect(snapshotKey([page, { path: "compile-worker.js", bytes: bytes("self.version=1") }])).not.toBe(
    snapshotKey([page, { path: "compile-worker.js", bytes: bytes("self.version=2") }]),
  );
});
test("offline snapshot identity is deterministic across directory traversal order", () => {
  const files = [
    { path: "index.html", bytes: bytes("page") },
    { path: "worker.js", bytes: bytes("worker") },
  ];
  expect(snapshotKey(files)).toBe(snapshotKey([...files].reverse()));
});
test("renamed assets invalidate offline snapshot as well as changed content", () => {
  expect(snapshotKey([{ path: "old.js", bytes: bytes("same") }])).not.toBe(
    snapshotKey([{ path: "new.js", bytes: bytes("same") }]),
  );
});
