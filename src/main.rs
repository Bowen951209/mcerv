use clap::Parser;
use mcerv::{instances_dir, system::cli::Cli};
use std::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fs::create_dir_all(instances_dir()).expect("Unable to create instances directory");
    Cli::parse().command.run().await
}
