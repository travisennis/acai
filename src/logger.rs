use std::path::Path;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to set logger: {0}")]
    SetLogger(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("Failed to initialize log file: {0}")]
    Init(#[from] tracing_appender::rolling::InitError),
}

/// Configure the logging system.
///
/// # Arguments
/// * `log_path` - Directory where the log file will be created
///
/// All logs go to the log file only. No stderr output.
/// This keeps user-facing output clean while preserving detailed logs for debugging.
///
/// Log level is controlled by the `RUST_LOG` environment variable.
/// If not set, defaults to INFO level for the cake crate.
/// Set `RUST_LOG=cake=trace` for verbose debugging.
///
/// # Log File Naming
///
/// With daily rotation, log files are named `cake.YYYY-MM-DD.log` where the date
/// is the current day. For example, logs for March 29, 2026 are written to
/// `cake.2026-03-29.log`. At midnight, a new file is created for the next day.
///
/// There is no "current" log file without a date - the dated file IS the current
/// log file for that day. This is the standard behavior of tracing-appender's
/// rolling file appender.
///
/// Log files are retained for up to 7 days.
pub fn configure(log_path: &Path) -> Result<(), Error> {
    // Default to INFO level for cake, but allow RUST_LOG to override
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("cake=info"));

    // Daily rotation with 7-day retention
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("cake")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_path)?;

    let subscriber = tracing_subscriber::registry().with(env_filter).with(
        fmt::layer()
            .with_writer(file_appender)
            .with_target(false)
            .with_thread_ids(false)
            .with_line_number(true)
            .with_timer(fmt::time::time()),
    );

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}
