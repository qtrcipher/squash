import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type ArchiveRef, type ItemRef, type JobEntry, type Settings } from "./api";
import { applyTheme, isolate } from "./format";
import i18n from "./i18n";
import {
  etaSeconds,
  initialQueueState,
  progressRatio,
  queueReducer,
  type QueueJob,
} from "./state/queue";
import {
  dismissDropHint,
  shouldShowDropHint,
  shouldShowWelcome,
} from "./state/onboarding";
import {
  initialUpdateState,
  runUpdateCheck,
  runUpdateInstall,
  shouldShowUpdateSheet,
  updateReducer,
} from "./state/updater";
import { initCrashReporting } from "./crashReporting";
import DropZone from "./components/DropZone";
import QueueList from "./components/QueueList";
import CompressSheet from "./components/CompressSheet";
import ExtractSheet from "./components/ExtractSheet";
import SettingsSheet from "./components/SettingsSheet";
import UpdateSheet from "./components/UpdateSheet";
import WelcomeSheet from "./components/WelcomeSheet";

type SheetState =
  | { kind: "compress"; items: ItemRef[]; totalBytes: number | null }
  | { kind: "extract"; archives: ArchiveRef[] }
  | { kind: "settings" }
  | { kind: "update" }
  | { kind: "welcome" }
  | null;

/**
 * S1 main window (docs/03 §1): toolbar + drop zone + job queue. Sheets
 * (S2/S3/S6) attach above it. State flow: Tauri events/commands → reducer →
 * components (state-management house pattern, one window, one store).
 */
export default function App() {
  const { t } = useTranslation();
  const [queue, dispatch] = useReducer(queueReducer, initialQueueState);
  const [update, dispatchUpdate] = useReducer(updateReducer, initialUpdateState);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [settingsWritable, setSettingsWritable] = useState(true);
  /** Whether this build can report crashes at all (docs/06 §6) — drives the
   * disabled-with-explanation consent toggles on S6/S7. */
  const [crashAvailable, setCrashAvailable] = useState(false);
  const [sheet, setSheet] = useState<SheetState>(null);
  const [pendingArchives, setPendingArchives] = useState<ArchiveRef[]>([]);
  const [announcement, setAnnouncement] = useState("");
  const knownStatuses = useRef<Record<string, QueueJob["status"]>>({});
  /** Highest 25% milestone already announced per job (throttling, docs/03 §5:
   * percent + ETA at throttled intervals — never per-percent chatter). */
  const knownMilestones = useRef<Record<string, number>>({});

  /** Dismiss the one-time drop-zone hint and persist the flag (best-effort —
   * a failed save only means the hint returns next launch). */
  const dismissHint = useCallback(() => {
    setSettings((current) => {
      if (!current || current.drop_zone_hint_dismissed) return current;
      const next = dismissDropHint(current);
      void api.setSettings(next).catch(() => undefined);
      return next;
    });
  }, []);

  /** F5 routing: archives → S3, everything else → S2; a mixed drop does S2
   * first and chains into S3 for the archives. */
  const handlePaths = useCallback(
    (paths: string[]) => {
      dismissHint(); // the first drop makes the hint obsolete
      void api
        .classifyPaths(paths)
        .then((result) => {
          if (result.items.length > 0) {
            if (result.archives.length > 0) setPendingArchives(result.archives);
            setSheet({ kind: "compress", items: result.items, totalBytes: result.totalBytes });
          } else if (result.archives.length > 0) {
            setSheet({ kind: "extract", archives: result.archives });
          }
        })
        .catch(() => undefined);
    },
    [dismissHint],
  );

  /** Pull OS-passed paths (docs/03 F6) and route them like a drop. */
  const drainOpenPaths = useCallback(() => {
    void api
      .takePendingOpenPaths()
      .then((paths) => {
        if (paths.length > 0) handlePaths(paths);
      })
      .catch(() => undefined);
  }, [handlePaths]);

  // Launch: load settings (language/theme apply live), restore the queue
  // (skeleton until this resolves), and subscribe to job progress.
  useEffect(() => {
    let active = true;
    void api
      .getSettings()
      .then((response) => {
        if (!active) return;
        setSettings(response.settings);
        setSettingsWritable(response.writable);
        // F1: S7 shows once, on first launch only.
        if (shouldShowWelcome(response.settings)) setSheet({ kind: "welcome" });
        void i18n.changeLanguage(response.settings.language);
        applyTheme(response.settings.theme);
        // Opt-in crash reporting (docs/06 §6): with consent off (default)
        // nothing below runs and the Sentry SDK is never even loaded.
        if (response.settings.crash_reporting) {
          void api
            .crashReportingConfig()
            .then((config) => {
              if (active) {
                void initCrashReporting({
                  consent: true,
                  config,
                  locale: response.settings.language,
                });
              }
            })
            .catch(() => undefined);
        }
        // Opt-in update check (docs/03 S6, docs/02): off by default, so no
        // network happens on launch unless the user turned this on.
        if (response.settings.update_check_opt_in) {
          void runUpdateCheck(
            dispatchUpdate,
            api.checkForUpdate,
            response.settings.release_channel,
          );
        }
      })
      .catch(() => undefined); // defaults already in place
    // The consent toggles on S6/S7 render disabled when the build has no DSN.
    void api
      .crashReportingConfig()
      .then((config) => {
        if (active) setCrashAvailable(config.available);
      })
      .catch(() => undefined);
    // Subscribe BEFORE snapshotting the queue: `listen()` registration is
    // async IPC, so a snapshot racing it can miss events emitted in the gap
    // (e.g. a restored job finishing during launch). After registration every
    // event is delivered live; anything emitted before it is in the snapshot.
    let unlisten: (() => void) | undefined;
    void api
      .onJobProgress((payload) => dispatch({ type: "progress", payload, at: Date.now() }))
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => undefined)
      .then(() => api.listQueue())
      .then((entries) => {
        if (active) dispatch({ type: "restored", entries });
      })
      .catch(() => {
        if (active) dispatch({ type: "restored", entries: [] });
      });
    // OS "open with" handoff: drain paths queued before the webview was
    // ready (cold start), then again on every nudge (warm start).
    drainOpenPaths();
    let unlistenOpen: (() => void) | undefined;
    void api.onOpenPaths(drainOpenPaths).then((fn) => {
      unlistenOpen = fn;
    });
    return () => {
      active = false;
      unlisten?.();
      unlistenOpen?.();
    };
  }, [drainOpenPaths]);

  // Screen-reader announcements on terminal transitions and throttled 25%
  // progress milestones (docs/03 §5).
  useEffect(() => {
    for (const id of queue.order) {
      const job = queue.jobs[id];
      const known = knownStatuses.current[id];
      if (known !== job.status) {
        knownStatuses.current[id] = job.status;
        knownMilestones.current[id] = 0; // a fresh run re-announces milestones
        if (known && job.status === "finished") {
          setAnnouncement(t("queue.announceDone", { name: isolate(job.label) }));
        } else if (known && job.status === "failed") {
          setAnnouncement(t("queue.announceFailed", { name: isolate(job.label) }));
        }
        continue;
      }
      if (job.status !== "running") continue;
      const ratio = progressRatio(job);
      if (ratio === null) continue; // indeterminate: nothing useful to announce
      const milestone = Math.min(3, Math.floor(ratio * 4));
      if (milestone <= (knownMilestones.current[id] ?? 0)) continue;
      knownMilestones.current[id] = milestone;
      const percent = milestone * 25;
      const eta = etaSeconds(job);
      setAnnouncement(
        eta !== null
          ? t("queue.announceProgressEta", { name: isolate(job.label), percent, seconds: eta })
          : t("queue.announceProgress", { name: isolate(job.label), percent }),
      );
    }
  }, [queue, t]);

  // An update found by any check opens D3 — but never over S7 first launch:
  // the welcome sheet is one-time and owns the first-run moment (docs/03 F1).
  useEffect(() => {
    if (update.kind === "available" && sheet?.kind !== "welcome" && sheet?.kind !== "update") {
      setSheet({ kind: "update" });
    }
  }, [update, sheet]);

  const upsert = useCallback((entry: JobEntry) => dispatch({ type: "upsert", entry }), []);
  const dismiss = useCallback((id: string) => dispatch({ type: "dismiss", id }), []);

  /**
   * Re-pull the host queue after a submit/retry. The response entry is a
   * snapshot taken at submit time and can already be stale: a fast job's
   * events are emitted from the worker/forwarder threads concurrently with
   * the IPC response, and a terminal event that reaches the webview before
   * our upsert is dropped by the reducer (no row to apply it to). The host
   * is the authoritative queue (docs/06 §7) — one reconcile pass converges
   * the row; the reducer's terminal guard keeps this race-free.
   */
  const reconcileQueue = useCallback(() => {
    void api
      .listQueue()
      .then((entries) => entries.forEach(upsert))
      .catch(() => undefined);
  }, [upsert]);

  /** Retry path (QueueRow): upsert the fresh entry, then reconcile. */
  const upsertAndReconcile = useCallback(
    (entry: JobEntry) => {
      upsert(entry);
      reconcileQueue();
    },
    [upsert, reconcileQueue],
  );

  /** Update check (docs/03 S6/D3): runs on the S6 button, and on launch only
   * when the user opted in (docs/02: no silent phone-home). A found update
   * opens D3 via the effect below; errors surface in S6 with a Retry. */
  const checkForUpdates = useCallback(
    (channel: Settings["release_channel"]) =>
      void runUpdateCheck(dispatchUpdate, api.checkForUpdate, channel),
    [],
  );

  const closeSheet = useCallback(() => setSheet(null), []);

  /** After S2 submits, a mixed drop continues to S3 for its archives. */
  const afterCompress = useCallback(
    (entries: JobEntry[]) => {
      entries.forEach(upsert);
      reconcileQueue();
      if (pendingArchives.length > 0) {
        setSheet({ kind: "extract", archives: pendingArchives });
        setPendingArchives([]);
      } else {
        setSheet(null);
      }
    },
    [pendingArchives, upsert, reconcileQueue],
  );

  const afterExtract = useCallback(
    (entries: JobEntry[]) => {
      entries.forEach(upsert);
      reconcileQueue();
      setSheet(null);
    },
    [upsert, reconcileQueue],
  );

  return (
    <div className="app-shell">
      <header className="toolbar">
        <span className="app-name">{t("app.title")}</span>
        <button type="button" className="button" onClick={() => setSheet({ kind: "settings" })}>
          {t("settings.title")}
        </button>
      </header>
      <main className="main-area">
        <div className="content">
          {shouldShowDropHint(settings, sheet !== null) && (
            <div className="hint" role="note">
              <span>{t("onboarding.dropHint")}</span>
              <button type="button" className="button small" onClick={dismissHint}>
                {t("actions.dismiss")}
              </button>
            </div>
          )}
          <DropZone onPaths={handlePaths} />
          <QueueList state={queue} onUpsert={upsertAndReconcile} onDismiss={dismiss} />
        </div>
      </main>

      {/* Live region for job state announcements (docs/03 §5). */}
      <div className="visually-hidden" role="status" aria-live="polite">
        {announcement}
      </div>

      {sheet?.kind === "compress" && (
        <CompressSheet
          items={sheet.items}
          totalBytes={sheet.totalBytes}
          defaultFormat={settings?.default_format ?? "zip"}
          defaultPreset={(settings?.default_preset ?? "builtin:balanced").replace(/^builtin:/, "")}
          onSubmitted={afterCompress}
          onClose={closeSheet}
        />
      )}
      {sheet?.kind === "extract" && (
        <ExtractSheet archives={sheet.archives} onSubmitted={afterExtract} onClose={closeSheet} />
      )}
      {sheet?.kind === "settings" && settings && (
        <SettingsSheet
          settings={settings}
          readOnly={!settingsWritable}
          crashReportingAvailable={crashAvailable}
          updateState={update}
          onSaved={setSettings}
          onCheckForUpdates={() => checkForUpdates(settings.release_channel)}
          onClose={closeSheet}
        />
      )}
      {sheet?.kind === "update" && shouldShowUpdateSheet(update) && (
        <UpdateSheet
          state={update}
          onInstall={() => void runUpdateInstall(dispatchUpdate, api.downloadAndInstallUpdate)}
          onRestart={() => void api.restartApp()}
          onLater={() => {
            dispatchUpdate({ type: "dismissed" });
            closeSheet();
          }}
        />
      )}
      {sheet?.kind === "welcome" && settings && (
        <WelcomeSheet
          settings={settings}
          readOnly={!settingsWritable}
          crashReportingAvailable={crashAvailable}
          onSaved={setSettings}
          onDone={(next) => {
            setSettings(next);
            closeSheet();
          }}
        />
      )}
    </div>
  );
}
