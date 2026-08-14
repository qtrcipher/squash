/**
 * Update-check state (docs/03 S6/D3) — one pure reducer, no framework, same
 * discipline as the queue store. The async runners take the network calls as
 * injected functions so tests never touch the Tauri updater plugin.
 *
 * Four-state discipline: checking / up-to-date / available / error-with-retry
 * (plus the install tail: downloading → ready-to-restart). An update found by
 * any check — manual or the opted-in launch check — is `available`; App turns
 * that into the D3 sheet.
 */
import type { ReleaseChannel, UpdateInfo } from "../api";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; info: UpdateInfo }
  | { kind: "downloading"; info: UpdateInfo }
  | { kind: "readyToRestart"; info: UpdateInfo }
  /** `during: "check"` retries with a fresh check; `"install"` likewise — the
   * host re-checks, since a failed install consumes the pending update. */
  | { kind: "error"; during: "check" | "install" };

export const initialUpdateState: UpdateState = { kind: "idle" };

export type UpdateAction =
  | { type: "checkStarted" }
  | { type: "checkFinished"; info: UpdateInfo | null }
  | { type: "checkFailed" }
  | { type: "installStarted" }
  | { type: "installFinished" }
  | { type: "installFailed" }
  /** "Later" on D3 or closing an up-to-date/error note — back to idle. */
  | { type: "dismissed" };

export function updateReducer(state: UpdateState, action: UpdateAction): UpdateState {
  switch (action.type) {
    case "checkStarted":
      // Never interrupt an install with a background check.
      if (state.kind === "downloading" || state.kind === "readyToRestart") return state;
      return { kind: "checking" };
    case "checkFinished":
      if (state.kind !== "checking") return state;
      return action.info ? { kind: "available", info: action.info } : { kind: "upToDate" };
    case "checkFailed":
      if (state.kind !== "checking") return state;
      return { kind: "error", during: "check" };
    case "installStarted":
      if (state.kind !== "available") return state;
      return { kind: "downloading", info: state.info };
    case "installFinished":
      if (state.kind !== "downloading") return state;
      return { kind: "readyToRestart", info: state.info };
    case "installFailed":
      if (state.kind !== "downloading") return state;
      return { kind: "error", during: "install" };
    case "dismissed":
      // readyToRestart survives dismissal: the sheet may close (Windows is
      // about to exit anyway) without losing the "restart to finish" truth.
      return state.kind === "readyToRestart" ? state : { kind: "idle" };
  }
}

/** True when an update sheet (D3) should be on screen; narrows the state. */
export function shouldShowUpdateSheet(
  state: UpdateState,
): state is Extract<UpdateState, { kind: "available" | "downloading" | "readyToRestart" }> {
  return state.kind === "available" || state.kind === "downloading" || state.kind === "readyToRestart";
}

/** Run a check, dispatching started/finished/failed. `check` is the host call. */
export async function runUpdateCheck(
  dispatch: (action: UpdateAction) => void,
  check: (channel: ReleaseChannel) => Promise<UpdateInfo | null>,
  channel: ReleaseChannel,
): Promise<void> {
  dispatch({ type: "checkStarted" });
  try {
    dispatch({ type: "checkFinished", info: await check(channel) });
  } catch {
    dispatch({ type: "checkFailed" });
  }
}

/** Download + install the pending update. `install` is the host call. */
export async function runUpdateInstall(
  dispatch: (action: UpdateAction) => void,
  install: () => Promise<void>,
): Promise<void> {
  dispatch({ type: "installStarted" });
  try {
    await install();
    dispatch({ type: "installFinished" });
  } catch {
    dispatch({ type: "installFailed" });
  }
}
