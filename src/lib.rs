mod command;
mod fabric_meta;
mod state;
use rustyline::error::ReadlineError;
use serde::{Deserialize, Serialize};
use shlex::QuoteError;
use std::{
    collections::HashMap,
    fs::{self, File},
    path::Path,
};

use crate::{command::CommandManager, state::State};

#[derive(Debug)]
pub enum ConfigError {
    InvalidJarNumber,
    InvalidXmxNumber,
    InvalidXmsNumber,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    servers: HashMap<String, ServerConfig>,
}

impl Config {
    pub fn save(&self) -> anyhow::Result<()> {
        let file = File::create("config.json")?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }

    pub fn get_servers(&self) -> &HashMap<String, ServerConfig> {
        &self.servers
    }

    pub fn get_servers_mut(&mut self) -> &mut HashMap<String, ServerConfig> {
        &mut self.servers
    }
}

#[derive(Serialize, Deserialize)]
pub struct ServerConfig {
    pub start_command: String,
}

impl ServerConfig {
    /// Create a new ServerConfig with max and min memory set to 4G.
    pub fn new(filename: &str) -> Self {
        ServerConfig {
            start_command: format!("java -Xmx4G -Xms4G -jar {filename} nogui"),
        }
    }

    pub fn check_start_command(&self) -> Result<(), ConfigError> {
        let tokens = shlex::split(&self.start_command).unwrap();

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

    pub fn set_jar(&mut self, path: &str) -> Result<(), QuoteError> {
        // Find the .jar in start_command and replace it
        let mut tokens = shlex::split(&self.start_command).unwrap();
        let found_jar = tokens.iter_mut().find(|t| t.contains(".jar")).unwrap();
        *found_jar = path.to_string();

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.start_command = shlex::try_join(str_tokens)?;

        Ok(())
    }

    pub fn set_max_memory(&mut self, max_memory: &str) -> Result<(), QuoteError> {
        // Find the Xmx in start_command and replace it
        let mut tokens = shlex::split(&self.start_command).unwrap();
        let found_xmx = tokens.iter_mut().find(|t| t.contains("-Xmx")).unwrap();
        *found_xmx = format!("-Xmx{}", max_memory);

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.start_command = shlex::try_join(str_tokens)?;

        Ok(())
    }

    pub fn set_min_memory(&mut self, min_memory: &str) -> Result<(), QuoteError> {
        // Find the Xms in start_command and replace it
        let mut tokens = shlex::split(&self.start_command).unwrap();
        let found_xms = tokens.iter_mut().find(|t| t.contains("-Xms")).unwrap();
        *found_xms = format!("-Xms{}", min_memory);

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.start_command = shlex::try_join(str_tokens)?;

        Ok(())
    }
}

pub fn load_config(path: &Path) -> anyhow::Result<Config> {
    let config_content = fs::read_to_string(path)?;
    let config: Config = serde_json::from_str(&config_content)?;

    Ok(config)
}

pub fn run() -> anyhow::Result<()> {
    let mut editor = command::create_editor()?;
    let cmd_manager = CommandManager::new();

    // load default config
    let config_path = Path::new("config.json");
    let config = load_config(config_path).expect("Failed to load config");
    // Check if start command is valid (might have been changed by the user manually)
    for (_, server_config) in &config.servers {
        server_config
            .check_start_command()
            .expect("Invalid start command");
    }
    let mut state = State::new(config);
    println!("Loaded config: {:?}", config_path);

    loop {
        let readline = editor.readline(">> ");
        match readline {
            Ok(line) => {
                editor.add_history_entry(line.trim())?;
                cmd_manager
                    .execute(line.trim(), &mut state)
                    .unwrap_or_else(|e| eprintln!("Error executing command: {}", e));
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_jar() {
        let mut config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar \"some-server.jar\" nogui"),
        };

        config
            .set_jar("a server with spaces in the name.jar")
            .unwrap();

        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms1G -jar 'a server with spaces in the name.jar' nogui")
        );

        config.set_jar("nospaces.jar").unwrap();
        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms1G -jar nospaces.jar nogui")
        );
    }

    #[test]
    fn test_set_max_memory() {
        let mut config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui"),
        };

        config.set_max_memory("3G").unwrap();

        assert_eq!(
            config.start_command,
            String::from("java -Xmx3G -Xms1G -jar some-server.jar nogui")
        );
    }

    #[test]
    fn test_set_min_memory() {
        let mut config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui"),
        };

        config.set_min_memory("3G").unwrap();

        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms3G -jar some-server.jar nogui")
        );
    }

    #[test]
    fn test_invalid_start_commands() {
        // invalid jar number
        let config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar two.jar nogui"),
        };
        assert!(config.check_start_command().is_err());

        let config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar nogui"),
        };
        assert!(config.check_start_command().is_err());

        // invalid Xmx number
        let config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui -Xmx1G"),
        };
        assert!(config.check_start_command().is_err());

        let config = ServerConfig {
            start_command: String::from("java -Xms1G -jar some-server.jar nogui"),
        };
        assert!(config.check_start_command().is_err());

        // invalid Xms number
        let config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui -Xms1G"),
        };
        assert!(config.check_start_command().is_err());

        let config = ServerConfig {
            start_command: String::from("java -Xmx2G -jar some-server.jar nogui"),
        };
        assert!(config.check_start_command().is_err());
    }
}
