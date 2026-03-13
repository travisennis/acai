//! acai - AI coding assistant CLI

mod cli;
mod clients;
mod config;
mod logger;
mod models;
mod prompts;

use crate::cli::CmdRunner;
use clap::Parser;
use clap::Subcommand;
use cli::instruct;
use config::DataDir;
use log::info;

/// coding assistant commands
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CodingAssistant {
    #[command(subcommand)]
    pub cmd: CodingAssistantCmd,
}

#[derive(Clone, Subcommand)]
enum CodingAssistantCmd {
    Instruct(instruct::Cmd),
}

/// Check if we should use quiet logging (no stderr output).
/// This is true when using machine-readable output formats like stream-json.
fn should_use_quiet_logging() -> bool {
    // Parse args to check for --output-format stream-json
    // We use try_parse to avoid panicking on invalid args (error will be shown later)
    CodingAssistant::try_parse()
        .map(|args| {
            matches!(
                args.cmd,
                CodingAssistantCmd::Instruct(ref cmd) if cmd.output_format == instruct::OutputFormat::StreamJson
            )
        })
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = DataDir::new()?;

    // Determine if we should use quiet logging before configuring the logger.
    // This ensures log messages don't pollute machine-readable output.
    let quiet = should_use_quiet_logging();

    let _ = logger::configure(&data_dir.get_cache_dir(), quiet);

    info!("data dir: {}", data_dir.get_cache_dir().display());

    let args = CodingAssistant::parse();

    match args.cmd {
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run(&data_dir).await?,
    }

    Ok(())
}
