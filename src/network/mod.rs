use std::fs::{self, File};

use reqwest::StatusCode;

pub mod fabric_meta;
pub mod modrinth;

use anyhow::anyhow;

pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    save_path: impl AsRef<std::path::Path>,
) -> anyhow::Result<()> {
    let response = client.get(url).send().await?;
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

fn display_json_value(json: &serde_json::Value, key: &str) -> String {
    match json.get(key) {
        Some(value) => format!("{key}: {value}"),
        None => format!("{key}: N/A"),
    }
}
