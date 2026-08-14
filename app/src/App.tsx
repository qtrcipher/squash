import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type ArchiveRef, type ItemRef, type JobEntry, type Settings } from "./api";
import { applyTheme } from "./format";
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
import { initCrashReporting } from "./crashReporting";
import DropZone from "./components/DropZone";
import QueueList from "./components/QueueList";
import CompressSheet from "./components/CompressSheet";
import ExtractSheet from "./components/ExtractSheet";
import SettingsSheet from "./components/SettingsSheet";
import WelcomeSheet from "./components/WelcomeSheet";

type SheetState =
  | { kind: "compress"; items: ItemRef[]; totalBytes: number | null }
  | { kind: "extract"; archives: ArchiveRef[] }
  | { kind: "settings" }
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
      })
      .catch(() => undefined); // defaults already in place
    // The consent toggles on S6/S7 render disabled when the build has no DSN.
    void api
      .crashReportingConfig()
      .then((config) => {
        if (active) setCrashAvailable(config.available);
      })
      .catch(() => undefined);
    void api
      .listQueue()
      .then((entries) => {
        if (active) dispatch({ type: "restored", entries });
      })
      .catch(() => {
        if (active) dispatch({ type: "restored", entries: [] });
      });
    let unlisten: (() => void) | undefined;
    void api
      .onJobProgress((payload) => dispatch({ type: "progress", payload, at: Date.now() }))
      .then((fn) => {
        unlisten = fn;
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
          setAnnouncement(t("queue.announceDone", { name: job.label }));
        } else if (known && job.status === "failed") {
          setAnnouncement(t("queue.announceFailed", { name: job.label }));
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
          ? t("queue.announceProgressEta", { name: job.label, percent, seconds: eta })
          : t("queue.announceProgress", { name: job.label, percent }),
      );
    }
  }, [queue, t]);

  const upsert = useCallback((entry: JobEntry) => dispatch({ type: "upsert", entry }), []);
  const dismiss = useCallback((id: string) => dispatch({ type: "dismiss", id }), []);

  const closeSheet = useCallback(() => setSheet(null), []);

  /** After S2 submits, a mixed drop continues to S3 for its archives. */
  const afterCompress = useCallback(
    (entries: JobEntry[]) => {
      entries.forEach(upsert);
      if (pendingArchives.length > 0) {
        setSheet({ kind: "extract", archives: pendingArchives });
        setPendingArchives([]);
      } else {
        setSheet(null);
      }
    },
    [pendingArchives, upsert],
  );

  const afterExtract = useCallback(
    (entries: JobEntry[]) => {
      entries.forEach(upsert);
      setSheet(null);
    },
    [upsert],
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
          <QueueList state={queue} onUpsert={upsert} onDismiss={dismiss} />
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
          onSaved={setSettings}
          onClose={closeSheet}
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
