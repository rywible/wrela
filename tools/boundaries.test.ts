import { expect, test } from "bun:test";
import { importedSpecifiers } from "./boundaries";

test("boundary analysis includes side effects, re-exports, dynamic imports and require without matching comments", () => {
  expect(
    importedSpecifiers(`// import "ignored";
    import "@wrela/model/private";
    import type { Project } from "@wrela/model";
    export * from "@wrela/compiler";
    const worker = import("@wrela/compiler/client");
    require("node:fs");`),
  ).toEqual(["@wrela/model/private", "@wrela/model", "@wrela/compiler", "@wrela/compiler/client", "node:fs"]);
});
