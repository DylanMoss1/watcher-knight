use clap::Parser;

fn main() {
    let cli = watcher_knight::cli::Cli::parse();
    match cli.command {
        watcher_knight::cli::Command::Run {
            root,
            model,
            diff,
            no_cache,
        } => watcher_knight::cli::run(&model, diff.as_deref(), no_cache, root.as_deref()),
    }
}
