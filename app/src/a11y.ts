/**
 * Pure keyboard-navigation helpers (docs/03 §5) — factored out of the
 * components so the sheet focus trap and segmented-control behavior are
 * unit-testable without a DOM. Real screen-reader behavior is verified in the
 * Phase 5 WebDriver E2E, not here.
 */

/**
 * Elements the sheet focus trap cycles through (docs/03 §5: sheets trap
 * focus). Includes `[tabindex]` so radios on a roving tabindex count as
 * interior stops even while they carry `tabindex="-1"`.
 */
export const FOCUSABLE_SELECTOR =
  "a[href], button:not([disabled]), input:not([disabled]), " +
  "select:not([disabled]), textarea:not([disabled]), [tabindex]";

/**
 * Focus-trap wrap decision for a modal sheet. `current` is the index of the
 * focused element within the sheet's focusable list (-1 when focus sits on
 * the sheet container or an unlisted element). Returns the index to move
 * focus to when Tab would otherwise escape the sheet, or null to let the
 * browser's default move proceed (it stays inside the sheet by construction).
 */
export function trapFocusTarget(
  current: number,
  shiftKey: boolean,
  count: number,
): number | null {
  if (count === 0) return null;
  if (shiftKey) return current <= 0 ? count - 1 : null;
  return current < 0 || current === count - 1 ? 0 : null;
}

/**
 * APG radio-group arrow-key navigation for the segmented controls
 * (format/preset/batch mode). Arrows follow the *visual* direction, so
 * Left/Right swap meaning under RTL while Down/Up do not (docs/03 §6).
 * Returns the next index (wrapping), or null for keys the group ignores.
 */
export function nextSegmentIndex(
  current: number,
  key: string,
  rtl: boolean,
  count: number,
): number | null {
  if (count <= 0) return null;
  switch (key) {
    case "ArrowRight":
      return rtl ? (current - 1 + count) % count : (current + 1) % count;
    case "ArrowLeft":
      return rtl ? (current + 1) % count : (current - 1 + count) % count;
    case "ArrowDown":
      return (current + 1) % count;
    case "ArrowUp":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
