mod cli;
mod exit_codes;
mod run;

use clap::Parser;

fn main() {
    std::process::exit(run::run(cli::Cli::parse()));
}
