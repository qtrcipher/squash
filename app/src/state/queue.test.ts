import { describe, expect, it } from "vitest";
import type { JobEntry } from "../api";
import {
  etaSeconds,
  initialQueueState,
  isQueueEmpty,
  progressRatio,
  queueList,
  queueReducer,
  savedPercent,
  type QueueState,
} from "./queue";

function entry(overrides: Partial<JobEntry> = {}): JobEntry {
  return {
    id: "job-1",
    operation: "compress",
    label: "photos.zip",
    inputs: ["/tmp/photos"],
    destination: "/tmp/photos.zip",
    format: "zip",
    preset: "builtin:balanced",
    status: "queued",
    totalBytesEstimate: null,
    bytesDone: 0,
    entriesDone: 0,
    inBytes: null,
    outBytes: null,
    durationMs: null,
    errorCode: null,
    startedAt: "2026-08-13T12:00:00Z",
    ...overrides,
  };
}

function withJob(...entries: JobEntry[]): QueueState {
  const list = entries.length > 0 ? entries : [entry()];
  return queueReducer(initialQueueState, { type: "restored", entries: list });
}

describe("queue reducer", () => {
  it("starts in the restoring (skeleton) state", () => {
    expect(initialQueueState.restoring).toBe(true);
    expect(isQueueEmpty(initialQueueState)).toBe(true);
  });

  it("restored entries populate the queue and end the skeleton", () => {
    const state = withJob(entry({ id: "a" }), entry({ id: "b" }));
    expect(state.restoring).toBe(false);
    expect(queueList(state).map((j) => j.id)).toEqual(["a", "b"]);
  });

  it("restoring an empty queue collapses the section (docs/03 S4 empty)", () => {
    const state = queueReducer(initialQueueState, { type: "restored", entries: [] });
    expect(state.restoring).toBe(false);
    expect(isQueueEmpty(state)).toBe(true);
  });

  it("submit → started → advanced → finished lifecycle", () => {
    let state = withJob();
    expect(queueList(state)[0].status).toBe("queued");

    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "started", id: "job-1", totalBytesEstimate: 1000 },
      at: 0,
    });
    let job = queueList(state)[0];
    expect(job.status).toBe("running");
    expect(job.totalBytesEstimate).toBe(1000);

    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "advanced", id: "job-1", bytesDone: 500, entriesDone: 3, currentPath: "/tmp/photos/a.jpg" },
      at: 1000,
    });
    job = queueList(state)[0];
    expect(job.bytesDone).toBe(500);
    expect(progressRatio(job)).toBe(0.5);

    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "finished", id: "job-1", inBytes: 1200, outBytes: 600, durationMs: 12300 },
      at: 2000,
    });
    job = queueList(state)[0];
    expect(job.status).toBe("finished");
    expect(savedPercent(job)).toBe(50);
  });

  it("failed transitions carry the stable error code", () => {
    let state = withJob();
    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "failed", id: "job-1", errorCode: "disk_full" },
      at: 0,
    });
    const job = queueList(state)[0];
    expect(job.status).toBe("failed");
    expect(job.errorCode).toBe("disk_full");
  });

  it("failed with the cancelled code lands in the cancelled state", () => {
    let state = withJob();
    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "failed", id: "job-1", errorCode: "cancelled" },
      at: 0,
    });
    expect(queueList(state)[0].status).toBe("cancelled");
  });

  it("one job's failure does not disturb its siblings (docs/03 F4)", () => {
    let state = withJob(entry({ id: "a" }), entry({ id: "b" }));
    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "failed", id: "a", errorCode: "corrupt_archive" },
      at: 0,
    });
    const [a, b] = queueList(state);
    expect(a.status).toBe("failed");
    expect(b.status).toBe("queued");
  });

  it("upsert (retry resubmission) replaces the entry in place", () => {
    let state = withJob(entry({ id: "a" }));
    state = queueReducer(state, { type: "upsert", entry: entry({ id: "a", status: "running" }) });
    expect(queueList(state)).toHaveLength(1);
    expect(queueList(state)[0].status).toBe("running");
    // New ids append.
    state = queueReducer(state, { type: "upsert", entry: entry({ id: "b" }) });
    expect(queueList(state).map((j) => j.id)).toEqual(["a", "b"]);
  });

  it("dismiss removes the row", () => {
    let state = withJob(entry({ id: "a" }), entry({ id: "b" }));
    state = queueReducer(state, { type: "dismiss", id: "a" });
    expect(queueList(state).map((j) => j.id)).toEqual(["b"]);
  });

  it("progress for an unknown job is ignored", () => {
    const state = withJob();
    const next = queueReducer(state, {
      type: "progress",
      payload: { kind: "started", id: "ghost", totalBytesEstimate: 1 },
      at: 0,
    });
    expect(next).toBe(state);
  });

  it("progressRatio is null without a total (indeterminate bar)", () => {
    const state = withJob(entry({ status: "running" }));
    expect(progressRatio(queueList(state)[0])).toBeNull();
  });

  it("etaSeconds derives from the last two samples and vanishes when stalled", () => {
    let state = withJob(entry({ status: "running", totalBytesEstimate: 1000 }));
    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "advanced", id: "job-1", bytesDone: 200, entriesDone: 1, currentPath: "a" },
      at: 1000,
    });
    expect(etaSeconds(queueList(state)[0])).toBeNull(); // only one sample
    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "advanced", id: "job-1", bytesDone: 600, entriesDone: 2, currentPath: "b" },
      at: 3000,
    });
    // 400 bytes / 2 s = 200 B/s; 400 left → 2 s.
    expect(etaSeconds(queueList(state)[0])).toBe(2);
    state = queueReducer(state, {
      type: "progress",
      payload: { kind: "advanced", id: "job-1", bytesDone: 600, entriesDone: 2, currentPath: "b" },
      at: 4000,
    });
    expect(etaSeconds(queueList(state)[0])).toBeNull(); // stalled
  });
});
