use tower_lsp::{LspService, Server};

use super::backend::Backend;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

pub async fn run() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
