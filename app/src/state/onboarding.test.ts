import { describe, expect, it } from "vitest";
import type { Settings } from "../api";
import {
  completeFirstLaunch,
  dismissDropHint,
  shouldShowDropHint,
  shouldShowWelcome,
} from "./onboarding";

function settings(overrides: Partial<Settings> = {}): Settings {
  return {
    version: 1,
    language: "en",
    theme: "system",
    default_preset: "builtin:balanced",
    default_format: "zip",
    extract: { dest_policy: "same_folder", loose_files_policy: "new_folder" },
    update_check_opt_in: false,
    activation_counter_opt_in: false,
    first_launch_done: false,
    drop_zone_hint_dismissed: false,
    debug_logging: false,
    ...overrides,
  };
}

describe("onboarding gating (docs/03 F1)", () => {
  it("S7 shows on first launch and never again once done", () => {
    expect(shouldShowWelcome(null)).toBe(false); // settings not loaded yet
    expect(shouldShowWelcome(settings())).toBe(true);
    expect(shouldShowWelcome(settings({ first_launch_done: true }))).toBe(false);
  });

  it("completeFirstLaunch flips only the flag (skippable = done)", () => {
    const before = settings({ language: "ar", theme: "dark" });
    const after = completeFirstLaunch(before);
    expect(after.first_launch_done).toBe(true);
    // Language/theme choices made on S7 survive the Continue.
    expect(after.language).toBe("ar");
    expect(after.theme).toBe("dark");
    expect(before.first_launch_done).toBe(false); // immutable update
  });

  it("drop-zone hint shows after first launch until dismissed", () => {
    expect(shouldShowDropHint(null, false)).toBe(false);
    expect(shouldShowDropHint(settings(), false)).toBe(true);
    expect(shouldShowDropHint(settings({ drop_zone_hint_dismissed: true }), false)).toBe(false);
  });

  it("drop-zone hint never renders under an open sheet", () => {
    expect(shouldShowDropHint(settings(), true)).toBe(false);
  });

  it("dismissDropHint flips only the flag", () => {
    const before = settings({ first_launch_done: true });
    const after = dismissDropHint(before);
    expect(after.drop_zone_hint_dismissed).toBe(true);
    expect(after.first_launch_done).toBe(true);
    expect(before.drop_zone_hint_dismissed).toBe(false);
  });
});
