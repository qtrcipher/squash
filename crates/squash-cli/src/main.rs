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

fn main() {
    let args = cli::Cli::parse();
    init_logging(args.verbose);
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
    std::process::exit(run::run(args));
}
