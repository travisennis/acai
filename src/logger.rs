use std::path::Path;

use log::{LevelFilter, SetLoggerError};
use log4rs::{
    Config,
    append::file::FileAppender,
    config::{Appender, Logger, Root},
    encode::pattern::PatternEncoder,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to set logger: {0}")]
    SetLogger(#[from] SetLoggerError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Configure the logging system.
///
/// # Arguments
/// * `log_path` - Directory where the log file will be created
///
/// All logs go to the log file only. No stderr output.
/// This keeps user-facing output clean while preserving detailed logs for debugging.
pub fn configure(log_path: &Path) -> Result<(), Error> {
    let log_line_pattern = "{d(%Y-%m-%d %H:%M:%S)} | {({l}):5.5} | {f}:{L} — {m}{n}\n";
    let file_path = log_path.join("acai.log");

    // Logging to log file.
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(log_line_pattern)))
        .build(file_path)
        .map_err(|e| Error::Configuration(format!("Failed to build file appender: {e}")))?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .logger(
            Logger::builder()
                .appender("logfile")
                .build("acai", LevelFilter::Trace),
        )
        .build(Root::builder().build(LevelFilter::Trace))
        .map_err(|e| Error::Configuration(format!("Failed to build config: {e}")))?;

    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let _handle = log4rs::init_config(config)?;

    Ok(())
}
