use std::{
    fs::{File, create_dir_all},
    io::copy,
    path::Path,
};

use anyhow::{Result, anyhow};
use prettytable::{Table, row};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};

#[derive(Deserialize)]
#[allow(unused)]
pub struct MinecraftVersion {
    version: String,
    stable: bool,
}

#[derive(Deserialize)]
#[allow(unused)]
pub struct FabricLoaderVersion {
    separator: String,
    build: i32,
    maven: String,
    version: String,
    stable: bool,
}

#[derive(Deserialize)]
#[allow(unused)]
pub struct FabricInstallerVersion {
    url: String,
    maven: String,
    version: String,
    stable: bool,
}

#[derive(Copy, Clone)]
pub enum PrintVersionMode {
    All,
    StableOnly,
}

pub async fn download_server(
    game_version: &str,
    fabric_loader_version: &str,
    installer_version: &str,
    server_name: &str,
) -> Result<String> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
        game_version, fabric_loader_version, installer_version
    );

    let client = Client::new();

    let response = client.head(&url).send().await?;

    if response.status() != StatusCode::OK {
        return Err(anyhow!(
            "Failed to fetch server jar. Probably invalid versions.".to_string()
        ));
    }

    let filename = format!(
        "fabric-server-mc.{}-loader.{}-launcher.{}.jar",
        game_version, fabric_loader_version, installer_version
    );
    let path_string = format!("instances/{}/{}", server_name, filename);
    let out_path = Path::new(&path_string);
    create_dir_all(out_path.parent().expect("Failed to get parent directory"))?;
    let mut out_file = File::create(&out_path)?;
    let content = response.bytes().await?;
    copy(&mut content.as_ref(), &mut out_file)?;

    Ok(filename)
}

pub async fn print_versions(print_mode: PrintVersionMode) -> Result<()> {
    let mut table = Table::new();

    table.add_row(row![
        "Minecraft Version",
        "Fabric Loader Version",
        "Installer Version"
    ]);

    let (minecraft_versions, fabric_loader_versions, installer_versions) = tokio::try_join!(
        fetch_json::<Vec<MinecraftVersion>>("https://meta.fabricmc.net/v2/versions/game"),
        fetch_json::<Vec<FabricLoaderVersion>>("https://meta.fabricmc.net/v2/versions/loader"),
        fetch_json::<Vec<FabricInstallerVersion>>(
            "https://meta.fabricmc.net/v2/versions/installer"
        ),
    )?;

    let minecraft_versions =
        filter_and_format(minecraft_versions, print_mode, |v| &v.version, |v| v.stable);

    let loader_versions = filter_and_format(
        fabric_loader_versions,
        print_mode,
        |v| &v.version,
        |v| v.stable,
    );

    let installer_versions =
        filter_and_format(installer_versions, print_mode, |v| &v.version, |v| v.stable);

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

async fn fetch_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let client = Client::new();
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch {}: {}", url, response.status());
    }

    let text = response.text().await?;
    let result = serde_json::from_str::<T>(&text)?;
    Ok(result)
}

fn filter_and_format<T>(
    versions: Vec<T>,
    print_mode: PrintVersionMode,
    get_version: impl Fn(&T) -> &str,
    is_stable: impl Fn(&T) -> bool,
) -> Vec<String> {
    versions
        .into_iter()
        .filter(|v| match print_mode {
            PrintVersionMode::All => true,
            PrintVersionMode::StableOnly => is_stable(v),
        })
        .map(|v| {
            format!(
                "{} ({})",
                get_version(&v),
                if is_stable(&v) { "stable" } else { "unstable" }
            )
        })
        .collect()
}
