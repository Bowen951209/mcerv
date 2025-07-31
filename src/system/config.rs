use serde::{Deserialize, Serialize};
use shlex::QuoteError;
use std::{
    error::Error,
    fmt::Display,
    fs::{self, File},
    io::BufReader,
    path::Path,
};
use zip::ZipArchive;

use crate::system::jar_parser;

#[derive(Debug)]
pub enum ConfigError {
    InvalidJarNumber,
    InvalidXmxNumber,
    InvalidXmsNumber,
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for ConfigError {}

#[derive(Serialize, Deserialize)]
pub struct StartCommand(String);

impl StartCommand {
    pub fn split(&self) -> Vec<String> {
        shlex::split(&self.0).expect("Command is erroneous")
    }

    fn check_valid(&self) -> Result<(), ConfigError> {
        let tokens = self.split();

        if tokens.iter().filter(|t| t.contains(".jar")).count() != 1 {
            return Err(ConfigError::InvalidJarNumber);
        }

        if tokens.iter().filter(|t| t.contains("-Xmx")).count() != 1 {
            return Err(ConfigError::InvalidXmxNumber);
        }

        if tokens.iter().filter(|t| t.contains("-Xms")).count() != 1 {
            return Err(ConfigError::InvalidXmsNumber);
        }

        Ok(())
    }

    pub fn get_jar_name(&self) -> String {
        // Find the .jar in start_command and return it
        let tokens = self.split();
        tokens.iter().find(|t| t.contains(".jar")).unwrap().clone()
    }

    pub fn set_jar(&mut self, jar_name: String) -> Result<(), QuoteError> {
        // Find the .jar in start_command and replace it
        let mut tokens = self.split();
        let found_jar = tokens.iter_mut().find(|t| t.contains(".jar")).unwrap();
        *found_jar = jar_name;

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.0 = shlex::try_join(str_tokens)?;

        Ok(())
    }

    pub fn set_max_memory(&mut self, max_memory: &str) -> Result<(), QuoteError> {
        // Find the Xmx in start_command and replace it
        let mut tokens = self.split();
        let found_xmx = tokens.iter_mut().find(|t| t.contains("-Xmx")).unwrap();
        *found_xmx = format!("-Xmx{}", max_memory);

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.0 = shlex::try_join(str_tokens)?;

        Ok(())
    }

    pub fn set_min_memory(&mut self, min_memory: &str) -> Result<(), QuoteError> {
        // Find the Xms in start_command and replace it
        let mut tokens = self.split();
        let found_xms = tokens.iter_mut().find(|t| t.contains("-Xms")).unwrap();
        *found_xms = format!("-Xms{}", min_memory);

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.0 = shlex::try_join(str_tokens)?;

        Ok(())
    }
}

impl From<String> for StartCommand {
    fn from(command: String) -> Self {
        StartCommand(command)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ServerFork {
    Fabric, // will support more in the future
}

pub enum StartScript {
    Windows(String),
    Unix(String),
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub start_command: StartCommand,
    pub java_home: Option<String>,
    pub server_fork: ServerFork,
    pub game_version: String,
    pub server_jar_hash: String,
}

impl Config {
    /// Create a new Config with max and min memory set to 4G.
    pub fn new(jar_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let jar_path = jar_path.as_ref();

        let jar_name = jar_path
            .file_name()
            .unwrap()
            .to_os_string()
            .into_string()
            .unwrap();

        let start_command = StartCommand(format!("java -Xmx4G -Xms4G -jar {jar_name} nogui"));

        let instance = Self::new_with_start_command(start_command, jar_path)?;

        Ok(instance)
    }

    pub fn new_with_start_command(
        start_command: StartCommand,
        jar_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let jar_path = jar_path.as_ref();
        let mut jar_file = File::open(jar_path)?;

        let server_jar_hash = jar_parser::calculate_hash(&mut jar_file)?;

        let mut archive = ZipArchive::new(BufReader::new(jar_file))?;

        let server_fork = jar_parser::detect_server_fork(&mut archive)?;
        let game_version = jar_parser::detect_game_version(&mut archive)?;

        Ok(Self {
            start_command,
            java_home: None,
            server_fork,
            game_version,
            server_jar_hash,
        })
    }

    pub fn load(path: &impl AsRef<Path>) -> anyhow::Result<Config> {
        let config_content = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&config_content)?;
        config.check_validity()?;

        Ok(config)
    }

    pub fn save(&self, server_name: &str) -> anyhow::Result<()> {
        let path = format!("instances/{server_name}/multi_server_config.json");
        let file = File::create(&path)?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }

    pub fn create_start_script(&self) -> Result<StartScript, ConfigError> {
        let script = if cfg!(target_os = "windows") {
            // Windows batch script

            let java_home_script = match &self.java_home {
                Some(java_home) => format!(
                    r#"set JAVA_HOME={}
set PATH=%JAVA_HOME%\bin;%PATH%"#,
                    java_home
                ),
                None => String::new(),
            };

            StartScript::Windows(format!(
                r#"@echo off
{}

echo Using Java: %JAVA_HOME%
java --version
{}"#,
                java_home_script, self.start_command.0
            ))
        } else {
            // Unix shell script
            let java_home_script = match &self.java_home {
                Some(java_home) => format!(
                    r#"export JAVA_HOME="{}"
export PATH="$JAVA_HOME/bin:$PATH""#,
                    java_home
                ),
                None => String::new(),
            };

            StartScript::Unix(format!(
                r#"#!/usr/bin/env bash
{}

echo Using Java: %JAVA_HOME%
java --version
{}"#,
                java_home_script, self.start_command.0
            ))
        };

        Ok(script)
    }

    pub fn check_validity(&self) -> Result<(), ConfigError> {
        self.start_command.check_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_jar() {
        let mut start_command: StartCommand = "java -Xmx2G -Xms1G -jar \"some-server.jar\" nogui"
            .to_string()
            .into();

        start_command
            .set_jar("a server with spaces in the name.jar".to_string())
            .unwrap();

        assert_eq!(
            start_command.0,
            "java -Xmx2G -Xms1G -jar 'a server with spaces in the name.jar' nogui".to_string()
        );

        start_command.set_jar("nospaces.jar".to_string()).unwrap();
        assert_eq!(
            start_command.0,
            "java -Xmx2G -Xms1G -jar nospaces.jar nogui".to_string()
        );
    }

    #[test]
    fn test_set_max_memory() {
        let mut start_command: StartCommand = "java -Xmx2G -Xms1G -jar some-server.jar nogui"
            .to_string()
            .into();

        start_command.set_max_memory("3G").unwrap();

        assert_eq!(
            start_command.0,
            "java -Xmx3G -Xms1G -jar some-server.jar nogui".to_string()
        );
    }

    #[test]
    fn test_set_min_memory() {
        let mut start_command: StartCommand = "java -Xmx2G -Xms1G -jar some-server.jar nogui"
            .to_string()
            .into();

        start_command.set_min_memory("3G").unwrap();

        assert_eq!(
            start_command.0,
            String::from("java -Xmx2G -Xms3G -jar some-server.jar nogui")
        );
    }

    #[test]
    fn test_invalid_start_commands() {
        // invalid jar number
        let start_command =
            StartCommand::from("java -Xmx2G -Xms1G -jar some-server.jar two.jar nogui".to_string());
        assert!(start_command.check_valid().is_err());

        let start_command = StartCommand::from("java -Xmx2G -Xms1G -jar nogui".to_string());
        assert!(start_command.check_valid().is_err());

        // invalid Xmx number
        let start_command =
            StartCommand::from("java -Xmx2G -Xms1G -jar some-server.jar nogui -Xmx1G".to_string());
        assert!(start_command.check_valid().is_err());

        let start_command =
            StartCommand::from("java -Xms1G -jar some-server.jar nogui".to_string());
        assert!(start_command.check_valid().is_err());

        // invalid Xms number
        let start_command =
            StartCommand::from("java -Xmx2G -Xms1G -jar some-server.jar nogui -Xms1G".to_string());
        assert!(start_command.check_valid().is_err());

        let start_command =
            StartCommand::from("java -Xmx2G -jar some-server.jar nogui".to_string());
        assert!(start_command.check_valid().is_err());
    }
}
