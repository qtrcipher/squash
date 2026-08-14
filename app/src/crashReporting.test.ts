import { describe, expect, it, vi } from "vitest";
import {
  crashReportingActive,
  initCrashReporting,
  scrubHomePath,
  shouldInitCrashReporting,
  shutdownCrashReporting,
  type CrashReportingConfig,
  type SentrySdk,
} from "./crashReporting";

function config(overrides: Partial<CrashReportingConfig> = {}): CrashReportingConfig {
  return {
    available: true,
    dsn: "https://key@o0.ingest.sentry.io/0",
    release: "squash@0.1.0",
    environment: "development",
    features: "rar=on",
    ...overrides,
  };
}

function fakeSentry() {
  return { init: vi.fn(), close: vi.fn(async () => true) };
}

describe("crash-reporting consent gate (docs/06 §6)", () => {
  it("consent off → the SDK is never even loaded", async () => {
    const sentry = fakeSentry();
    const loader = vi.fn(async () => sentry as unknown as SentrySdk);
    const started = await initCrashReporting({ consent: false, config: config(), locale: "en" }, loader);
    expect(started).toBe(false);
    expect(loader).not.toHaveBeenCalled();
    expect(crashReportingActive()).toBe(false);
  });

  it("a build without a DSN never loads the SDK, consent or not", async () => {
    const loader = vi.fn(async () => fakeSentry() as unknown as SentrySdk);
    expect(shouldInitCrashReporting(true, config({ available: false }))).toBe(false);
    expect(shouldInitCrashReporting(true, config({ dsn: null }))).toBe(false);
    expect(shouldInitCrashReporting(true, config({ dsn: "  " }))).toBe(false);
    const started = await initCrashReporting(
      { consent: true, config: config({ available: false, dsn: null }), locale: "en" },
      loader,
    );
    expect(started).toBe(false);
    expect(loader).not.toHaveBeenCalled();
  });

  it("consent + DSN → init with release/environment and the privacy filter", async () => {
    const sentry = fakeSentry();
    const started = await initCrashReporting(
      { consent: true, config: config(), locale: "ar" },
      async () => sentry as unknown as SentrySdk,
    );
    expect(started).toBe(true);
    expect(crashReportingActive()).toBe(true);
    expect(sentry.init).toHaveBeenCalledOnce();
    const options = sentry.init.mock.calls[0][0] as Record<string, never> & {
      beforeSend: (event: never) => unknown;
      beforeBreadcrumb: (crumb: unknown) => unknown;
    };
    expect(options.dsn).toBe(config().dsn);
    expect(options.release).toBe("squash@0.1.0");
    expect(options.environment).toBe("development");
    expect(options.sendDefaultPii).toBe(false);

    // The scrub rules (docs/06 §6): hostname/user/breadcrumbs dropped, home
    // dir rewritten to ~, file contents never touched (no breadcrumbs at all).
    const event = {
      server_name: "my-macbook",
      user: { id: "123" },
      breadcrumbs: [{ message: "read /Users/hamam/secret.txt" }],
      message: "failed at /home/hamam/docs/a.zip",
      exception: {
        values: [
          {
            stacktrace: {
              frames: [{ filename: "/Users/hamam/src/main.ts", abs_path: "/Users/hamam/src/main.ts" }],
            },
          },
        ],
      },
    };
    const scrubbed = options.beforeSend(event as never) as typeof event;
    expect(scrubbed.server_name).toBeUndefined();
    expect(scrubbed.user).toBeUndefined();
    expect(scrubbed.breadcrumbs).toEqual([]);
    expect(scrubbed.message).toBe("failed at ~/docs/a.zip");
    expect(scrubbed.exception.values[0].stacktrace.frames[0].filename).toBe("~/src/main.ts");
    expect(scrubbed.exception.values[0].stacktrace.frames[0].abs_path).toBe("~/src/main.ts");
    expect(options.beforeBreadcrumb({})).toBeNull();

    // Toggle off closes the client; a second shutdown stays a no-op.
    await shutdownCrashReporting(async () => sentry as unknown as SentrySdk);
    expect(sentry.close).toHaveBeenCalledOnce();
    expect(crashReportingActive()).toBe(false);
  });
});

describe("scrubHomePath", () => {
  it("rewrites macOS/Linux/Windows home prefixes to ~", () => {
    expect(scrubHomePath("/Users/hamam/Developer/f.zip")).toBe("~/Developer/f.zip");
    expect(scrubHomePath("/home/nadia/docs/a.zip")).toBe("~/docs/a.zip");
    expect(scrubHomePath("C:\\Users\\hamam\\docs\\a.zip")).toBe("~\\docs\\a.zip");
  });

  it("handles unicode usernames", () => {
    expect(scrubHomePath("/Users/مستخدم/ملفات/أرشيف.zip")).toBe("~/ملفات/أرشيف.zip");
    expect(scrubHomePath("C:\\Users\\José\\café.zip")).toBe("~\\café.zip");
  });

  it("leaves non-home paths and other users' homes alone", () => {
    expect(scrubHomePath("/var/log/system.log")).toBe("/var/log/system.log");
    expect(scrubHomePath("relative/path.zip")).toBe("relative/path.zip");
  });
});
