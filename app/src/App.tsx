import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type ArchiveRef, type ItemRef, type JobEntry, type Settings } from "./api";
import { applyTheme } from "./format";
import i18n from "./i18n";
import {
  initialQueueState,
  queueReducer,
  type QueueJob,
} from "./state/queue";
import DropZone from "./components/DropZone";
import QueueList from "./components/QueueList";
import CompressSheet from "./components/CompressSheet";
import ExtractSheet from "./components/ExtractSheet";
import SettingsSheet from "./components/SettingsSheet";

type SheetState =
  | { kind: "compress"; items: ItemRef[]; totalBytes: number | null }
  | { kind: "extract"; archives: ArchiveRef[] }
  | { kind: "settings" }
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
  const [sheet, setSheet] = useState<SheetState>(null);
  const [pendingArchives, setPendingArchives] = useState<ArchiveRef[]>([]);
  const [announcement, setAnnouncement] = useState("");
  const knownStatuses = useRef<Record<string, QueueJob["status"]>>({});

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
        void i18n.changeLanguage(response.settings.language);
        applyTheme(response.settings.theme);
      })
      .catch(() => undefined); // defaults already in place
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
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  // Screen-reader announcements on terminal transitions (docs/03 §5).
  useEffect(() => {
    for (const id of queue.order) {
      const job = queue.jobs[id];
      const known = knownStatuses.current[id];
      if (known === job.status) continue;
      knownStatuses.current[id] = job.status;
      if (known && job.status === "finished") {
        setAnnouncement(t("queue.announceDone", { name: job.label }));
      } else if (known && job.status === "failed") {
        setAnnouncement(t("queue.announceFailed", { name: job.label }));
      }
    }
  }, [queue, t]);

  /** F5 routing: archives → S3, everything else → S2; a mixed drop does S2
   * first and chains into S3 for the archives. */
  const handlePaths = useCallback((paths: string[]) => {
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
  }, []);

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
          onSaved={setSettings}
          onClose={closeSheet}
        />
      )}
    </div>
  );
}
