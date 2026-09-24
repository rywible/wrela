import { expect, test } from "bun:test";
import { PipelineVariants } from "./pipeline-variants";

test("optional variants compile once on demand and never publish across device replacement", async () => {
  const errors: unknown[] = [],
    variants = new PipelineVariants<string, string>((e) => errors.push(e));
  let calls = 0,
    finish!: (value: string) => void;
  variants.define("used", () => {
    calls++;
    return new Promise((resolve) => {
      finish = resolve;
    });
  });
  variants.define("unused", async () => {
    throw Error("must stay deferred");
  });
  expect(calls).toBe(0);
  expect(variants.get("used")).toBeUndefined();
  expect(variants.get("used")).toBeUndefined();
  await Bun.sleep(0);
  expect(calls).toBe(1);
  variants.clear();
  variants.define("used", async () => "new device");
  finish("old device");
  await Bun.sleep(0);
  expect(variants.get("used")).toBeUndefined();
  await Bun.sleep(0);
  expect(variants.get("used")).toBe("new device");
  expect(errors).toEqual([]);
  variants.define("failed", async () => {
    throw Error("compilation failed");
  });
  variants.get("failed");
  await Bun.sleep(0);
  variants.get("failed");
  await Bun.sleep(0);
  expect(errors).toHaveLength(1);
  variants.define("cancelled", async () => {
    throw Error("Disposed device was used");
  });
  variants.get("cancelled");
  variants.clear();
  await Bun.sleep(0);
  expect(errors).toHaveLength(1);
});

test("measurement barrier waits for requested compilation without warming unused variants", async () => {
  const variants = new PipelineVariants<string, number>(() => {});
  let release!: (n: number) => void,
    unused = false;
  variants.define(
    "active",
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  variants.define("unused", async () => {
    unused = true;
    return 2;
  });
  variants.get("active");
  let settled = false;
  const barrier = variants.settle().then(() => {
    settled = true;
  });
  await Bun.sleep(0);
  expect(settled).toBe(false);
  release(1);
  await barrier;
  expect(variants.get("active")).toBe(1);
  expect(unused).toBe(false);
});
