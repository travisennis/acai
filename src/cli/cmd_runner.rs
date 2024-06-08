use std::error::Error;

pub trait CmdRunner {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
}
