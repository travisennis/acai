use std::path::Path;

use log::{LevelFilter, SetLoggerError};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Logger, Root},
    encode::pattern::PatternEncoder,
    filter::threshold::ThresholdFilter,
    Config,
};

// error!("Goes to stderr and file");
// warn!("Goes to stderr and file");
// info!("Goes to stderr and file");
// debug!("Goes to file only");
// trace!("Goes to file only");
pub fn configure(log_path: &Path) -> Result<(), SetLoggerError> {
    let log_line_pattern = "{d(%Y-%m-%d %H:%M:%S)} | {({l}):5.5} | {f}:{L} — {m}{n}\n";
    let level = LevelFilter::Info;
    let file_path = log_path.join("acai.log");

    // Build a stderr logger.
    let stderr = ConsoleAppender::builder().target(Target::Stderr).build();

    // Logging to log file.
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(log_line_pattern)))
        .build(file_path)
        .unwrap();

    // Log Trace level output to file where trace is the default level
    // and the programmatically specified level to stderr.
    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(level)))
                .build("stderr", Box::new(stderr)),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .build("acai", LevelFilter::Trace),
        )
        .build(Root::builder().appender("stderr").build(LevelFilter::Trace))
        .unwrap();

    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let _handle = log4rs::init_config(config)?;

    Ok(())
}
