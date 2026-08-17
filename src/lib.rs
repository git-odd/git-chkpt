pub mod cli;
pub mod commands;
pub mod fossil;
pub mod git;
pub mod lock;
pub mod manifest;
pub mod pathutil;
pub mod snapshot;

use clap::Parser;
use cli::Cli;

pub fn run() {
    let cli = Cli::parse();
    if let Err(err) = commands::run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
