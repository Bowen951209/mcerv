use crate::system::{
    forks::{self, ServerFork},
    jar_parser,
};
use std::{
    fmt::{Debug, Display},
    path::Path,
};

#[derive(Debug)]
pub struct ServerInfo {
    pub server_fork: ServerFork,
    pub game_version: String,
}

impl ServerInfo {
    pub fn new(jar_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut archive = jar_parser::archive(jar_path)?;

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
