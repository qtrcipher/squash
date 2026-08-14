import { describe, expect, it, vi } from "vitest";
import type { UpdateInfo } from "../api";
import {
  initialUpdateState,
  runUpdateCheck,
  runUpdateInstall,
  shouldShowUpdateSheet,
  updateReducer,
  type UpdateAction,
  type UpdateState,
} from "./updater";

const info: UpdateInfo = { version: "0.2.0", notes: "Bug fixes", date: "2026-08-14T00:00:00Z" };

/** Apply an action list through the real reducer, like a component would. */
function reduceAll(actions: UpdateAction[], from: UpdateState = initialUpdateState): UpdateState {
  return actions.reduce(updateReducer, from);
}

describe("updateReducer", () => {
  it("check lifecycle: idle → checking → available carries the update info", () => {
    const state = reduceAll([{ type: "checkStarted" }, { type: "checkFinished", info }]);
    expect(state).toEqual({ kind: "available", info });
  });

  it("a null result means up-to-date", () => {
    const state = reduceAll([{ type: "checkStarted" }, { type: "checkFinished", info: null }]);
    expect(state).toEqual({ kind: "upToDate" });
  });

  it("a failed check lands in the error state (retry = a fresh check)", () => {
    const state = reduceAll([{ type: "checkStarted" }, { type: "checkFailed" }]);
    expect(state).toEqual({ kind: "error", during: "check" });
    const retried = reduceAll([{ type: "checkStarted" }, { type: "checkFinished", info: null }], state);
    expect(retried).toEqual({ kind: "upToDate" });
  });

  it("install lifecycle: available → downloading → readyToRestart", () => {
    const state = reduceAll([
      { type: "checkStarted" },
      { type: "checkFinished", info },
      { type: "installStarted" },
      { type: "installFinished" },
    ]);
    expect(state).toEqual({ kind: "readyToRestart", info });
  });

  it("a failed install lands in the error state", () => {
    const state = reduceAll([
      { type: "checkStarted" },
      { type: "checkFinished", info },
      { type: "installStarted" },
      { type: "installFailed" },
    ]);
    expect(state).toEqual({ kind: "error", during: "install" });
  });

  it("dismiss ('Later') returns to idle from any pre-install state", () => {
    for (const from of [
      { kind: "upToDate" },
      { kind: "available", info },
      { kind: "error", during: "check" },
    ] as UpdateState[]) {
      expect(updateReducer(from, { type: "dismissed" })).toEqual({ kind: "idle" });
    }
  });

  it("readyToRestart survives dismissal — the update is installed", () => {
    const state: UpdateState = { kind: "readyToRestart", info };
    expect(updateReducer(state, { type: "dismissed" })).toEqual(state);
  });

  it("a background check never interrupts an install", () => {
    const downloading: UpdateState = { kind: "downloading", info };
    expect(updateReducer(downloading, { type: "checkStarted" })).toEqual(downloading);
    const ready: UpdateState = { kind: "readyToRestart", info };
    expect(updateReducer(ready, { type: "checkStarted" })).toEqual(ready);
  });

  it("out-of-order actions are no-ops (stale async completions)", () => {
    expect(updateReducer(initialUpdateState, { type: "checkFinished", info })).toEqual(
      initialUpdateState,
    );
    expect(updateReducer(initialUpdateState, { type: "installStarted" })).toEqual(
      initialUpdateState,
    );
    const available: UpdateState = { kind: "available", info };
    expect(updateReducer(available, { type: "installFinished" })).toEqual(available);
  });

  it("only the sheet-worthy states show D3", () => {
    expect(shouldShowUpdateSheet({ kind: "idle" })).toBe(false);
    expect(shouldShowUpdateSheet({ kind: "checking" })).toBe(false);
    expect(shouldShowUpdateSheet({ kind: "upToDate" })).toBe(false);
    expect(shouldShowUpdateSheet({ kind: "error", during: "check" })).toBe(false);
    expect(shouldShowUpdateSheet({ kind: "available", info })).toBe(true);
    expect(shouldShowUpdateSheet({ kind: "downloading", info })).toBe(true);
    expect(shouldShowUpdateSheet({ kind: "readyToRestart", info })).toBe(true);
  });
});

describe("runUpdateCheck (mocked updater plugin API)", () => {
  it("dispatches started → finished with the host's answer", async () => {
    const dispatch = vi.fn();
    const check = vi.fn().mockResolvedValue(info);
    await runUpdateCheck(dispatch, check, "beta");
    expect(check).toHaveBeenCalledWith("beta");
    expect(dispatch.mock.calls.map(([a]) => a)).toEqual([
      { type: "checkStarted" },
      { type: "checkFinished", info },
    ]);
  });

  it("dispatches started → failed when the host rejects", async () => {
    const dispatch = vi.fn();
    const check = vi.fn().mockRejectedValue(new Error("offline"));
    await runUpdateCheck(dispatch, check, "stable");
    expect(dispatch.mock.calls.map(([a]) => a)).toEqual([
      { type: "checkStarted" },
      { type: "checkFailed" },
    ]);
  });
});

describe("runUpdateInstall (mocked updater plugin API)", () => {
  it("dispatches installStarted → installFinished on success", async () => {
    const dispatch = vi.fn();
    await runUpdateInstall(dispatch, vi.fn().mockResolvedValue(undefined));
    expect(dispatch.mock.calls.map(([a]) => a)).toEqual([
      { type: "installStarted" },
      { type: "installFinished" },
    ]);
  });

  it("dispatches installStarted → installFailed on rejection", async () => {
    const dispatch = vi.fn();
    await runUpdateInstall(dispatch, vi.fn().mockRejectedValue(new Error("bad signature")));
    expect(dispatch.mock.calls.map(([a]) => a)).toEqual([
      { type: "installStarted" },
      { type: "installFailed" },
    ]);
  });
});
