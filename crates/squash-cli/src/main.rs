mod cli;
mod exit_codes;
mod run;

use clap::Parser;

/// Install the `log` sink: env_logger on **stderr** (stdout stays pipe-clean
/// per the `--json` contract). Default is warn-level (user-facing only);
/// `-v`/`--verbose` or the `SQUASH_LOG` filter (e.g. `SQUASH_LOG=debug`)
/// turns on the detailed stream. A set `SQUASH_LOG` wins over the flag.
fn init_logging(verbose: bool) {
    let default_filter = if verbose { "debug" } else { "warn" };
    env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("SQUASH_LOG", default_filter),
    )
    .format_timestamp_millis()
    .init();
}

/// `SQUASH_CRASH_REPORTING` — explicit per-run crash-reporting opt-in for
/// hermetic runs (docs/06 §6): "1"/"true"/"yes" (case-insensitive) enable it;
/// anything else, including unset, does not.
fn env_crash_consent(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes")
    )
}

/// Crash-reporting consent for a CLI run (docs/06 §6). The env var is an
/// explicit per-run opt-in that wins even over `--no-config`; otherwise the
/// persisted setting applies, unless `--no-config` was given. Anything that
/// fails to resolve or parse fails closed — no consent, no client.
fn crash_consent(no_config: bool, env: Option<&str>) -> bool {
    if env_crash_consent(env) {
        return true;
    }
    if no_config {
        return false;
    }
    squash_core::store::StoreDirs::resolve()
        .and_then(|dirs| squash_core::store::load_settings(&dirs.config_dir).ok())
        .map(|outcome| outcome.value.crash_reporting)
        .unwrap_or(false)
}

fn main() {
    let args = cli::Cli::parse();
    init_logging(args.verbose);
    // Opt-in crash reporting (docs/06 §6): without consent (the default) or
    // a baked-in DSN this is `None` and no Sentry client ever exists.
    let crash_guard = squash_core::crash::init(
        crash_consent(
            args.no_config,
            std::env::var("SQUASH_CRASH_REPORTING").ok().as_deref(),
        ),
        "cli",
        None,
    );
    log::debug!(
        "squash {} ({} {}, rar {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if squash_core::FEATURE_RAR {
            "on"
        } else {
            "off"
        },
    );
    let code = run::run(args);
    if let Some(guard) = crash_guard {
        // Flush the queue before exit so a captured crash actually ships.
        guard.close(Some(std::time::Duration::from_secs(2)));
    }
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_opt_in_parsing() {
        assert!(env_crash_consent(Some("1")));
        assert!(env_crash_consent(Some("true")));
        assert!(env_crash_consent(Some("TRUE")));
        assert!(env_crash_consent(Some(" yes ")));
        assert!(!env_crash_consent(None));
        assert!(!env_crash_consent(Some("")));
        assert!(!env_crash_consent(Some("0")));
        assert!(!env_crash_consent(Some("false")));
        assert!(!env_crash_consent(Some("enabled")));
    }

    #[test]
    fn no_config_without_env_override_means_no_consent() {
        // Hermetic run: settings.toml is never read, so consent is false
        // regardless of what it contains.
        assert!(!crash_consent(true, None));
        assert!(!crash_consent(true, Some("0")));
        // …but an explicit per-run opt-in still wins (docs/06 §6).
        assert!(crash_consent(true, Some("1")));
    }
}
