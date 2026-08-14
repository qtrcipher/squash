/**
 * Snapshot-harness mock for the Tauri bridge. Wired in via vite resolve.alias
 * (only when SNAPSHOT_MOCK=1 — see vite.config.ts), replacing:
 *   @tauri-apps/api/core    → invoke
 *   @tauri-apps/api/event   → listen
 *   @tauri-apps/api/webview → getCurrentWebview
 *   @tauri-apps/plugin-dialog → open
 *
 * The REAL App renders against this mock, so screens, states, RTL switching
 * and theming are exercised end-to-end without a backend. The scenario comes
 * from URL params:
 *   ?screen=s1|s2|s3|s4|s6|s7|d3&state=<state>&lang=en|ar&theme=light|dark
 *
 * Everything returned here is deterministic (fixed timestamps, fixed byte
 * sizes, fixed names — including an Arabic filename for bidi review) so
 * captures are byte-stable on a given platform.
 */
import type {
  ClassifiedPaths,
  JobEntry,
  SettingsResponse,
  UpdateInfo,
} from "../api";
import type { CrashReportingConfig } from "../crashReporting";

const params = new URLSearchParams(window.location.search);

export const scenario = {
  screen: params.get("screen") ?? "s1",
  state: params.get("state") ?? "default",
  lang: params.get("lang") === "ar" ? ("ar" as const) : ("en" as const),
  theme: params.get("theme") === "dark" ? ("dark" as const) : ("light" as const),
};

const STARTED_AT = "2026-08-14T12:00:00.000Z";
const never = <T>() => new Promise<T>(() => undefined);

function job(partial: Partial<JobEntry> & Pick<JobEntry, "id" | "label" | "status">): JobEntry {
  return {
    operation: "compress",
    inputs: [],
    destination: `/mock/output/${partial.label}`,
    format: "zip",
    preset: "builtin:balanced",
    totalBytesEstimate: null,
    bytesDone: 0,
    entriesDone: 0,
    inBytes: null,
    outBytes: null,
    durationMs: null,
    errorCode: null,
    startedAt: STARTED_AT,
    ...partial,
  };
}

/** S4 queue fixtures (docs/03 §2 S4). */
function queueEntries(): Promise<JobEntry[]> {
  if (scenario.screen !== "s4") return Promise.resolve([]);
  switch (scenario.state) {
    case "restoring":
      // Launch skeleton: the restore never lands within the capture.
      return never();
    case "running":
      return Promise.resolve([
        job({
          id: "job-running",
          label: "backup-2026-08.zip",
          status: "running",
          totalBytesEstimate: 1_073_741_824, // 1 GB
          bytesDone: 666_894_336, // 636 MB → 62%
          entriesDone: 142,
        }),
        job({ id: "job-queued", label: "photos.tar.zst", status: "queued", format: "tar.zst" }),
      ]);
    case "success":
      return Promise.resolve([
        job({
          id: "job-done-compress",
          label: "تقارير-المالية.zip",
          status: "finished",
          inBytes: 1_258_291_200, // 1.2 GB
          outBytes: 662_700_032, // 632 MB → saved 47%
          durationMs: 8_400,
        }),
        job({
          id: "job-done-extract",
          label: "photos.zip",
          operation: "extract",
          status: "finished",
          durationMs: 2_100,
        }),
      ]);
    case "failed":
      return Promise.resolve([
        job({
          id: "job-failed",
          label: "family-videos.7z",
          format: "7z",
          status: "failed",
          errorCode: "disk_full",
        }),
        job({
          id: "job-done-extract",
          label: "photos.zip",
          operation: "extract",
          status: "finished",
          durationMs: 2_100,
        }),
      ]);
    default: // "empty" — the section collapses entirely (docs/03 §2 S4)
      return Promise.resolve([]);
  }
}

/** OS "open with" handoff drives S2/S3 onto the screen through App itself. */
function pendingOpenPaths(): string[] {
  if (scenario.screen === "s2") return ["/mock/Documents/تقارير-2026"];
  if (scenario.screen === "s3") return ["/mock/Downloads/أرشيف-الصور.zip"];
  return [];
}

function classify(): ClassifiedPaths {
  if (scenario.screen === "s2") {
    return {
      items: [{ path: "/mock/Documents/تقارير-2026", isDir: true }],
      archives: [],
      totalBytes: 1_258_291_200,
    };
  }
  return {
    items: [],
    archives: [{ path: "/mock/Downloads/أرشيف-الصور.zip", format: "zip" }],
    totalBytes: 662_700_032,
  };
}

function settingsResponse(): SettingsResponse {
  return {
    settings: {
      version: 1,
      language: scenario.lang,
      theme: scenario.theme,
      default_preset: "builtin:balanced",
      default_format: "zip",
      extract: { dest_policy: "same_folder", loose_files_policy: "new_folder" },
      update_check_opt_in: scenario.screen === "d3",
      activation_counter_opt_in: false,
      release_channel: "stable",
      first_launch_done: scenario.screen !== "s7",
      drop_zone_hint_dismissed: scenario.screen !== "s1" || scenario.state !== "empty",
      debug_logging: false,
      crash_reporting: false,
    },
    writable: true,
    warning: null,
  };
}

function updateInfo(): UpdateInfo | null {
  if (scenario.screen !== "d3") return null;
  return {
    version: "0.2.0",
    notes: "Faster zstd preset, improved queue restore, Arabic UI fixes.",
    date: "2026-08-01",
  };
}

export function invoke<T>(cmd: string): Promise<T> {
  switch (cmd) {
    case "get_settings":
      return Promise.resolve(settingsResponse() as T);
    case "list_queue":
      return queueEntries() as Promise<T>;
    case "classify_paths":
      return Promise.resolve(classify() as T);
    case "path_exists":
      // S2 validation-error state: the proposed output name already exists.
      return Promise.resolve(
        (scenario.screen === "s2" && scenario.state === "validation-error") as T,
      );
    case "crash_reporting_config":
      return Promise.resolve({
        available: true,
        dsn: null,
        release: "squash@0.1.0",
        environment: "snapshot",
        features: "rar=on",
      } satisfies CrashReportingConfig as T);
    case "take_pending_open_paths":
      return Promise.resolve(pendingOpenPaths() as T);
    case "check_for_update":
      return Promise.resolve(updateInfo() as T);
    default:
      // set_settings, reveal_path, reveal_logs, cancel_job, … : no-ops.
      return Promise.resolve(undefined as T);
  }
}

export function listen(): Promise<() => void> {
  // Queue fixtures arrive via list_queue; no live events in snapshots.
  return Promise.resolve(() => undefined);
}

export function getCurrentWebview() {
  return {
    onDragDropEvent: (
      callback: (event: { payload: { type: string; paths: string[] } }) => void,
    ): Promise<() => void> => {
      if (scenario.screen === "s1" && scenario.state === "dragover") {
        queueMicrotask(() => callback({ payload: { type: "enter", paths: [] } }));
      }
      return Promise.resolve(() => undefined);
    },
  };
}

export function open(): Promise<null> {
  return Promise.resolve(null);
}
