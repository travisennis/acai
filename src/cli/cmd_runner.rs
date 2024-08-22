use std::error::Error;

/// A trait representing a command runner.
///
/// This trait defines a single method, `run`, which is responsible for executing the logic of a specific command.
/// Implementations of this trait are responsible for handling command-specific arguments, interacting with
/// the appropriate services or APIs, and performing the necessary actions based on the command's purpose.
///
/// # Example
///
/// ```rust
/// use std::error::Error;
///
/// #[derive(Debug)]
/// struct MyCommand;
///
/// impl CmdRunner for MyCommand {
///     async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
///         println!("Running MyCommand!");
///         Ok(())
///     }
/// }
/// ```
pub trait CmdRunner {
    /// Executes the command's logic.
    ///
    /// This method is responsible for handling command-specific arguments, interacting with
    /// the appropriate services or APIs, and performing the necessary actions based on the command's purpose.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating whether the command execution was successful.
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
}
