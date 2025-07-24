use serde::{Deserialize, Serialize};
use shlex::QuoteError;
use std::{
    error::Error,
    fmt::Display,
    fs::{self, File},
    path::Path,
};

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

pub enum StartScript {
    Windows(String),
    Unix(String),
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub start_command: String,
    pub java_home: Option<String>,
}

impl Config {
    /// Create a new Config with max and min memory set to 4G.
    pub fn new(jar_file_name: &str) -> Self {
        Self {
            start_command: format!("java -Xmx2G -Xms1G -jar {jar_file_name} nogui"),
            java_home: None,
        }
    }

    pub fn load(path: &Path) -> anyhow::Result<Config> {
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
                java_home_script, self.start_command
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
                java_home_script, self.start_command
            ))
        };

        Ok(script)
    }

    pub fn check_validity(&self) -> Result<(), ConfigError> {
        self.check_start_command()
    }

    fn check_start_command(&self) -> Result<(), ConfigError> {
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

    pub fn get_jar_name(&self) -> String {
        // Find the .jar in start_command and return it
        let tokens = shlex::split(&self.start_command).unwrap();
        tokens.iter().find(|t| t.contains(".jar")).unwrap().clone()
    }

    pub fn set_jar(&mut self, jar_name: String) -> Result<(), QuoteError> {
        // Find the .jar in start_command and replace it
        let mut tokens = shlex::split(&self.start_command).unwrap();
        let found_jar = tokens.iter_mut().find(|t| t.contains(".jar")).unwrap();
        *found_jar = jar_name;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_jar() {
        let mut config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar \"some-server.jar\" nogui"),
            java_home: None,
        };

        config
            .set_jar("a server with spaces in the name.jar".to_string())
            .unwrap();

        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms1G -jar 'a server with spaces in the name.jar' nogui")
        );

        config.set_jar("nospaces.jar".to_string()).unwrap();
        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms1G -jar nospaces.jar nogui")
        );
    }

    #[test]
    fn test_set_max_memory() {
        let mut config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui"),
            java_home: None,
        };

        config.set_max_memory("3G").unwrap();

        assert_eq!(
            config.start_command,
            String::from("java -Xmx3G -Xms1G -jar some-server.jar nogui")
        );
    }

    #[test]
    fn test_set_min_memory() {
        let mut config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui"),
            java_home: None,
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
        let config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar two.jar nogui"),
            java_home: None,
        };
        assert!(config.check_start_command().is_err());

        let config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar nogui"),
            java_home: None,
        };
        assert!(config.check_start_command().is_err());

        // invalid Xmx number
        let config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui -Xmx1G"),
            java_home: None,
        };
        assert!(config.check_start_command().is_err());

        let config = Config {
            start_command: String::from("java -Xms1G -jar some-server.jar nogui"),
            java_home: None,
        };
        assert!(config.check_start_command().is_err());

        // invalid Xms number
        let config = Config {
            start_command: String::from("java -Xmx2G -Xms1G -jar some-server.jar nogui -Xms1G"),
            java_home: None,
        };
        assert!(config.check_start_command().is_err());

        let config = Config {
            start_command: String::from("java -Xmx2G -jar some-server.jar nogui"),
            java_home: None,
        };
        assert!(config.check_start_command().is_err());
    }
}
