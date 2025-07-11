use regex::Regex;
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize)]
pub struct Config {
    pub server: ServerConfig,
}

#[derive(Deserialize)]
pub struct ServerConfig {
    pub start_command: String,
}

impl ServerConfig {
    pub fn set_jar(&mut self, path: &str) {
        // Find the "***.jar" in start_command and replace it
        let re = Regex::new(r#""[^"]*\.jar""#).unwrap();
        self.start_command = re
            .replace(&self.start_command, &format!("\"{}\"", path))
            .to_string();
    }

    pub fn set_max_memory(&mut self, max_memory: &str) {
        // Find the "Xmx***" in start_command and replace it
        let re = Regex::new(r#"Xmx[^ ]*"#).unwrap();
        self.start_command = re
            .replace(&self.start_command, &format!("Xmx{}", max_memory))
            .to_string();
    }

    pub fn set_min_memory(&mut self, min_memory: &str) {
        // Find the "Xms***" in start_command and replace it
        let re = Regex::new(r#"Xms[^ ]*"#).unwrap();
        self.start_command = re
            .replace(&self.start_command, &format!("Xms{}", min_memory))
            .to_string();
    }
}

pub fn load_config(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let config_content = fs::read_to_string(path)?;
    let config: Config = serde_json::from_str(&config_content)?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_jar() {
        let mut config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar \"some-server.jar\" nogui"),
        };

        config.set_jar("a server with spaces in the name.jar");

        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms1G -jar \"a server with spaces in the name.jar\" nogui")
        );
    }

    #[test]
    fn test_set_max_memory() {
        let mut config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar \"some-server.jar\" nogui"),
        };

        config.set_max_memory("3G");

        assert_eq!(
            config.start_command,
            String::from("java -Xmx3G -Xms1G -jar \"some-server.jar\" nogui")
        );
    }

    #[test]
    fn test_set_min_memory() {
        let mut config = ServerConfig {
            start_command: String::from("java -Xmx2G -Xms1G -jar \"some-server.jar\" nogui"),
        };

        config.set_min_memory("3G");

        assert_eq!(
            config.start_command,
            String::from("java -Xmx2G -Xms3G -jar \"some-server.jar\" nogui")
        );
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
