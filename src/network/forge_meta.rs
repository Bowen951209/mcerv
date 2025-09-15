use std::path::Path;

use reqwest::Client;
use roxmltree::Document;

use crate::network::{download_file, fetch_text};

pub async fn download_installer(
    client: &Client,
    version: &str,
    save_dir_path: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let filename = format!("forge-{version}-installer.jar");
    let url =
        format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/{filename}");

    download_file(client, &url, &save_dir_path.as_ref().join(&filename)).await?;

    Ok(filename)
}

pub async fn versions(client: &Client) -> anyhow::Result<String> {
    let url = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
    let text = fetch_text(client, url).await?;
    let doc = Document::parse(&text)?;
    let versions = doc
        .descendants()
        .filter(|node| node.has_tag_name("version"))
        .filter_map(|node| node.text().map(String::from))
        .collect::<Vec<_>>();

    Ok(versions.join("\n"))
}

pub async fn fetch_latest_version(client: &Client) -> anyhow::Result<String> {
    let url = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
    let text = fetch_text(client, url).await?;
    let doc = Document::parse(&text)?;
    let latest_version = doc
        .descendants()
        .find(|node| node.has_tag_name("latest"))
        .and_then(|node| node.text().map(String::from))
        .ok_or_else(|| anyhow::anyhow!("Latest version not found in metadata"))?;

    Ok(latest_version)
}
