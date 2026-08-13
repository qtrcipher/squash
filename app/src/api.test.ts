import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import { api, OPEN_PATHS_EVENT, PROGRESS_EVENT, type JobEntry } from "./api";

const entry: JobEntry = {
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
};

describe("api wrapper", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
  });

  it("submitCompress invokes with mirrored Job args", async () => {
    invokeMock.mockResolvedValue(entry);
    const result = await api.submitCompress(["/tmp/photos"], "/tmp/photos.zip", "zip", "builtin:fast");
    expect(invokeMock).toHaveBeenCalledWith("submit_compress", {
      inputs: ["/tmp/photos"],
      destination: "/tmp/photos.zip",
      format: "zip",
      preset: "builtin:fast",
    });
    expect(result).toEqual(entry);
  });

  it("submitExtract invokes the extract command", async () => {
    invokeMock.mockResolvedValue(entry);
    await api.submitExtract("/tmp/a.tar.gz", "/tmp", "tar.gz");
    expect(invokeMock).toHaveBeenCalledWith("submit_extract", {
      archive: "/tmp/a.tar.gz",
      destination: "/tmp",
      format: "tar.gz",
    });
  });

  it("cancel/dismiss/retry pass the job id", async () => {
    invokeMock.mockResolvedValue(undefined);
    await api.cancelJob("job-1");
    await api.dismissJob("job-1");
    expect(invokeMock).toHaveBeenNthCalledWith(1, "cancel_job", { id: "job-1" });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "dismiss_job", { id: "job-1" });
    invokeMock.mockResolvedValue(entry);
    await api.retryJob("job-1");
    expect(invokeMock).toHaveBeenNthCalledWith(3, "retry_job", { id: "job-1" });
  });

  it("settings round-trip through get/set", async () => {
    const response = { settings: { version: 1 }, writable: true, warning: null };
    invokeMock.mockResolvedValue(response);
    await expect(api.getSettings()).resolves.toEqual(response);
    expect(invokeMock).toHaveBeenCalledWith("get_settings");
    invokeMock.mockResolvedValue(undefined);
    await api.setSettings(response.settings as never);
    expect(invokeMock).toHaveBeenCalledWith("set_settings", { settings: response.settings });
  });

  it("rejects propagate to the caller (error states recover, not crash)", async () => {
    invokeMock.mockRejectedValue(new Error("disk full"));
    await expect(api.setSettings({} as never)).rejects.toThrow("disk full");
  });

  it("onJobProgress subscribes to the squash://job-progress channel", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    const callback = vi.fn();
    await api.onJobProgress(callback);
    expect(listenMock).toHaveBeenCalledWith(PROGRESS_EVENT, expect.any(Function));
    // The registered handler unwraps the event payload.
    const handler = listenMock.mock.calls[0][1] as (e: { payload: unknown }) => void;
    handler({ payload: { kind: "failed", id: "x", errorCode: "internal" } });
    expect(callback).toHaveBeenCalledWith({ kind: "failed", id: "x", errorCode: "internal" });
  });

  it("takePendingOpenPaths drains the host queue (docs/03 F6)", async () => {
    invokeMock.mockResolvedValue(["/tmp/a.zip", "/tmp/photos"]);
    await expect(api.takePendingOpenPaths()).resolves.toEqual(["/tmp/a.zip", "/tmp/photos"]);
    expect(invokeMock).toHaveBeenCalledWith("take_pending_open_paths");
  });

  it("onOpenPaths subscribes to the nudge and ignores its (empty) payload", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    const callback = vi.fn();
    await api.onOpenPaths(callback);
    expect(listenMock).toHaveBeenCalledWith(OPEN_PATHS_EVENT, expect.any(Function));
    const handler = listenMock.mock.calls[0][1] as (e: unknown) => void;
    handler({ payload: null });
    expect(callback).toHaveBeenCalledTimes(1);
  });
});
