use std::path::Path;

use anyhow::{Result, anyhow};
use prettytable::{Table, row};
use serde::de::DeserializeOwned;

use crate::network::download_file;

#[derive(Copy, Clone)]
pub enum PrintVersionMode {
    All,
    StableOnly,
}

pub async fn download_server(
    client: &reqwest::Client,
    game_version: &str,
    fabric_loader_version: &str,
    installer_version: &str,
    save_dir_path: impl AsRef<Path>,
) -> Result<String> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{game_version}/{fabric_loader_version}/{installer_version}/server/jar"
    );

    let filename = format!(
        "fabric-server-mc.{game_version}-loader.{fabric_loader_version}-launcher.{installer_version}.jar"
    );

    download_file(client, &url, &save_dir_path.as_ref().join(&filename)).await?;

    Ok(filename)
}

pub async fn print_versions(client: &reqwest::Client, print_mode: PrintVersionMode) -> Result<()> {
    let mut table = Table::new();

    table.add_row(row![
        "Minecraft Version",
        "Fabric Loader Version",
        "Installer Version"
    ]);

    let (minecraft_versions, fabric_loader_versions, installer_versions) =
        get_versions(client).await?;

    let minecraft_versions = filter_and_format(minecraft_versions, print_mode);
    let loader_versions = filter_and_format(fabric_loader_versions, print_mode);
    let installer_versions = filter_and_format(installer_versions, print_mode);

    let length = minecraft_versions
        .len()
        .max(loader_versions.len())
        .max(installer_versions.len());

    for i in 0..length {
        table.add_row(row![
            minecraft_versions.get(i).unwrap_or(&"-".to_string()),
            loader_versions.get(i).unwrap_or(&"-".to_string()),
            installer_versions.get(i).unwrap_or(&"-".to_string())
        ]);
    }
    table.printstd();

    Ok(())
}

pub async fn fetch_latest_stable_versions(
    client: &reqwest::Client,
) -> Result<(String, String, String)> {
    let (minecraft_versions, fabric_loader_versions, installer_versions) =
        get_versions(client).await?;

    let minecraft_version = minecraft_versions
        .into_iter()
        .find(|v| v["stable"].as_bool().unwrap())
        .ok_or(anyhow!("Failed to find stable minecraft version"))?["version"]
        .as_str()
        .unwrap()
        .to_string();
    let fabric_loader_version = fabric_loader_versions
        .into_iter()
        .find(|v| v["stable"].as_bool().unwrap())
        .ok_or(anyhow!("Failed to find stable fabric loader version"))?["version"]
        .as_str()
        .unwrap()
        .to_string();
    let installer_version = installer_versions
        .into_iter()
        .find(|v| v["stable"].as_bool().unwrap())
        .ok_or(anyhow!("Failed to find stable fabric installer version"))?["version"]
        .as_str()
        .unwrap()
        .to_string();

    Ok((minecraft_version, fabric_loader_version, installer_version))
}

async fn get_versions(
    client: &reqwest::Client,
) -> Result<(
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
    Vec<serde_json::Value>,
)> {
    tokio::try_join!(
        fetch_json(client, "https://meta.fabricmc.net/v2/versions/game"),
        fetch_json(client, "https://meta.fabricmc.net/v2/versions/loader"),
        fetch_json(client, "https://meta.fabricmc.net/v2/versions/installer"),
    )
}

async fn fetch_json<T: DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch {}: {}", url, response.status());
    }

    let text = response.text().await?;
    let result = serde_json::from_str::<T>(&text)?;
    Ok(result)
}

fn filter_and_format(
    versions: Vec<serde_json::Value>,
    print_mode: PrintVersionMode,
) -> Vec<String> {
    versions
        .into_iter()
        .filter(|v| match print_mode {
            PrintVersionMode::All => true,
            PrintVersionMode::StableOnly => v["stable"].as_bool().unwrap(),
        })
        .map(|v| {
            format!(
                "{} ({})",
                v["version"].as_str().unwrap(),
                if v["stable"].as_bool().unwrap() {
                    "stable"
                } else {
                    "unstable"
                }
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_latest_stable_versions() {
        let client = reqwest::Client::new();
        let versions = fetch_latest_stable_versions(&client).await;

        assert!(versions.is_ok());

        let (game, loader, installer) = versions.unwrap();

        assert!(!game.is_empty());
        assert!(!loader.is_empty());
        assert!(!installer.is_empty());
    }

    #[tokio::test]
    async fn test_get_versions() {
        let client = reqwest::Client::new();
        let versions = get_versions(&client).await;

        assert!(versions.is_ok());

        let (minecraft_versions, fabric_loader_versions, installer_versions) = versions.unwrap();

        assert!(!minecraft_versions.is_empty());
        assert!(!fabric_loader_versions.is_empty());
        assert!(!installer_versions.is_empty());
    }
}
