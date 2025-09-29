use crate::network::{PrintVersionMode, download_file};
use anyhow::anyhow;
use core::panic;
use reqwest::Client;
use std::{error::Error, path::Path};

const URL: &str = "https://gist.githubusercontent.com/cliffano/77a982a7503669c3e1acb0a0cf6127e9/raw/minecraft-server-jar-downloads.md";

#[derive(Debug)]
pub enum DownloadError {
    VersionNotFound,
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::VersionNotFound => write!(f, "Version not found"),
        }
    }
}

impl Error for DownloadError {}

pub async fn download_server(
    client: &Client,
    version: &str,
    save_dir_path: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let content = client.get(URL).send().await?.text().await?;
    let url = versions_and_download_links(&content)
        .find(|(v, _)| *v == version)
        .ok_or(anyhow!(DownloadError::VersionNotFound))?
        .1;

    let filename = format!("vanilla-{version}.jar");
    download_file(client, &url, &save_dir_path.as_ref().join(&filename)).await?;
    Ok(filename)
}

pub async fn versions(
    client: &reqwest::Client,
    print_mode: PrintVersionMode,
) -> anyhow::Result<String> {
    let content = client.get(URL).send().await?.text().await?;
    let versions = versions_and_download_links(&content)
        .filter_map(|(version, _)| {
            if matches!(print_mode, PrintVersionMode::StableOnly) && is_unstable_version(version) {
                None
            } else {
                Some(version)
            }
        })
        .collect::<Vec<_>>();

    Ok(versions.join("\n"))
}

pub async fn fetch_latest_stable_version(client: &reqwest::Client) -> anyhow::Result<String> {
    let content = client.get(URL).send().await?.text().await?;
    for (version, _) in versions_and_download_links(&content) {
        if is_stable_version(version) {
            return Ok(version.to_string());
        }
    }

    panic!("Could not find any stable versions in the fetched data");
}

fn versions_and_download_links(content: &str) -> impl Iterator<Item = (&str, &str)> {
    content.lines().skip(2).filter_map(|line| {
        let mut columns = line.split('|');
        let version_name = columns.nth(1).unwrap().trim();
        let server_jar_url = columns.next().unwrap().trim();

        if server_jar_url == "Not found" {
            None
        } else {
            Some((version_name, server_jar_url))
        }
    })
}

fn is_stable_version(version: &str) -> bool {
    !is_unstable_version(version)
}

fn is_unstable_version(version: &str) -> bool {
    version.contains('-') || version.contains('w')
}
