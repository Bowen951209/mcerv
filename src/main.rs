use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    mc_server_manager::run().await
}
