mod cache;
mod claude;
mod cli;
mod marker;
mod prompt;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Run {
            model,
            diff,
            no_cache,
        } => cli::run(&model, diff.as_deref(), no_cache),
    }
}
