use std::error::Error;

use super::CmdConfig;

pub trait CmdRunner {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>>;
}
