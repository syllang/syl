#[tokio::main]
async fn main() {
    syl_lsp::SylLspServerRunner::new().serve().await;
}
