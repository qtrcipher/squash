//! Benchmark harness binary (docs/05 §6). All logic lives in the library so
//! the integration tests drive the same code paths.

use clap::Parser;
use squash_bench::cli::{self, Cli};

fn main() {
    std::process::exit(cli::run_cli(Cli::parse()));
}
