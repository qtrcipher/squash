mod cli;
// Only INTERNAL is used by the Phase 1 shell; the rest are the documented
// contract consumed by Phase 2 dispatch.
#[allow(dead_code)]
mod exit_codes;

use clap::Parser;

fn main() {
    let args = cli::Cli::parse();

    // Phase 1 shell: parsing works, engine dispatch lands in Phase 2.
    let op = match &args.command {
        cli::Commands::Compress { .. } => "compress",
        cli::Commands::Extract { .. } => "extract",
    };
    eprintln!("squash: '{op}' is not yet implemented (Phase 2)");
    std::process::exit(exit_codes::INTERNAL);
}
