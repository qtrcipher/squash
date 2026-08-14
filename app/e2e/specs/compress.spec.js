/**
 * E2E: compress happy path (docs/03 F2/F6). The app is launched with a
 * folder as argv — the OS "open with" route, which is how E2E gets paths
 * into the app without the un-automatable native file dialog. S2 opens
 * pre-filled → Compress → S4 row reaches done → the output archive is
 * verified on disk and round-tripped through the real CLI.
 */
import assert from "node:assert";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const work = process.env.SQUASH_E2E_WORK;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const cli = path.join(
  repoRoot,
  "target",
  "debug",
  process.platform === "win32" ? "squash.exe" : "squash",
);

assert.ok(work, "SQUASH_E2E_WORK must be set (run via e2e/run.mjs)");

const waitForSheet = (want = true) =>
  browser.waitUntil(async () => (await $(".sheet").isExisting()) === want, {
    timeout: 30000,
    timeoutMsg: `sheet existence never became ${want}`,
  });

describe("compress: open-with folder → S2 → S4 done (docs/03 F2)", () => {
  it("routes the argv folder into a pre-filled compress sheet", async () => {
    await waitForSheet(true);

    // S2 defaults: output name = folder name, location = its parent (F2).
    await expect($("#compress-name")).toHaveValue("report");
    await expect($("#compress-location")).toHaveValue(path.join(work, "inputs"));
    // No validation error → primary action enabled.
    await expect($(".sheet .sheet-actions .cta")).toBeEnabled();
  });

  it("submit → queue row finishes → archive exists and round-trips via the CLI", async () => {
    await (await $(".sheet .sheet-actions .cta")).click();
    await waitForSheet(false);

    const row = await $(".queue-row.state-finished");
    await row.waitForExist({ timeout: 60000 });
    await expect(row.$(".job-label")).toHaveText("report.zip");

    const archive = path.join(work, "inputs", "report.zip");
    assert.ok(fs.existsSync(archive), `output archive exists at ${archive}`);

    // Round-trip: the GUI-built archive must extract cleanly through the CLI.
    const out = path.join(work, "roundtrip");
    execFileSync(cli, ["--no-config", "x", archive, "-o", out]);
    assert.strictEqual(
      fs.readFileSync(path.join(out, "report", "summary.txt"), "utf8"),
      "quarterly squash report\n",
    );
    assert.strictEqual(
      fs.readFileSync(path.join(out, "report", "data", "numbers.csv"), "utf8"),
      "a,b,c\n1,2,3\n",
    );
    // Arabic filename survived the whole stack (docs/03 §6).
    assert.strictEqual(
      fs.readFileSync(path.join(out, "report", "ملاحظات.txt"), "utf8"),
      "ملاحظات الاختبار\n",
    );
  });
});
