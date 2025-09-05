#[tokio::main]
async fn main() -> anyhow::Result<()> {
    multi_server::run().await
}
