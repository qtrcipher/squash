import { describe, expect, it } from "vitest";
import { isolate } from "./format";

/**
 * docs/03 §6: data values interpolated into localized prose are wrapped in
 * LTR bidi isolates so mixed-direction strings never scramble.
 */
describe("isolate", () => {
  it("wraps the value in LRI … PDI", () => {
    expect(isolate("تقارير-المالية.zip")).toBe("\u2066تقارير-المالية.zip\u2069");
    expect(isolate("1.2 GB")).toBe("\u20661.2 GB\u2069");
  });
});
