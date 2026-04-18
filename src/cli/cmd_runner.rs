use crate::config::DataDir;

/// A trait representing a command runner.
///
/// This trait defines the interface for commands that can be executed by the CLI.
/// Implementations handle command-specific logic, service interactions, and
/// necessary actions based on the command's purpose.
///
/// # Examples
///
/// ```rust
/// use cake::cli::CmdRunner;
/// use cake::config::DataDir;
///
/// struct MyCommand;
///
/// impl CmdRunner for MyCommand {
///     async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()> {
///         println!("Running MyCommand!");
///         Ok(())
///     }
/// }
/// ```
pub trait CmdRunner {
    /// Executes the command's logic.
    ///
    /// # Errors
    ///
    /// Returns an error if the command execution fails.
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()>;
}
