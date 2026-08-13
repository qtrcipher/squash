import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { formatBytes } from "../format";
import {
  etaSeconds,
  progressRatio,
  queueList,
  savedPercent,
  type QueueJob,
  type QueueState,
} from "../state/queue";

/**
 * S4 job queue (docs/03 §2 S4 — all four states):
 * - loading: skeleton rows while the interrupted queue restores on launch
 * - empty: the section collapses entirely (renders nothing)
 * - error: failed rows stay, tinted, with a plain-language reason + recovery
 * - success: running rows show bar/%/ETA; finished rows collapse to a summary
 */
export default function QueueList({
  state,
  onUpsert,
  onDismiss,
}: {
  state: QueueState;
  onUpsert: (entry: import("../api").JobEntry) => void;
  onDismiss: (id: string) => void;
}) {
  const { t } = useTranslation();

  if (state.restoring) {
    return (
      <section className="queue" aria-label={t("queue.title")} aria-busy="true">
        <h2 className="queue-title">{t("queue.title")}</h2>
        <p className="visually-hidden" role="status">
          {t("queue.restoring")}
        </p>
        <ul className="queue-list" aria-hidden="true">
          {[0, 1].map((i) => (
            <li key={i} className="queue-row skeleton">
              <span className="skeleton-block" />
              <span className="skeleton-block wide" />
            </li>
          ))}
        </ul>
      </section>
    );
  }

  const jobs = queueList(state);
  if (jobs.length === 0) return null;

  return (
    <section className="queue" aria-label={t("queue.title")}>
      <h2 className="queue-title">{t("queue.title")}</h2>
      <ul className="queue-list">
        {jobs.map((job) => (
          <QueueRow key={job.id} job={job} onUpsert={onUpsert} onDismiss={onDismiss} />
        ))}
      </ul>
    </section>
  );
}

function QueueRow({
  job,
  onUpsert,
  onDismiss,
}: {
  job: QueueJob;
  onUpsert: (entry: import("../api").JobEntry) => void;
  onDismiss: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const cancel = () => void api.cancelJob(job.id).catch(() => undefined);
  const retry = () =>
    void api
      .retryJob(job.id)
      .then(onUpsert)
      .catch(() => undefined);
  const dismiss = () =>
    void api
      .dismissJob(job.id)
      .then(() => onDismiss(job.id))
      .catch(() => undefined);
  const reveal = () => void api.revealPath(job.destination).catch(() => undefined);
  const copyPath = () =>
    void navigator.clipboard
      .writeText(job.destination)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => undefined);

  const ratio = progressRatio(job);
  const eta = job.status === "running" ? etaSeconds(job) : null;
  const saved = job.status === "finished" ? savedPercent(job) : null;

  return (
    <li className={`queue-row state-${job.status}`}>
      <span className="job-icon" aria-hidden="true">
        {job.operation === "compress" ? <CompressIcon /> : <ExtractIcon />}
      </span>
      <div className="job-main">
        <div className="job-headline">
          <span className="job-label" dir="auto">
            {job.label}
          </span>
          <span className={`badge badge-${job.status}`}>{t(`queue.${badgeKey(job)}`)}</span>
        </div>

        {job.status === "running" && (
          <div className="job-progress">
            <div
              className={ratio === null ? "progress indeterminate" : "progress"}
              role="progressbar"
              aria-label={t("queue.progressLabel", { name: job.label })}
              aria-valuenow={ratio === null ? undefined : Math.round(ratio * 100)}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div
                className="progress-fill"
                style={ratio === null ? undefined : { inlineSize: `${ratio * 100}%` }}
              />
            </div>
            <span className="job-meta mono" dir="ltr">
              {ratio !== null && `${Math.round(ratio * 100)}% · `}
              {formatBytes(job.bytesDone)}
              {job.totalBytesEstimate !== null && ` / ${formatBytes(job.totalBytesEstimate)}`}
              {eta !== null && ` · ${t("queue.eta", { seconds: eta })}`}
            </span>
          </div>
        )}

        {job.status === "queued" && <span className="job-meta">{t("queue.queued")}</span>}

        {job.status === "finished" && (
          <span className="job-meta" dir="auto">
            {job.operation === "compress" && saved !== null
              ? t("queue.savedSummary", {
                  name: job.label,
                  inBytes: formatBytes(job.inBytes ?? 0),
                  outBytes: formatBytes(job.outBytes ?? 0),
                  percent: saved,
                })
              : t("queue.extractedSummary", { name: job.label })}
          </span>
        )}

        {(job.status === "failed" || job.status === "cancelled") && (
          <span className="job-error" role="alert">
            {t(`errors.${job.errorCode ?? "unknown"}`)}
          </span>
        )}
      </div>

      <div className="job-actions">
        {(job.status === "queued" || job.status === "running") && (
          <button
            type="button"
            className="button small"
            aria-label={t("queue.cancelJob", { name: job.label })}
            onClick={cancel}
          >
            {t("actions.cancel")}
          </button>
        )}
        {job.status === "finished" && (
          <>
            <button
              type="button"
              className="button small"
              aria-label={t("queue.revealJob", { name: job.label })}
              onClick={reveal}
            >
              {t("actions.reveal")}
            </button>
            <button
              type="button"
              className="button small"
              aria-label={t("queue.copyPathJob", { name: job.label })}
              onClick={copyPath}
            >
              {copied ? t("actions.copied") : t("actions.copyPath")}
            </button>
          </>
        )}
        {job.status === "failed" && (
          <>
            <button
              type="button"
              className="button small"
              aria-label={t("queue.retryJob", { name: job.label })}
              onClick={retry}
            >
              {t("actions.retry")}
            </button>
            <button
              type="button"
              className="button small"
              aria-label={t("queue.revealJob", { name: job.label })}
              onClick={reveal}
            >
              {t("actions.reveal")}
            </button>
          </>
        )}
        {job.status === "cancelled" && (
          <button
            type="button"
            className="button small"
            aria-label={t("queue.retryJob", { name: job.label })}
            onClick={retry}
          >
            {t("actions.retry")}
          </button>
        )}
        {job.status !== "queued" && job.status !== "running" && (
          <button
            type="button"
            className="button small"
            aria-label={t("queue.dismissJob", { name: job.label })}
            onClick={dismiss}
          >
            {t("actions.dismiss")}
          </button>
        )}
      </div>
    </li>
  );
}

function badgeKey(job: QueueJob): string {
  switch (job.status) {
    case "finished":
      return "done";
    default:
      return job.status;
  }
}

function CompressIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M21 8v13H3V8" />
      <path d="M1 3h22v5H1z" />
      <path d="M12 12v6" />
      <path d="m9 15 3 3 3-3" />
    </svg>
  );
}

function ExtractIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M21 8v13H3V8" />
      <path d="M1 3h22v5H1z" />
      <path d="M12 18v-6" />
      <path d="m9 12 3-3 3 3" />
    </svg>
  );
}
