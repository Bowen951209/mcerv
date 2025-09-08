use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs::{self, File},
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
};

use sha1::{Digest, Sha1};
use zip::ZipArchive;

use anyhow::anyhow;

use crate::system::server_info::ServerFork;

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

#[derive(Debug)]
pub enum InvalidServerDirError {
    MultipleJars,
    NoJar,
}

impl Display for InvalidServerDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidServerDirError::MultipleJars => {
                write!(f, "multiple .jar files found in server directory")
            }
            InvalidServerDirError::NoJar => write!(f, "no .jar file found in server directory"),
        }
    }
}

impl Error for InvalidServerDirError {}

/// Returns the first `.jar` file found in the server directory.
///
/// # Errors
/// - If there are multiple `.jar` files, returns [`InvalidServerDirError::MultipleJars`].
/// - If no `.jar` file is found, returns [`InvalidServerDirError::NoJar`].
/// - If trouble reading the directory, returns the underlying [`io::Error`].
pub fn single_jar(server_dir: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let mut jars = jar_files(server_dir)?.into_iter();
    let jar = jars.next();

    if jars.next().is_some() {
        anyhow::bail!(InvalidServerDirError::MultipleJars);
    }

    jar.ok_or(anyhow::anyhow!(InvalidServerDirError::NoJar))
}

/// Returns all `.jar` files found in the server directory.
pub fn jar_files(server_dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let server_dir = server_dir.as_ref();
    let mut jars = vec![];

    for entry in fs::read_dir(server_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension() != Some(std::ffi::OsStr::new("jar")) {
            continue;
        }
        jars.push(path);
    }

    Ok(jars)
}

pub fn detect_server_fork(
    archive: &mut ZipArchive<BufReader<&File>>,
) -> anyhow::Result<ServerFork> {
    let manifest = parse_manifest(&read_file(archive, "META-INF/MANIFEST.MF")?);

    // !!! Currently support fabric only.
    if let Some(main_class) = manifest.get("Main-Class") {
        if main_class.contains("net.fabricmc") {
            return Ok(ServerFork::Fabric);
        }

        anyhow::bail!(DetectServerInfoError::UnknownServerFork);
    }

    anyhow::bail!(DetectServerInfoError::MainClassNotFound);
}

pub fn detect_game_version(archive: &mut ZipArchive<BufReader<&File>>) -> anyhow::Result<String> {
    // Game version property is stored in `install.properties`.
    let install_properties = parse_properties(&read_file(archive, "install.properties")?);

    if let Some(version) = install_properties.get("game-version") {
        return Ok(version.clone());
    }

    anyhow::bail!(DetectServerInfoError::GameVersionNotFound);
}

// Calculate the SHA1 hash of the file contents.
pub fn calculate_hash(file: &mut File) -> std::io::Result<String> {
    let mut hasher = Sha1::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    hasher.update(&buffer);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_file(
    archive: &mut ZipArchive<BufReader<&File>>,
    file_name: &str,
) -> anyhow::Result<String> {
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
        let mut archive = ZipArchive::new(BufReader::new(&file)).unwrap();
        let fork = detect_server_fork(&mut archive).unwrap();

        assert_eq!(fork, ServerFork::Fabric)
    }

    #[test]
    fn test_detect_game_version() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let file = File::open(jar_path).unwrap();
        let mut archive = ZipArchive::new(BufReader::new(&file)).unwrap();
        let version = detect_game_version(&mut archive).unwrap();

        assert_eq!(version, "1.21.8")
    }
}
