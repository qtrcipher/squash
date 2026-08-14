/**
 * E2E scenario runner (docs/05 §6): builds the real app + CLI, then runs one
 * wdio invocation per scenario — each is a fresh app launch with its own
 * throwaway store (SQUASH_STORE_DIR) and its own argv, because the OS
 * "open with" routing we use to bypass native dialogs (docs/03 F6) only
 * happens at process start.
 *
 *   npm run test:e2e        build everything, run all scenarios
 *   npm run test:e2e:run    skip the build step (binaries must exist)
 *
 * Scenarios (specs/):
 *   launch    no argv, no seeded settings → F1 first-launch sheet → S1 empty
 *             state, keyboard smoke, S6 language switch → RTL + screenshots
 *   compress  argv = a folder → S2 pre-filled → Compress → S4 done → output
 *             archive verified on disk and round-tripped through the CLI
 *   extract   argv = a loose-files zip (built by the CLI) → S3 → Extract →
 *             S4 done → files verified under <dest>/<archive-stem>/ (F3)
 *
 * Note: close any running Squash instance first — the single-instance
 * plugin would forward our argv to it instead of starting a test app.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const appDir = path.resolve(here, "..");
const repoRoot = path.resolve(appDir, "..");
const artifacts = path.join(here, "artifacts");

const exe = process.platform === "win32" ? ".exe" : "";
const appBinary = path.join(repoRoot, "target", "debug", `squash-app${exe}`);
const cliBinary = path.join(repoRoot, "target", "debug", `squash${exe}`);
// spawnSync without a shell needs the .cmd shims on Windows.
const npmCmd = process.platform === "win32" ? "npm.cmd" : "npm";
const npxCmd = process.platform === "win32" ? "npx.cmd" : "npx";

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, { stdio: "inherit", ...opts });
  if (result.status !== 0) {
    throw new Error(`${cmd} ${args.join(" ")} failed with exit ${result.status}`);
  }
}

/** Full settings.toml matching squash-core's schema (version 1): skips the
 * first-launch sheet and the drop-zone hint, pins a deterministic theme. */
function seedSettings(storeDir) {
  const configDir = path.join(storeDir, "config");
  fs.mkdirSync(configDir, { recursive: true });
  fs.writeFileSync(
    path.join(configDir, "settings.toml"),
    [
      'version = 1',
      'language = "en"',
      'theme = "light"',
      'default_preset = "builtin:balanced"',
      'default_format = "zip"',
      "update_check_opt_in = false",
      "activation_counter_opt_in = false",
      'release_channel = "stable"',
      "first_launch_done = true",
      "drop_zone_hint_dismissed = true",
      "debug_logging = false",
      "crash_reporting = false",
      "",
      "[extract]",
      'dest_policy = "same_folder"',
      'loose_files_policy = "new_folder"',
      "",
    ].join("\n"),
  );
}

/** Per-scenario fixtures; returns the argv for the app launch. */
function prepareScenario(name, workDir, storeDir) {
  switch (name) {
    case "launch":
      // Fresh store on purpose: the S7 first-launch sheet is part of the test.
      return [];
    case "compress": {
      seedSettings(storeDir);
      const input = path.join(workDir, "inputs", "report");
      fs.mkdirSync(path.join(input, "data"), { recursive: true });
      fs.writeFileSync(path.join(input, "summary.txt"), "quarterly squash report\n");
      fs.writeFileSync(path.join(input, "data", "numbers.csv"), "a,b,c\n1,2,3\n");
      // An Arabic filename through the real compress path (docs/03 §6).
      fs.writeFileSync(path.join(input, "ملاحظات.txt"), "ملاحظات الاختبار\n");
      return [input];
    }
    case "extract": {
      seedSettings(storeDir);
      const payload = path.join(workDir, "payload");
      fs.mkdirSync(payload, { recursive: true });
      fs.writeFileSync(path.join(payload, "hello.txt"), "hello squash\n");
      fs.writeFileSync(path.join(payload, "notes.md"), "# notes\n");
      // Loose files at the archive root → F3 "new folder named after the
      // archive" rule must kick in on extraction.
      const zipPath = path.join(workDir, "downloads", "loose-bundle.zip");
      fs.mkdirSync(path.dirname(zipPath), { recursive: true });
      run(cliBinary, [
        "--no-config",
        "c",
        path.join(payload, "hello.txt"),
        path.join(payload, "notes.md"),
        "-o",
        zipPath,
      ]);
      return [zipPath];
    }
    default:
      throw new Error(`unknown scenario ${name}`);
  }
}

function runScenario(name) {
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), `squash-e2e-${name}-`));
  const storeDir = path.join(workDir, "store");
  fs.mkdirSync(artifacts, { recursive: true });
  const appArgs = prepareScenario(name, workDir, storeDir);

  console.log(`\n=== e2e scenario: ${name} (work: ${workDir}) ===`);
  const result = spawnSync(
    npxCmd,
    ["wdio", "run", path.join(here, "wdio.conf.js"), "--spec", path.join(here, "specs", `${name}.spec.js`)],
    {
      cwd: appDir,
      stdio: "inherit",
      env: {
        ...process.env,
        SQUASH_STORE_DIR: storeDir,
        SQUASH_E2E_APP_ARGS: JSON.stringify(appArgs),
        SQUASH_E2E_WORK: workDir,
        SQUASH_E2E_ARTIFACTS: artifacts,
      },
    },
  );
  const ok = result.status === 0;
  if (ok) {
    // Only keep work dirs of failed scenarios for post-mortem.
    fs.rmSync(workDir, { recursive: true, force: true });
  } else {
    console.error(`scenario ${name} FAILED — fixtures kept at ${workDir}`);
  }
  return ok;
}

// 1. Build: real frontend (no snapshot mock), app with the e2e feature, CLI
//    for the round-trip verification.
const args = process.argv.slice(2);
const skipBuild = args.includes("--skip-build");
if (!skipBuild) {
  console.log("=== e2e build: frontend ===");
  run(npmCmd, ["run", "build"], { cwd: appDir });
  console.log("=== e2e build: squash-app (feature e2e) + squash-cli ===");
  // TAURI_CONFIG merges at compile time (tauri-codegen): the e2e build gets
  // `withGlobalTauri` so the service's window-focus checks hit a fast
  // "unknown plugin" answer instead of a 5s timeout per command. The
  // production config (tauri.conf.json) is NOT touched.
  run("cargo", ["build", "-p", "squash-app", "--features", "e2e"], {
    cwd: repoRoot,
    env: { ...process.env, TAURI_CONFIG: '{"app":{"withGlobalTauri":true}}' },
  });
  run("cargo", ["build", "-p", "squash-cli"], { cwd: repoRoot });
}

const scenarios = args.filter((a) => a !== "--skip-build");
const toRun = scenarios.length > 0 ? scenarios : ["launch", "compress", "extract"];
const results = toRun.map((name) => ({ name, ok: runScenario(name) }));

console.log("\n=== e2e summary ===");
for (const { name, ok } of results) {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}`);
}
process.exit(results.every((r) => r.ok) ? 0 : 1);
