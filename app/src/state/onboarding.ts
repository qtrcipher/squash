import type { Settings } from "../api";

/**
 * S7 first-launch + S1 drop-zone hint gating (docs/03 F1). Pure helpers over
 * the persisted settings flags so the sheet/hint logic is testable without a
 * webview; App.tsx wires them to `api.setSettings`.
 */

/** S7 shows once, on first launch only, and never again (docs/03 §2 S7). */
export function shouldShowWelcome(settings: Settings | null): boolean {
  return settings !== null && !settings.first_launch_done;
}

/**
 * The single contextual hint (docs/03 F1: "no tutorial — the drop zone
 * explains itself"). Shown after first launch until dismissed or the first
 * drop; a sheet being open suppresses it so it never sits under a modal.
 */
export function shouldShowDropHint(settings: Settings | null, sheetOpen: boolean): boolean {
  return settings !== null && !sheetOpen && !settings.drop_zone_hint_dismissed;
}

/** Continue/dismiss on S7 both count as "done" — it is skippable (docs/03 F1). */
export function completeFirstLaunch(settings: Settings): Settings {
  return { ...settings, first_launch_done: true };
}

export function dismissDropHint(settings: Settings): Settings {
  return { ...settings, drop_zone_hint_dismissed: true };
}
