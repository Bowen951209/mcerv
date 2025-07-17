use serde::{Deserialize, Serialize};
use shlex::QuoteError;
use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs::{self, File},
    io,
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

#[derive(Serialize, Deserialize)]
pub struct Config {
    servers: HashMap<String, ServerConfig>,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Config> {
        let config_content = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&config_content)?;

        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let file = File::create("config.json")?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }

    pub fn add_new_folders_to_config(&mut self) -> io::Result<()> {
        let paths = fs::read_dir("instances")?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();

        for path in paths {
            if path.is_file() {
                eprintln!(
                    "Unexpected file in `instances` directory: {}",
                    path.display()
                );
                continue;
            }

            let dir_name = path.file_name().unwrap().to_str().unwrap().to_owned();

            if self.servers.contains_key(&dir_name) {
                continue;
            }

            let jar_files = fs::read_dir(&path)?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |ext| ext == "jar"))
                .collect::<Vec<_>>();

            if jar_files.len() != 1 {
                eprintln!(
                    "Directory `{}` has {} .jar files. It will not be automatically added to the config.",
                    path.display(),
                    jar_files.len()
                );
                continue;
            }

            let jar_name = jar_files[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();

            self.servers
                .insert(dir_name.clone(), ServerConfig::new(&jar_name));
            println!("Automatically added `{}` to the config.", dir_name);
        }

        Ok(())
    }

    pub fn retain_valid(&mut self) -> io::Result<()> {
        let server_dirs = fs::read_dir("instances")?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        self.servers
            .retain(|server_name, _| server_dirs.contains(server_name));

        Ok(())
    }

    pub fn check_validity(&self) -> Result<(), ConfigError> {
        for (_, server_config) in &self.servers {
            server_config.check_start_command()?;
        }
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
    start_command: String,
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

    #[allow(dead_code)] // TODO: use it when updating server or something
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

    pub fn get_start_command(&self) -> &str {
        &self.start_command
    }
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
