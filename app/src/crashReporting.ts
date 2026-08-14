/**
 * Opt-in crash reporting (docs/06 §6, docs/01 §6.3): `@sentry/react` is
 * imported dynamically and initialized ONLY when the user has consented AND
 * the build ships a DSN. With consent off (the default) the SDK is never
 * even loaded — zero crash-reporting code runs, zero network calls.
 *
 * What a report contains (mirrored in the S6/S7 consent text): the crash
 * location (stack trace), app version, OS/arch, enabled features, locale.
 * `beforeSend` drops hostname/user/breadcrumbs and rewrites the user's home
 * directory in any path to `~`.
 */

/** What the host's `crash_reporting_config` command returns. */
export interface CrashReportingConfig {
  /** A DSN is baked into this build (set at build time, never committed). */
  available: boolean;
  dsn: string | null;
  release: string;
  environment: string;
  /** Enabled feature set, e.g. `rar=on` — part of the documented report. */
  features: string;
}

/** The SDK module type without pulling it into the main bundle. */
export type SentrySdk = typeof import("@sentry/react");

/** Whether the SDK was loaded and initialized in this session. */
let active = false;

/** The consent gate — pure, so "off → SDK never loads" is testable. */
export function shouldInitCrashReporting(
  consent: boolean,
  config: Pick<CrashReportingConfig, "available" | "dsn">,
): boolean {
  return (
    consent &&
    config.available &&
    typeof config.dsn === "string" &&
    config.dsn.trim().length > 0
  );
}

/**
 * Replace the user's home directory in a path with `~` (macOS `/Users/x`,
 * Linux `/home/x`, Windows `C:\Users\x`). Unicode usernames included.
 */
export function scrubHomePath(value: string): string {
  return value
    .replace(/\/(Users|home)\/[^/\\]+/g, "~")
    .replace(/[A-Za-z]:\\Users\\[^\\]+/g, "~");
}

/**
 * Initialize the SDK when — and only when — the gate allows it. The loader
 * is injectable so tests can prove it is never called without consent.
 * Returns whether reporting is now active.
 */
export async function initCrashReporting(
  options: { consent: boolean; config: CrashReportingConfig; locale: string },
  loadSdk: () => Promise<SentrySdk> = () => import("@sentry/react"),
): Promise<boolean> {
  if (!shouldInitCrashReporting(options.consent, options.config)) return false;
  const Sentry = await loadSdk();
  Sentry.init({
    dsn: options.config.dsn ?? undefined,
    release: options.config.release,
    environment: options.config.environment,
    sendDefaultPii: false,
    initialScope: {
      tags: {
        component: "gui-frontend",
        locale: options.locale,
        features: options.config.features,
      },
    },
    // No breadcrumbs at all: nothing derived from file contents or paths.
    beforeBreadcrumb: () => null,
    beforeSend(event) {
      delete event.server_name; // hostname
      delete event.user; // never set — enforced anyway
      event.breadcrumbs = [];
      for (const exception of event.exception?.values ?? []) {
        for (const frame of exception.stacktrace?.frames ?? []) {
          if (frame.filename) frame.filename = scrubHomePath(frame.filename);
          if (frame.abs_path) frame.abs_path = scrubHomePath(frame.abs_path);
        }
      }
      if (event.message) event.message = scrubHomePath(event.message);
      return event;
    },
  });
  active = true;
  return true;
}

/**
 * S6/S7 toggle off: flush + close the client so nothing more can be sent.
 * A no-op when the SDK was never initialized — it stays unloaded.
 */
export async function shutdownCrashReporting(
  loadSdk: () => Promise<SentrySdk> = () => import("@sentry/react"),
): Promise<void> {
  if (!active) return;
  const Sentry = await loadSdk();
  await Sentry.close();
  active = false;
}

/** Test seam: is reporting live right now? */
export function crashReportingActive(): boolean {
  return active;
}
