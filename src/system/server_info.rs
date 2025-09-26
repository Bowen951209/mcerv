use std::{
    fmt::{Debug, Display},
    fs::File,
    io::BufReader,
    path::Path,
};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::system::forks;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, ValueEnum)]
pub enum ServerFork {
    Fabric,
    Forge,
}

#[derive(Debug)]
pub struct ServerInfo {
    pub server_fork: ServerFork,
    pub game_version: String,
}

impl ServerInfo {
    pub fn new(jar_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let jar_file = File::open(&jar_path)?;
        let mut archive = ZipArchive::new(BufReader::new(&jar_file))?;

        let server_fork = forks::detect_server_fork(&mut archive)?;
        let game_version = forks::detect_game_version(&mut archive, server_fork)?;

        Ok(Self {
            server_fork,
            game_version,
        })
    }
}

impl Display for ServerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Server Fork: {:?}", self.server_fork)?;
        writeln!(f, "Minecraft Version: {}", self.game_version)?;
        Ok(())
    }
}
