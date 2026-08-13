import { describe, expect, it } from "vitest";
import { nextSegmentIndex, trapFocusTarget } from "./a11y";

describe("trapFocusTarget (docs/03 §5: sheets trap focus)", () => {
  it("lets ordinary interior moves proceed", () => {
    expect(trapFocusTarget(1, false, 4)).toBeNull();
    expect(trapFocusTarget(1, true, 4)).toBeNull();
  });

  it("wraps Tab from the last stop back to the first", () => {
    expect(trapFocusTarget(3, false, 4)).toBe(0);
  });

  it("wraps Shift+Tab from the first stop to the last", () => {
    expect(trapFocusTarget(0, true, 4)).toBe(3);
  });

  it("moves into the list when focus is outside it", () => {
    // Focus on the sheet container: Tab enters at the first stop,
    // Shift+Tab enters at the last.
    expect(trapFocusTarget(-1, false, 4)).toBe(0);
    expect(trapFocusTarget(-1, true, 4)).toBe(3);
  });

  it("never escapes an empty sheet", () => {
    expect(trapFocusTarget(-1, false, 0)).toBeNull();
    expect(trapFocusTarget(-1, true, 0)).toBeNull();
  });

  it("single-stop sheet traps in place", () => {
    expect(trapFocusTarget(0, false, 1)).toBe(0);
    expect(trapFocusTarget(0, true, 1)).toBe(0);
  });
});

describe("nextSegmentIndex (APG radio-group arrows on segmented controls)", () => {
  it("Right/Down move forward and wrap in LTR", () => {
    expect(nextSegmentIndex(0, "ArrowRight", false, 4)).toBe(1);
    expect(nextSegmentIndex(3, "ArrowRight", false, 4)).toBe(0);
    expect(nextSegmentIndex(3, "ArrowDown", false, 4)).toBe(0);
  });

  it("Left/Up move backward and wrap in LTR", () => {
    expect(nextSegmentIndex(0, "ArrowLeft", false, 4)).toBe(3);
    expect(nextSegmentIndex(2, "ArrowLeft", false, 4)).toBe(1);
    expect(nextSegmentIndex(0, "ArrowUp", false, 4)).toBe(3);
  });

  it("Left/Right follow the visual direction under RTL", () => {
    // In RTL the row is mirrored: ArrowLeft is visually "forward".
    expect(nextSegmentIndex(0, "ArrowLeft", true, 4)).toBe(1);
    expect(nextSegmentIndex(0, "ArrowRight", true, 4)).toBe(3);
    expect(nextSegmentIndex(2, "ArrowDown", true, 4)).toBe(3);
  });

  it("Home/End jump to the ends", () => {
    expect(nextSegmentIndex(2, "Home", false, 4)).toBe(0);
    expect(nextSegmentIndex(2, "End", true, 4)).toBe(3);
  });

  it("ignores unhandled keys and empty groups", () => {
    expect(nextSegmentIndex(0, "Tab", false, 4)).toBeNull();
    expect(nextSegmentIndex(0, "Enter", false, 4)).toBeNull();
    expect(nextSegmentIndex(0, "ArrowRight", false, 0)).toBeNull();
  });
});
