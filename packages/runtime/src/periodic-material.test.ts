import { expect, test } from "bun:test";
import { materialSchema } from "@wrela/model";

import { renderMaterial } from "./scene-host";

test("authored weave survives serialization and maps to the compiled renderer material", () => {
  const authored = materialSchema.parse({
    id: "cloth",
    name: "Cloth",
    schemaVersion: 1,
    kind: "material",
    dependencies: [],
    pattern: "weave",
    color: [0.1, 0.2, 0.3],
    secondary: [0.7, 0.8, 0.9],
    roughness: 0.8,
    metallic: 0,
    normalStrength: 0,
    scale: 32,
    domain: "world",
  });
  const restored = materialSchema.parse(JSON.parse(JSON.stringify(authored)));
  expect(renderMaterial(restored)).toMatchObject({ pattern: 4, scale: 32, domain: "world" });
});
