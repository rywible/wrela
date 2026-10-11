import { expect, spyOn, test } from "bun:test";
import { clockImport, printed } from "../src/imports.ts";

for (const shared of [false, true]) {
  test(`the clock writes to ${shared ? "shared" : "unshared"} memory before and after growth`, () => {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 2, shared });
    const clock = clockImport(memory);
    const now = spyOn(performance, "now").mockReturnValue(1234);
    try {
      const expected = (performance.timeOrigin + 1234) / 1000;
      clock(8);
      clock(16);
      expect(new DataView(memory.buffer).getFloat64(8, true)).toBe(expected);
      expect(new DataView(memory.buffer).getFloat64(16, true)).toBe(expected);
      expect(() => clock(65_532)).toThrow(RangeError);
      memory.grow(1);
      clock(65_532);
      expect(new DataView(memory.buffer).getFloat64(65_532, true)).toBe(expected);
      expect(() => clock(131_068)).toThrow(RangeError);
    } finally {
      now.mockRestore();
    }
  });
}

test("consecutive printed lines decode independently, keeping replacement and BOM behavior", () => {
  const memory = new WebAssembly.Memory({ initial: 1, maximum: 1, shared: true });
  const bytes = new Uint8Array(memory.buffer);
  bytes.set([0xc3], 8); // A partial UTF-8 sequence must not carry into the next print.
  bytes.set(new TextEncoder().encode("\uFEFFé"), 16);
  const log = spyOn(console, "log").mockImplementation(() => {});
  try {
    expect(printed(memory, 8, 1)).toBe("\uFFFD");
    expect(printed(memory, 16, 5)).toBe("é");
    expect(printed(memory, 16, 0)).toBe("");
    expect(log.mock.calls.map(([line]) => line)).toEqual(["wrela: \uFFFD", "wrela: é", "wrela: "]);
    expect(() => printed(memory, 65_535, 2)).toThrow(RangeError);
  } finally {
    log.mockRestore();
  }
});
