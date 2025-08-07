use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use directories::ProjectDirs;
use tracing::info;

pub static DEFAULT_MESSAGES_LOG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut result: PathBuf = proj_dirs.data_dir().to_path_buf();
    let filename: &str = "messages.json";

    result.push(filename);
    Mutex::new(result)
});

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: CLIArgs = CLIArgs::parse();

    match args.action {
        Actions::Messages => messages(),
    }
}

fn messages() -> Result<(), Box<dyn std::error::Error>> {
    todo!();
}

#[derive(Debug, Parser)]
#[clap(version, author, about = env!("CARGO_PKG_DESCRIPTION"))]
struct CLIArgs {
    #[clap(subcommand)]
    action: Actions,

    #[arg(long, default_value = DEFAULT_MESSAGES_LOG_PATH.lock().unwrap().display().to_string())]
    messages_log_path: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
enum Actions {
    Messages,
}
