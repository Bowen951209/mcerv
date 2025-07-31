use std::{
    fs::{self, File},
    path::PathBuf,
};

use reqwest::StatusCode;

pub mod fabric_meta;
pub mod modrinth;

use anyhow::anyhow;
use tokio::task::JoinSet;

pub async fn download_file(
    client: &reqwest::Client,
    url: &impl AsRef<str>,
    save_path: &impl AsRef<std::path::Path>,
) -> anyhow::Result<()> {
    let response = client.get(url.as_ref()).send().await?;
    let status = response.status();

    if status != StatusCode::OK {
        return Err(anyhow!(status));
    }

    fs::create_dir_all(
        save_path
            .as_ref()
            .parent()
            .expect("save_path parent is not available."),
    )?;
    let mut file = File::create(save_path.as_ref())?;
    let content = response.bytes().await?;
    std::io::copy(&mut content.as_ref(), &mut file)?;

    Ok(())
}

pub async fn download_files(
    client: &reqwest::Client,
    downloads: impl Iterator<Item = (String, PathBuf)>, // (url, save_path) pairs
) -> anyhow::Result<()> {
    let mut join_set = JoinSet::new();

    for (url, save_path) in downloads {
        let client = client.clone();
        join_set.spawn(async move { download_file(&client, &url, &save_path).await });
    }

    while let Some(result) = join_set.join_next().await {
        result??;
    }

    Ok(())
}

fn display_json_value(json: &serde_json::Value, key: &str) -> String {
    match json.get(key) {
        Some(value) => format!("{key}: {value}"),
        None => format!("{key}: N/A"),
    }
}
