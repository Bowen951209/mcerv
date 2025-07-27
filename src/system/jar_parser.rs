use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use zip::{ZipArchive, result::ZipResult};

use crate::system::config::ServerFork;
use anyhow::anyhow;

#[derive(Debug, Clone)]
pub enum DetectServerForkError {
    MainClassNotFound,
    UnknownServerFork,
}

impl Error for DetectServerForkError {}

impl Display for DetectServerForkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectServerForkError::MainClassNotFound => {
                write!(f, "Main-Class not found in JAR MANIFEST.MF")
            }
            DetectServerForkError::UnknownServerFork => {
                write!(
                    f,
                    "Detected an unknown server fork. Probably not supported by multi-server"
                )
            }
        }
    }
}

pub fn detect_server_fork(jar_path: impl AsRef<Path>) -> anyhow::Result<ServerFork> {
    let manifest = read_manifest_from_jar(jar_path)?;

    // !!! Currenty support fabric only.
    if let Some(main_class) = manifest.get("Main-Class") {
        if main_class.contains("net.fabricmc") {
            return Ok(ServerFork::Fabric);
        }

        return Err(anyhow!(DetectServerForkError::UnknownServerFork));
    }

    Err(anyhow!(DetectServerForkError::MainClassNotFound))
}

fn read_manifest_from_jar(jar_path: impl AsRef<Path>) -> ZipResult<HashMap<String, String>> {
    let file = File::open(jar_path)?;
    let mut archive = ZipArchive::new(BufReader::new(file))?;

    let mut manifest_file = archive.by_name("META-INF/MANIFEST.MF")?;
    let mut content = String::new();
    manifest_file.read_to_string(&mut content)?;

    Ok(parse_manifest(&content))
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_detect_fabric_fork() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let fork = detect_server_fork(jar_path).unwrap();

        assert_eq!(fork, ServerFork::Fabric)
    }
}
