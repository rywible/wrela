import { expect, test } from "bun:test";
import { materialColorFromHex, materialColorToHex } from "./material-controls";

test("material color picker displays linear source albedo in sRGB and saves linear values", () => {
  expect(materialColorToHex([0, 0.18, 1])).toBe("#0076ff");
  const source = materialColorFromHex("#808080");
  expect(source[0]).toBeCloseTo(0.21586, 4);
  expect(materialColorToHex(source)).toBe("#808080");
  expect(materialColorFromHex("#000000")).toEqual([0, 0, 0]);
  expect(materialColorFromHex("#ffffff")).toEqual([1, 1, 1]);
});
