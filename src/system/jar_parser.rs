use anyhow::anyhow;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs::{self, File},
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

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

// Calculate the SHA1 hash of the file contents.
pub fn calculate_hash(file: &mut File) -> std::io::Result<String> {
    let mut hasher = Sha1::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    hasher.update(&buffer);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn read_file(
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

pub fn parse_properties(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Properties format is `key=value`
    for line in content.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        }
    }

    map
}

pub fn parse_manifest(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Manifest format is `key: value`
    for line in content.lines() {
        if let Some((key, value)) = line.split_once(": ") {
            map.insert(key.to_string(), value.to_string());
        }
    }

    map
}
