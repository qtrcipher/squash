//! Opt-in crash reporting (docs/06 §6, docs/01 §6.3 — owner decided: opt-in
//! Sentry, no silent telemetry).
//!
//! The trust contract (docs/02 differentiator):
//!
//! - **Default off.** `Settings::crash_reporting` defaults to `false` and the
//!   runtime consent gate ([`consent_given`]) starts unset. Without consent
//!   no Sentry client is ever created, so zero crash-reporting code runs and
//!   zero network is possible.
//! - **Build-time DSN.** The DSN comes from `SQUASH_SENTRY_DSN` at compile
//!   time ([`DSN`]) and is never committed. A build without it has the
//!   feature disabled at runtime: [`init`] returns `None` without touching
//!   the SDK, and the GUI shows the consent toggle disabled with a "not
//!   available in this build" note.
//! - **Scrubbing.** Every event passes the `before_send` filter: server name
//!   (hostname), user and breadcrumbs are dropped, and the user's home
//!   directory in any path or message is rewritten to `~`.
//!
//! The Sentry dependency itself lives behind the `crash-reporting` cargo
//! feature, enabled by the shells (GUI host + CLI) but not by `squash-bench`.

use std::sync::atomic::{AtomicBool, Ordering};

/// Sentry DSN baked in at build time via `SQUASH_SENTRY_DSN` (release CI sets
/// it; it is never committed). `None` or empty → crash reporting is
/// unavailable in this build.
pub const DSN: Option<&str> = option_env!("SQUASH_SENTRY_DSN");

/// Runtime consent gate. Checked in `before_send`, so revoking consent
/// mid-session drops every later event even if a client was initialized.
static CONSENT: AtomicBool = AtomicBool::new(false);

/// Whether this build can report crashes at all (non-empty DSN baked in).
pub fn available() -> bool {
    matches!(DSN, Some(dsn) if !dsn.trim().is_empty())
}

/// Current runtime consent state. The persisted value is
/// `Settings::crash_reporting`; this gate is what the event pipeline checks.
pub fn consent_given() -> bool {
    CONSENT.load(Ordering::SeqCst)
}

/// Flip the runtime consent gate (S6/S7 toggle).
pub fn set_consent(consent: bool) {
    CONSENT.store(consent, Ordering::SeqCst);
}

/// The Sentry release tag: app version, `squash@<version>` (docs/06 §6).
pub fn release_tag() -> String {
    format!("squash@{}", env!("CARGO_PKG_VERSION"))
}

/// The Sentry environment: dev builds vs signed releases.
pub fn environment() -> &'static str {
    if cfg!(debug_assertions) {
        "development"
    } else {
        "production"
    }
}

/// The user's home directory (`HOME`, else `USERPROFILE`), for scrubbing.
pub fn home_dir() -> Option<String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|v| v.to_string_lossy().into_owned())
        .filter(|v| !v.is_empty())
}

/// Replace the `home` prefix in `path` with `~` (pure, so tests can use
/// arbitrary and unicode homes). A bare home becomes `~`; a child path keeps
/// its remainder (`/home/x/f` → `~/f`). Either path separator is accepted.
pub fn scrub_home(path: &str, home: &str) -> String {
    if home.is_empty() {
        return path.to_string();
    }
    if path == home {
        return "~".to_string();
    }
    for sep in ['/', '\\'] {
        let prefix = format!("{home}{sep}");
        if let Some(rest) = path.strip_prefix(&prefix) {
            return format!("~{sep}{rest}");
        }
    }
    path.to_string()
}

/// [`scrub_home`] with the process home directory.
pub fn scrub_path(path: &str) -> String {
    match home_dir() {
        Some(home) => scrub_home(path, &home),
        None => path.to_string(),
    }
}

/// Replace every occurrence of the home directory inside free text (event
/// messages) with `~`.
pub fn scrub_text(text: &str) -> String {
    match home_dir() {
        Some(home) => text.replace(&home, "~"),
        None => text.to_string(),
    }
}

#[cfg(feature = "crash-reporting")]
mod sentry_integration {
    use super::{
        available, consent_given, environment, release_tag, scrub_path, scrub_text, set_consent,
        DSN,
    };
    use sentry::protocol::Event;
    use std::sync::Arc;

    pub use sentry::ClientInitGuard;

    /// Initialize the Sentry client iff `consent` is given AND this build has
    /// a DSN. Returns the client guard; `None` means nothing was initialized
    /// — the testable "off / unavailable → zero Sentry" seam.
    pub fn init(
        consent: bool,
        component: &'static str,
        locale: Option<&str>,
    ) -> Option<ClientInitGuard> {
        if !available() {
            // No DSN in this build: the feature is disabled and consent stays
            // off, so the pipeline never runs.
            set_consent(false);
            return None;
        }
        init_with_dsn(consent, component, locale, DSN)
    }

    /// [`init`] with the DSN injected, so option construction stays testable
    /// in builds without a baked-in DSN.
    fn init_with_dsn(
        consent: bool,
        component: &'static str,
        locale: Option<&str>,
        dsn: Option<&str>,
    ) -> Option<ClientInitGuard> {
        let dsn = match (consent, dsn) {
            (true, Some(dsn)) if !dsn.trim().is_empty() => dsn,
            // No consent or no usable DSN: no client, gate stays off.
            _ => {
                set_consent(false);
                return None;
            }
        };
        set_consent(true);
        let guard = sentry::init((dsn, client_options()));
        sentry::configure_scope(|scope| {
            scope.set_tag("component", component);
            scope.set_tag("os", std::env::consts::OS);
            scope.set_tag("arch", std::env::consts::ARCH);
            scope.set_tag("rar", if crate::FEATURE_RAR { "on" } else { "off" });
            if let Some(locale) = locale {
                scope.set_tag("locale", locale.to_owned());
            }
        });
        Some(guard)
    }

    /// Stop reporting immediately (S6 toggle off): the consent gate drops
    /// every later event and the client is unbound from the hub, so no
    /// transport can send anything from this point on.
    pub fn shutdown() {
        set_consent(false);
        sentry::Hub::main().bind_client(None);
    }

    /// The client options every shell shares: release/environment tags and
    /// the privacy filter. `send_default_pii` stays off.
    fn client_options() -> sentry::ClientOptions {
        // `ClientOptions` is non-exhaustive: default + field assignment.
        let mut options = sentry::ClientOptions::default();
        options.release = Some(release_tag().into());
        options.environment = Some(environment().into());
        options.send_default_pii = false;
        options.before_send = Some(Arc::new(scrub_event));
        options
    }

    /// `before_send`: the privacy filter every event passes through
    /// (docs/06 §6). Sends: stack trace, app version, OS/arch, enabled
    /// features, locale. Strips: hostname, user, breadcrumbs, and the home
    /// directory in any path (rewritten to `~`).
    fn scrub_event(mut event: Event<'static>) -> Option<Event<'static>> {
        // Consent revoked mid-session: drop everything.
        if !consent_given() {
            return None;
        }
        event.server_name = None;
        event.user = None;
        event.breadcrumbs = Default::default();
        for exception in &mut event.exception.values {
            if let Some(stacktrace) = &mut exception.stacktrace {
                for frame in &mut stacktrace.frames {
                    if let Some(filename) = &mut frame.filename {
                        *filename = scrub_path(filename);
                    }
                    if let Some(abs_path) = &mut frame.abs_path {
                        *abs_path = scrub_path(abs_path);
                    }
                }
            }
        }
        if let Some(message) = &mut event.message {
            *message = scrub_text(message);
        }
        Some(event)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use sentry::protocol::{Exception, Frame, Stacktrace, Values};
        use std::sync::Mutex;

        /// The consent gate is process-global: tests that flip it serialize.
        static TEST_LOCK: Mutex<()> = Mutex::new(());

        #[test]
        fn consent_off_initializes_nothing() {
            let _guard = TEST_LOCK.lock().unwrap();
            set_consent(false);
            // Even with a DSN injected, "no consent" must build no client.
            let guard = init_with_dsn(
                false,
                "test",
                None,
                Some("https://key@o0.ingest.sentry.io/0"),
            );
            assert!(guard.is_none(), "off → no client (zero network)");
            assert!(!consent_given());
            assert!(sentry::Hub::current().client().is_none());
        }

        #[test]
        fn missing_or_blank_dsn_initializes_nothing() {
            let _guard = TEST_LOCK.lock().unwrap();
            set_consent(false);
            assert!(init_with_dsn(true, "test", None, None).is_none());
            assert!(init_with_dsn(true, "test", None, Some("  ")).is_none());
            // Blank DSN counts as unavailable, consent gate stays off.
            assert!(!consent_given());
        }

        #[test]
        fn client_options_tag_release_environment_and_install_the_filter() {
            let options = client_options();
            assert_eq!(options.release.as_deref(), Some(release_tag().as_str()));
            // Tests are debug builds.
            assert_eq!(options.environment.as_deref(), Some("development"));
            assert!(!options.send_default_pii);
            assert!(options.before_send.is_some());
        }

        #[test]
        fn before_send_strips_host_user_breadcrumbs_and_home_paths() {
            let _guard = TEST_LOCK.lock().unwrap();
            set_consent(true);
            let home = home_dir_or_fallback();
            let event = Event {
                server_name: Some("my-macbook".into()),
                message: Some(format!("failed to read {home}/docs/a.zip")),
                breadcrumbs: Values {
                    values: vec![Default::default()],
                },
                exception: Values {
                    values: vec![Exception {
                        stacktrace: Some(Stacktrace {
                            frames: vec![Frame {
                                filename: Some(format!("{home}/src/main.rs")),
                                abs_path: Some(format!("{home}/src/main.rs")),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                },
                ..Default::default()
            };
            let scrubbed = scrub_event(event).expect("consent given → event kept");
            assert_eq!(scrubbed.server_name, None);
            assert_eq!(scrubbed.user, None);
            assert!(scrubbed.breadcrumbs.values.is_empty());
            let frame = &scrubbed.exception.values[0]
                .stacktrace
                .as_ref()
                .unwrap()
                .frames[0];
            assert_eq!(frame.filename.as_deref(), Some("~/src/main.rs"));
            assert_eq!(frame.abs_path.as_deref(), Some("~/src/main.rs"));
            assert_eq!(
                scrubbed.message.as_deref(),
                Some("failed to read ~/docs/a.zip")
            );
            set_consent(false);
        }

        #[test]
        fn before_send_drops_events_once_consent_is_revoked() {
            let _guard = TEST_LOCK.lock().unwrap();
            set_consent(false);
            assert!(scrub_event(Event::default()).is_none());
        }

        /// Scrub tests run against the real process home — every dev machine
        /// and CI runner sets `HOME`/`USERPROFILE`.
        fn home_dir_or_fallback() -> String {
            super::super::home_dir().expect("tests run with HOME/USERPROFILE set")
        }
    }
}

#[cfg(feature = "crash-reporting")]
pub use sentry_integration::{init, shutdown, ClientInitGuard};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsn_availability_matches_the_build_time_env() {
        // Dev/test builds bake in no DSN (it is never committed), so the
        // feature is unavailable here; the assertion stays correct if a DSN
        // is ever set for a test run.
        assert_eq!(available(), DSN.is_some_and(|d| !d.trim().is_empty()));
    }

    #[test]
    fn consent_gate_defaults_off_and_flips() {
        assert!(!consent_given());
        set_consent(true);
        assert!(consent_given());
        set_consent(false);
        assert!(!consent_given());
    }

    #[test]
    fn scrub_home_replaces_the_home_prefix() {
        assert_eq!(
            scrub_home("/home/nadia/docs/a.zip", "/home/nadia"),
            "~/docs/a.zip"
        );
        assert_eq!(scrub_home("/home/nadia", "/home/nadia"), "~");
        // Windows-style separators.
        assert_eq!(
            scrub_home("C:\\Users\\nadia\\docs\\a.zip", "C:\\Users\\nadia"),
            "~\\docs\\a.zip"
        );
        // A path merely containing the home string as a non-prefix is kept.
        assert_eq!(
            scrub_home("/var/backups/home/nadia.tar", "/home/nadia"),
            "/var/backups/home/nadia.tar"
        );
        // Sibling dirs with a shared prefix are not home.
        assert_eq!(
            scrub_home("/home/nadia2/x", "/home/nadia"),
            "/home/nadia2/x"
        );
        assert_eq!(scrub_home("/etc/hosts", "/home/nadia"), "/etc/hosts");
        assert_eq!(scrub_home("/home/nadia/x", ""), "/home/nadia/x");
    }

    #[test]
    fn scrub_home_handles_unicode_homes() {
        assert_eq!(
            scrub_home("/Users/مستخدم/ملفات/أرشيف.zip", "/Users/مستخدم"),
            "~/ملفات/أرشيف.zip"
        );
        assert_eq!(
            scrub_home("/home/josé/café.zip", "/home/josé"),
            "~/café.zip"
        );
    }
}
