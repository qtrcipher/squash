/**
 * S4 job-queue state (docs/03 S4) — one pure reducer, no framework.
 *
 * Unidirectional flow (state-management house pattern): Tauri events and
 * command responses are actions; components read derived state only.
 * `restoring` drives the launch skeleton (docs/03 §2 S1/S4 loading state).
 */
import type { JobEntry, ProgressPayload } from "../api";

export interface QueueJob extends JobEntry {
  /** Last two `advanced` samples, for client-side ETA. */
  prevSample: { bytes: number; at: number } | null;
  lastSample: { bytes: number; at: number } | null;
}

export interface QueueState {
  restoring: boolean;
  order: string[];
  jobs: Record<string, QueueJob>;
}

export const initialQueueState: QueueState = {
  restoring: true,
  order: [],
  jobs: {},
};

export type QueueAction =
  | { type: "restored"; entries: JobEntry[] }
  | { type: "upsert"; entry: JobEntry }
  | { type: "progress"; payload: ProgressPayload; at: number }
  | { type: "dismiss"; id: string };

function toQueueJob(entry: JobEntry, existing?: QueueJob): QueueJob {
  return { ...entry, prevSample: existing?.prevSample ?? null, lastSample: existing?.lastSample ?? null };
}

export function queueReducer(state: QueueState, action: QueueAction): QueueState {
  switch (action.type) {
    case "restored": {
      const jobs: Record<string, QueueJob> = {};
      const order: string[] = [];
      for (const entry of action.entries) {
        jobs[entry.id] = toQueueJob(entry);
        order.push(entry.id);
      }
      return { restoring: false, order, jobs };
    }

    case "upsert": {
      const { entry } = action;
      const order = state.order.includes(entry.id) ? state.order : [...state.order, entry.id];
      return {
        ...state,
        order,
        jobs: { ...state.jobs, [entry.id]: toQueueJob(entry, state.jobs[entry.id]) },
      };
    }

    case "progress": {
      const job = state.jobs[action.payload.id];
      if (!job) return state;
      const { payload, at } = action;
      let next: QueueJob;
      switch (payload.kind) {
        case "started":
          next = {
            ...job,
            status: "running",
            totalBytesEstimate: payload.totalBytesEstimate,
          };
          break;
        case "advanced":
          next = {
            ...job,
            status: "running",
            bytesDone: payload.bytesDone,
            entriesDone: payload.entriesDone,
            prevSample: job.lastSample,
            lastSample: { bytes: payload.bytesDone, at },
          };
          break;
        case "finished":
          next = {
            ...job,
            status: "finished",
            inBytes: payload.inBytes,
            outBytes: payload.outBytes,
            durationMs: payload.durationMs,
          };
          break;
        case "failed":
          next = {
            ...job,
            status: payload.errorCode === "cancelled" ? "cancelled" : "failed",
            errorCode: payload.errorCode,
          };
          break;
      }
      return { ...state, jobs: { ...state.jobs, [job.id]: next } };
    }

    case "dismiss": {
      const job = state.jobs[action.id];
      if (!job) return state;
      const jobs = { ...state.jobs };
      delete jobs[action.id];
      return { ...state, jobs, order: state.order.filter((id) => id !== action.id) };
    }
  }
}

/** Jobs in display order. */
export function queueList(state: QueueState): QueueJob[] {
  return state.order.map((id) => state.jobs[id]).filter(Boolean);
}

/** Whole-queue empty state: the section collapses entirely (docs/03 §2 S4). */
export function isQueueEmpty(state: QueueState): boolean {
  return state.order.length === 0;
}

/** Progress 0–1, or null while the total is unknown (indeterminate bar). */
export function progressRatio(job: QueueJob): number | null {
  if (job.totalBytesEstimate === null || job.totalBytesEstimate === 0) return null;
  return Math.min(1, job.bytesDone / job.totalBytesEstimate);
}

/** ETA in whole seconds from the last two progress samples; null when the
 * rate is unknown or stalled (the line is omitted, never "calculating…"). */
export function etaSeconds(job: QueueJob): number | null {
  const total = job.totalBytesEstimate;
  if (total === null || !job.lastSample || !job.prevSample) return null;
  const dt = (job.lastSample.at - job.prevSample.at) / 1000;
  const rate = (job.lastSample.bytes - job.prevSample.bytes) / dt;
  if (!(rate > 0)) return null;
  return Math.max(0, Math.round((total - job.bytesDone) / rate));
}

/** "saved 47%" for the finished-compress summary line (docs/03 S4). */
export function savedPercent(job: QueueJob): number | null {
  if (job.inBytes === null || job.outBytes === null || job.inBytes === 0) return null;
  return Math.max(0, Math.round((1 - job.outBytes / job.inBytes) * 100));
}
