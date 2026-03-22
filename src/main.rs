use clap::Parser;

mod cache;
mod claude;
mod cli;
mod marker;
mod prompt;

fn main() {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Run {
            root,
            model,
            diff,
            no_cache,
        } => cli::run(&model, diff.as_deref(), no_cache, root.as_deref()),
    }
}
