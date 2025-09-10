use crate::{
    network::fabric_meta::{self, PrintVersionMode},
    server_dir,
    system::{jar_parser, server_info::ServerFork},
};
use anyhow::anyhow;
use reqwest::Client;
use std::{error::Error, fmt::Display, fs::File, io::BufReader};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub enum DetectServerInfoError {
    MainClassNotFound,
    UnknownServerFork,
    GameVersionNotFound,
}

impl Display for DetectServerInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectServerInfoError::MainClassNotFound => {
                write!(f, "Main-Class not found in MANIFEST.MF")
            }
            DetectServerInfoError::UnknownServerFork => {
                write!(
                    f,
                    "Detected an unknown server fork. Probably not supported by mcerv"
                )
            }
            DetectServerInfoError::GameVersionNotFound => {
                write!(f, "Game version not found in install.properties")
            }
        }
    }
}

impl Error for DetectServerInfoError {}

pub trait Fork {
    type FetchConfig;
    type Version;

    fn is_this_fork(main_class: &str) -> bool;

    fn game_version(archive: &mut ZipArchive<BufReader<&File>>) -> anyhow::Result<String>;

    async fn install(
        server_name: &str,
        version: Self::Version,
        request_client: &Client,
    ) -> anyhow::Result<String>;

    async fn fetch_availables(
        config: Self::FetchConfig,
        request_client: &Client,
    ) -> anyhow::Result<String>;
}

pub struct Fabric;

impl Fork for Fabric {
    type FetchConfig = bool;
    type Version = (String, String, String); // (game_version, loader_version, installer_version)

    fn is_this_fork(main_class: &str) -> bool {
        main_class.contains("net.fabricmc")
    }

    fn game_version(archive: &mut ZipArchive<BufReader<&File>>) -> anyhow::Result<String> {
        // Game version property is stored in `install.properties`
        let content = jar_parser::read_file(archive, "install.properties")?;
        let mut install_properties = jar_parser::parse_properties(&content);

        let version = install_properties
            .remove("game-version") // Use remove to get owned String
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;

        return Ok(version);
    }

    async fn install(
        server_name: &str,
        version: Self::Version,
        request_client: &Client,
    ) -> anyhow::Result<String> {
        let server_dir = server_dir(server_name);

        fabric_meta::download_server(
            request_client,
            &version.0,
            &version.1,
            &version.2,
            &server_dir,
        )
        .await
    }

    async fn fetch_availables(all: bool, request_client: &Client) -> anyhow::Result<String> {
        let mode = if all {
            PrintVersionMode::All
        } else {
            PrintVersionMode::StableOnly
        };

        fabric_meta::versions(request_client, mode).await
    }
}

pub struct Forge;

impl Fork for Forge {
    type FetchConfig = ();
    type Version = ();

    fn is_this_fork(main_class: &str) -> bool {
        main_class.contains("net.minecraftforge")
    }

    fn game_version(archive: &mut ZipArchive<BufReader<&File>>) -> anyhow::Result<String> {
        // Game version property is stored in `bootstrap-shim.list`
        // The line format goes like:
        // HASH net.minecraftforge:forge:1.21.8-58.1.0:server net/minecraftforge/forge/1.21.8-58.1.0/forge-1.21.8-58.1.0-server.jar

        let content = jar_parser::read_file(archive, "bootstrap-shim.list")?;
        let line = content
            .lines()
            .find(|line| line.contains("net.minecraftforge:forge:") && line.contains(":server"))
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;
        let long_version = line
            .split(':')
            .nth(2)
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;
        let game_version = long_version
            .split('-')
            .next()
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;

        Ok(game_version.to_string())
    }

    async fn install(
        _server_name: &str,
        _version: Self::Version,
        _request_client: &Client,
    ) -> anyhow::Result<String> {
        todo!()
    }

    async fn fetch_availables(_config: (), _request_client: &Client) -> anyhow::Result<String> {
        todo!()
    }
}

pub fn detect_server_fork(
    archive: &mut ZipArchive<BufReader<&File>>,
) -> anyhow::Result<ServerFork> {
    let content = jar_parser::read_file(archive, "META-INF/MANIFEST.MF")?;
    let manifest = jar_parser::parse_manifest(&content);
    let main_class = manifest
        .get("Main-Class")
        .ok_or(anyhow!(DetectServerInfoError::MainClassNotFound))?;

    // loop through all structs that implements Fork
    if Fabric::is_this_fork(main_class) {
        return Ok(ServerFork::Fabric);
    } else if Forge::is_this_fork(main_class) {
        return Ok(ServerFork::Forge);
    }

    anyhow::bail!(DetectServerInfoError::UnknownServerFork);
}

pub fn detect_game_version(
    archive: &mut ZipArchive<BufReader<&File>>,
    fork: ServerFork,
) -> anyhow::Result<String> {
    match fork {
        ServerFork::Fabric => Fabric::game_version(archive),
        ServerFork::Forge => Forge::game_version(archive),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_detect_fabric_fork() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let file = File::open(jar_path).unwrap();
        let mut archive = ZipArchive::new(BufReader::new(&file)).unwrap();
        let fork = detect_server_fork(&mut archive).unwrap();

        assert_eq!(fork, ServerFork::Fabric)
    }

    #[test]
    fn test_detect_game_version() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let file = File::open(jar_path).unwrap();
        let mut archive = ZipArchive::new(BufReader::new(&file)).unwrap();
        let version = detect_game_version(&mut archive, ServerFork::Fabric).unwrap();

        assert_eq!(version, "1.21.8")
    }
}
