mod cli;
mod commands;
mod fossil;
mod git;
mod lock;
mod manifest;
mod pathutil;
mod snapshot;

use clap::Parser;
use cli::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(err) = commands::run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
