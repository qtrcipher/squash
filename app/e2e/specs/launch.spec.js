/**
 * E2E: first launch (docs/03 F1) → S1 empty state, keyboard smoke, RTL.
 * Runs against a completely fresh store (no seeded settings), so S7 shows.
 *
 * Driver notes (embedded WebDriver / WKWebView — see e2e/README.md):
 * - Key events are synthetic (isTrusted=false), so Tab's focus traversal
 *   default action never runs — Tab-reachability is NOT asserted here.
 *   What IS asserted: focus moves into a sheet on open, Esc closes it, and
 *   focus returns to the trigger (the app's own keyboard contract).
 * - Native <select> popups can't be driven; the language switch sets value +
 *   dispatches a real bubbling change event, exercising the app's actual
 *   onChange → i18n → dir=rtl path.
 */
import assert from "node:assert";
import path from "node:path";

const artifacts = process.env.SQUASH_E2E_ARTIFACTS ?? path.join(process.cwd(), "artifacts");

const sheetExists = () => $(".sheet").isExisting();
const waitForSheet = (want = true) =>
  browser.waitUntil(async () => (await sheetExists()) === want, {
    timeout: 30000,
    timeoutMsg: `sheet existence never became ${want}`,
  });

describe("launch: F1 first launch → S1 (docs/03 §3)", () => {
  it("shows the S7 first-launch sheet, then the S1 empty state after Continue", async () => {
    await waitForSheet(true);
    await expect($("#onboarding-language")).toBeExisting();

    await (await $(".sheet .sheet-actions .cta")).click();
    await waitForSheet(false);

    // S1: the drop zone illustration IS the empty state (docs/03 §2 S1).
    await expect($(".drop-zone")).toBeExisting();
    // The empty queue section collapses entirely (docs/03 §2 S4).
    assert.strictEqual(await $(".queue").isExisting(), false);
    // The one-time drop-zone hint shows on first launch (F1).
    await expect($(".hint")).toBeExisting();
  });

  it("keyboard: focus enters sheets on open, Esc closes, focus returns (docs/03 §5)", async () => {
    const settingsButton = await $("header.toolbar .button");
    await settingsButton.click();
    await waitForSheet(true);

    // Focus moved into the sheet on open (S6 autofocuses the language field).
    const focusInside = await browser.execute(
      () => document.querySelector(".sheet")?.contains(document.activeElement) ?? false,
    );
    assert.ok(focusInside, "focus must move into the sheet on open");

    await browser.keys(["Escape"]);
    await waitForSheet(false);

    // Focus returns to the trigger that opened the sheet.
    const backOnSettings = await browser.execute(
      () => document.activeElement === document.querySelector("header.toolbar .button"),
    );
    assert.ok(backOnSettings, "focus must return to the Settings button after Esc");
    await expect($(".drop-zone")).toBeExisting();
  });

  it("RTL: switching language in S6 flips dir=rtl on <html> (docs/03 §6)", async () => {
    await (await $("header.toolbar .button")).click();
    await waitForSheet(true);

    // Native <select> popups are not WebDriver-drivable on WKWebView; set the
    // value and fire the bubbling change event React listens for.
    await browser.execute(() => {
      const select = document.querySelector("#settings-language");
      select.value = "ar";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await browser.waitUntil(
      async () => (await browser.execute(() => document.documentElement.dir)) === "rtl",
      { timeout: 5000, timeoutMsg: "document.dir did not flip to rtl" },
    );
    assert.strictEqual(await browser.execute(() => document.documentElement.lang), "ar");

    // RTL visual-check artifacts (reviewed like the snapshot baselines).
    await browser.saveScreenshot(path.join(artifacts, "rtl-settings-ar.png"));
    await browser.keys(["Escape"]);
    await waitForSheet(false);
    await expect($(".drop-zone")).toBeExisting();
    await browser.saveScreenshot(path.join(artifacts, "rtl-main-ar.png"));
  });
});
