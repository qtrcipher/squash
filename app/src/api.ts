/**
 * Typed wrapper over the Tauri command surface (docs/05 §2). The frontend
 * never touches the filesystem (docs/06 §7) — it renders whatever the host
 * returns and sends user intents back as commands.
 *
 * Everything goes through this module so tests can mock the two Tauri entry
 * points (`invoke`, `listen`) in one place.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CrashReportingConfig } from "./crashReporting";

export type { CrashReportingConfig };

export type Operation = "compress" | "extract";
export type JobStatus = "queued" | "running" | "finished" | "failed" | "cancelled";

/** Updater release channel (docs/03 S6) — mirrors the core `ReleaseChannel`. */
export type ReleaseChannel = "stable" | "beta";

/** What a successful update check returns (D3 sheet content). */
export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}

/** S4 row model (docs/03 S4) — mirrors the host's `JobEntryDto`. */
export interface JobEntry {
  id: string;
  operation: Operation;
  label: string;
  inputs: string[];
  destination: string;
  format: string;
  preset: string;
  status: JobStatus;
  totalBytesEstimate: number | null;
  bytesDone: number;
  entriesDone: number;
  inBytes: number | null;
  outBytes: number | null;
  durationMs: number | null;
  /** Stable SquashError code — re-localized at render (docs/06 §2). */
  errorCode: string | null;
  startedAt: string;
}

/** Payload of the `squash://job-progress` event. */
export type ProgressPayload =
  | { kind: "started"; id: string; totalBytesEstimate: number | null }
  | { kind: "advanced"; id: string; bytesDone: number; entriesDone: number; currentPath: string }
  | { kind: "finished"; id: string; inBytes: number; outBytes: number; durationMs: number }
  | { kind: "failed"; id: string; errorCode: string };

/** Mirrors the core `Settings` schema (snake_case, docs/06 §2). */
export interface Settings {
  version: number;
  language: "en" | "ar";
  theme: "system" | "light" | "dark";
  default_preset: string;
  default_format: string;
  extract: {
    dest_policy: "same_folder" | "ask";
    loose_files_policy: "new_folder" | "here";
  };
  update_check_opt_in: boolean;
  activation_counter_opt_in: boolean;
  /** Updater channel (docs/03 S6): stable (default) or beta prereleases. */
  release_channel: ReleaseChannel;
  first_launch_done: boolean;
  drop_zone_hint_dismissed: boolean;
  /** S6 verbose toggle — writes the local debug log (docs/06 §3). */
  debug_logging: boolean;
  /** Opt-in crash reporting (docs/06 §6). Default off — S7/S6 consent only. */
  crash_reporting: boolean;
}

export interface SettingsResponse {
  settings: Settings;
  writable: boolean;
  warning: string | null;
}

export interface ArchiveRef {
  path: string;
  format: string;
}

export interface ItemRef {
  path: string;
  isDir: boolean;
}

export interface ClassifiedPaths {
  archives: ArchiveRef[];
  items: ItemRef[];
  totalBytes: number | null;
}

export const PROGRESS_EVENT = "squash://job-progress";

/**
 * Nudge event for OS "open with" handoff (docs/03 F6). Payload is empty —
 * the paths themselves are pulled via `takePendingOpenPaths` so cold-start
 * events that fired before the webview subscribed are never lost.
 */
export const OPEN_PATHS_EVENT = "squash://open-paths";

export const api = {
  submitCompress: (
    inputs: string[],
    destination: string,
    format: string,
    preset: string,
  ): Promise<JobEntry> =>
    invoke<JobEntry>("submit_compress", { inputs, destination, format, preset }),

  submitExtract: (archive: string, destination: string, format: string): Promise<JobEntry> =>
    invoke<JobEntry>("submit_extract", { archive, destination, format }),

  cancelJob: (id: string): Promise<void> => invoke<void>("cancel_job", { id }),

  dismissJob: (id: string): Promise<void> => invoke<void>("dismiss_job", { id }),

  retryJob: (id: string): Promise<JobEntry> => invoke<JobEntry>("retry_job", { id }),

  listQueue: (): Promise<JobEntry[]> => invoke<JobEntry[]>("list_queue"),

  getSettings: (): Promise<SettingsResponse> => invoke<SettingsResponse>("get_settings"),

  setSettings: (settings: Settings): Promise<void> =>
    invoke<void>("set_settings", { settings }),

  /**
   * Crash-reporting build config for the S6/S7 consent UI (docs/06 §6):
   * whether this build ships a DSN, plus the tags sent when the user has
   * opted in. Reading it is local-only — no network happens here.
   */
  crashReportingConfig: (): Promise<CrashReportingConfig> =>
    invoke<CrashReportingConfig>("crash_reporting_config"),

  classifyPaths: (paths: string[]): Promise<ClassifiedPaths> =>
    invoke<ClassifiedPaths>("classify_paths", { paths }),

  pathExists: (path: string): Promise<boolean> => invoke<boolean>("path_exists", { path }),

  revealPath: (path: string): Promise<void> => invoke<void>("reveal_path", { path }),

  /**
   * S6 "Reveal log folder": opens the OS file manager on the debug log
   * (docs/06 §3). Logs stay on the device; the user attaches them manually.
   */
  revealLogs: (): Promise<void> => invoke<void>("reveal_logs"),

  /**
   * S7 "make default handler" (docs/03 F1): opens the OS default-apps UI
   * where one exists (Windows); rejects elsewhere so the caller can show
   * manual instructions instead of a fake toggle (docs/03 F6).
   */
  openDefaultAppsSettings: (): Promise<void> => invoke<void>("open_default_apps_settings"),

  onJobProgress: (callback: (payload: ProgressPayload) => void): Promise<UnlistenFn> =>
    listen<ProgressPayload>(PROGRESS_EVENT, (event) => callback(event.payload)),

  /**
   * Drain paths the OS handed to the host (argv / RunEvent::Opened /
   * second-instance launch, docs/03 F6). Route them through classifyPaths.
   */
  takePendingOpenPaths: (): Promise<string[]> => invoke<string[]>("take_pending_open_paths"),

  /** Subscribe to the open-paths nudge; the callback should pull, not read a payload. */
  onOpenPaths: (callback: () => void): Promise<UnlistenFn> =>
    listen(OPEN_PATHS_EVENT, () => callback()),

  /**
   * Check the channel's manifest on GitHub Releases for a newer version
   * (docs/03 S6/D3). Resolves `null` when up-to-date; rejects on network /
   * manifest errors so the caller can show the error-with-retry state.
   */
  checkForUpdate: (channel: ReleaseChannel): Promise<UpdateInfo | null> =>
    invoke<UpdateInfo | null>("check_for_update", { channel }),

  /**
   * Download, verify (updater signature) and install the update found by the
   * last check. On Windows the installer exits the app; elsewhere call
   * `restartApp` when the user chooses to.
   */
  downloadAndInstallUpdate: (): Promise<void> => invoke<void>("download_and_install_update"),

  /** Restart into the installed update (macOS/Linux). Never resolves. */
  restartApp: (): Promise<void> => invoke<void>("restart_app"),
};
