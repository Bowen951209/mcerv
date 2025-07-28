use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs::File,
    io::{BufReader, Read},
};

use zip::ZipArchive;

use crate::system::config::ServerFork;
use anyhow::anyhow;

#[derive(Debug, Clone)]
pub enum DetectServerInfoError {
    MainClassNotFound,
    UnknownServerFork,
    GameVersionNotFound,
}

impl Error for DetectServerInfoError {}

impl Display for DetectServerInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectServerInfoError::MainClassNotFound => {
                write!(f, "Main-Class not found in MANIFEST.MF")
            }
            DetectServerInfoError::UnknownServerFork => {
                write!(
                    f,
                    "Detected an unknown server fork. Probably not supported by multi-server"
                )
            }
            DetectServerInfoError::GameVersionNotFound => {
                write!(f, "Game version not found in install.properties")
            }
        }
    }
}

pub fn detect_server_fork(archive: &mut ZipArchive<BufReader<File>>) -> anyhow::Result<ServerFork> {
    let manifest = parse_manifest(&read_file(archive, "META-INF/MANIFEST.MF")?);

    // !!! Currently support fabric only.
    if let Some(main_class) = manifest.get("Main-Class") {
        if main_class.contains("net.fabricmc") {
            return Ok(ServerFork::Fabric);
        }

        return Err(anyhow!(DetectServerInfoError::UnknownServerFork));
    }

    Err(anyhow!(DetectServerInfoError::MainClassNotFound))
}

pub fn detect_game_version(archive: &mut ZipArchive<BufReader<File>>) -> anyhow::Result<String> {
    // Game version property is stored in `install.properties`.
    let install_properties = parse_properties(&read_file(archive, "install.properties")?);

    if let Some(version) = install_properties.get("game-version") {
        return Ok(version.clone());
    }

    Err(anyhow!(DetectServerInfoError::GameVersionNotFound))
}

pub fn detect_mod_id(archive: &mut ZipArchive<BufReader<File>>) -> anyhow::Result<String> {
    // Mod ID is stored in `fabric.mod.json`.
    let fabric_mod_json = read_file(archive, "fabric.mod.json")?;
    let fabric_mod: serde_json::Value = serde_json::from_str(&fabric_mod_json)?;

    let id = fabric_mod["id"].as_str().unwrap();

    Ok(id.to_string())
}

fn read_file(archive: &mut ZipArchive<BufReader<File>>, file_name: &str) -> anyhow::Result<String> {
    let mut file_in_jar = archive
        .by_name(file_name)
        .map_err(|_| anyhow!("{} not found in JAR", file_name))?;

    let mut content = String::new();
    file_in_jar.read_to_string(&mut content)?;

    Ok(content)
}

fn parse_manifest(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Manifest format is `key: value`
    for line in content.lines() {
        if let Some((key, value)) = line.split_once(": ") {
            map.insert(key.to_string(), value.to_string());
        }
    }

    map
}

fn parse_properties(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Properties format is `key=value`
    for line in content.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_detect_fabric_fork() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let file = File::open(jar_path).unwrap();
        let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();
        let fork = detect_server_fork(&mut archive).unwrap();

        assert_eq!(fork, ServerFork::Fabric)
    }

    #[test]
    fn test_detect_game_version() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let file = File::open(jar_path).unwrap();
        let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();
        let version = detect_game_version(&mut archive).unwrap();

        assert_eq!(version, "1.21.8")
    }
}
