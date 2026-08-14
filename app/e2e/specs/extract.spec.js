/**
 * E2E: extract happy path (docs/03 F3/F6). The app is launched with a zip
 * (built by the real CLI, loose files at the root) as argv → S3 opens with
 * the destination pre-filled → Extract → S4 row done → the anti-desktop-
 * explosion rule (loose files into <dest>/<archive-stem>/) is verified on
 * disk byte-for-byte.
 */
import assert from "node:assert";
import fs from "node:fs";
import path from "node:path";

const work = process.env.SQUASH_E2E_WORK;
assert.ok(work, "SQUASH_E2E_WORK must be set (run via e2e/run.mjs)");

const waitForSheet = (want = true) =>
  browser.waitUntil(async () => (await $(".sheet").isExisting()) === want, {
    timeout: 30000,
    timeoutMsg: `sheet existence never became ${want}`,
  });

describe("extract: open-with archive → S3 → S4 done (docs/03 F3)", () => {
  it("routes the argv archive into the extract sheet", async () => {
    await waitForSheet(true);

    // S3 default destination: the folder containing the archive (F3).
    await expect($("#extract-destination")).toHaveValue(path.join(work, "downloads"));
    await expect($(".sheet .sheet-actions .cta")).toBeEnabled();
  });

  it("submit → queue row finishes → loose files land in a folder named after the archive", async () => {
    await (await $(".sheet .sheet-actions .cta")).click();
    await waitForSheet(false);

    const row = await $(".queue-row.state-finished");
    await row.waitForExist({ timeout: 60000 });
    await expect(row.$(".job-label")).toHaveText("loose-bundle.zip");

    const dest = path.join(work, "downloads", "loose-bundle");
    assert.strictEqual(fs.readFileSync(path.join(dest, "hello.txt"), "utf8"), "hello squash\n");
    assert.strictEqual(fs.readFileSync(path.join(dest, "notes.md"), "utf8"), "# notes\n");
  });
});
