mod command;
mod fabric_meta;
use rustyline::error::ReadlineError;
use serde::Deserialize;
use shlex::QuoteError;
use std::{fs, path::Path};

#[derive(Deserialize)]
pub struct Config {
    pub server: ServerConfig,
}

#[derive(Deserialize)]
pub struct ServerConfig {
    pub start_command: String,
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidJarNumber,
    InvalidXmxNumber,
    InvalidXmsNumber,
}

impl ServerConfig {
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

pub async fn run() -> anyhow::Result<()> {
    let mut editor = command::get_editor()?;
    loop {
        let readline = editor.readline(">> ");
        match readline {
            Ok(line) => {
                editor.add_history_entry(line.trim())?;
                command::execute(line.trim()).await;
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

    #[test]
    fn test_valid_config1() {
        let path = Path::new("tests/data/config1.json");
        let config = load_config(path).unwrap();
        assert_eq!(
            config.server.start_command,
            "java -Xmx2G -Xms1G -jar \"server.jar\" nogui"
        );
    }
}
