use crate::{server_dir, system::jar_parser, try_server_dir};
use serde::{Deserialize, Serialize};
use shlex::QuoteError;
use std::{
    error::Error,
    fmt::Display,
    fs::{self, File},
    io::{self, BufReader},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

#[derive(Debug)]
pub enum InvalidStartCommandError {
    JarOccurrence,
    XmxOccurrence,
    XmsOccurrence,
    Split,
}

impl Display for InvalidStartCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidStartCommandError::JarOccurrence => {
                write!(f, "start_command must contain exactly one .jar file")
            }
            InvalidStartCommandError::XmxOccurrence => {
                write!(f, "start_command must contain exactly one -Xmx option")
            }
            InvalidStartCommandError::XmsOccurrence => {
                write!(f, "start_command must contain exactly one -Xms option")
            }
            InvalidStartCommandError::Split => {
                write!(f, "failed to split start_command with shlex")
            }
        }
    }
}

impl Error for InvalidStartCommandError {}

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

#[derive(Serialize, Deserialize)]
pub struct StartCommand(String);

impl StartCommand {
    pub fn get_jar_name(&self) -> String {
        // Find the .jar in start_command and return it
        self.split()
            .iter()
            .find(|t| t.ends_with(".jar"))
            .unwrap()
            .clone()
    }

    pub fn set_max_memory(&mut self, max_memory: &str) -> Result<(), QuoteError> {
        // Find the Xmx in start_command and replace it
        let mut tokens = self.split();
        let found_xmx = tokens.iter_mut().find(|t| t.contains("-Xmx")).unwrap();
        *found_xmx = format!("-Xmx{max_memory}");

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.0 = shlex::try_join(str_tokens)?;

        Ok(())
    }

    pub fn set_min_memory(&mut self, min_memory: &str) -> Result<(), QuoteError> {
        // Find the Xms in start_command and replace it
        let mut tokens = self.split();
        let found_xms = tokens.iter_mut().find(|t| t.contains("-Xms")).unwrap();
        *found_xms = format!("-Xms{min_memory}");

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.0 = shlex::try_join(str_tokens)?;

        Ok(())
    }

    fn set_jar(&mut self, jar_name: String) -> Result<(), QuoteError> {
        // Find the .jar in start_command and replace it
        let mut tokens = self.split();
        let found_jar = tokens.iter_mut().find(|t| t.ends_with(".jar")).unwrap();
        *found_jar = jar_name;

        let str_tokens = tokens.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        self.0 = shlex::try_join(str_tokens)?;

        Ok(())
    }

    fn split(&self) -> Vec<String> {
        shlex::split(&self.0).unwrap()
    }

    fn check_valid(&self) -> Result<(), InvalidStartCommandError> {
        let tokens = shlex::split(&self.0).ok_or(InvalidStartCommandError::Split)?;

        if tokens.iter().filter(|t| t.ends_with(".jar")).count() != 1 {
            return Err(InvalidStartCommandError::JarOccurrence);
        }

        if tokens.iter().filter(|t| t.contains("-Xmx")).count() != 1 {
            return Err(InvalidStartCommandError::XmxOccurrence);
        }

        if tokens.iter().filter(|t| t.contains("-Xms")).count() != 1 {
            return Err(InvalidStartCommandError::XmsOccurrence);
        }

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
        let start_command = StartCommand("java -Xmx4G -Xms4G -jar replaceme.jar nogui".to_string());
        let instance = Self::new_with_start_command(start_command, jar_path)?;

        Ok(instance)
    }

    pub fn new_with_start_command(
        start_command: StartCommand,
        jar_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let mut config = Self {
            start_command,
            java_home: None,
            server_fork: ServerFork::Fabric,
            game_version: String::new(),
            server_jar_hash: String::new(),
        };

        config.set_jar(jar_path)?;

        Ok(config)
    }

    /// Load the config from the server directory.
    /// If the config file does not exist, create a new one with default values.
    pub fn load_or_create(server_name: &str) -> anyhow::Result<Config> {
        let server_dir = try_server_dir(server_name)?;
        let path = server_dir.join("multi_server_config.json");

        if !path.exists() {
            println!("mcerv config file does not exist, creating a new one with default values...");
            let jar = single_jar(&server_dir)?;
            return Config::new(jar);
        }

        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;
        config.check_validity_and_update(server_name)?;

        Ok(config)
    }

    pub fn save(&self, server_name: &str) -> anyhow::Result<()> {
        let path = try_server_dir(server_name)?.join("multi_server_config.json");
        let file = File::create(&path)?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }

    pub fn set_jar(&mut self, jar_path: impl AsRef<Path>) -> anyhow::Result<()> {
        let filename = jar_path
            .as_ref()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        self.start_command.set_jar(filename)?;

        let mut file = File::open(jar_path)?;
        let hash = jar_parser::calculate_hash(&mut file)?;
        let mut archive = ZipArchive::new(BufReader::new(&file))?;

        self.server_jar_hash = hash;
        self.server_fork = jar_parser::detect_server_fork(&mut archive)?;
        self.game_version = jar_parser::detect_game_version(&mut archive)?;

        Ok(())
    }

    pub fn create_start_script(&self) -> Result<String, InvalidStartCommandError> {
        let script = if cfg!(target_os = "windows") {
            // Windows batch script
            let java_home_script = match &self.java_home {
                Some(java_home) => format!(
                    "\
set JAVA_HOME={java_home}
set PATH=%JAVA_HOME%\\bin;%PATH%"
                ),
                None => String::new(),
            };

            format!(
                "\
@echo off
{java_home_script}

echo Using Java: %JAVA_HOME%
java --version
{start_command}",
                java_home_script = java_home_script,
                start_command = self.start_command.0
            )
        } else {
            // Unix shell script
            let java_home_script = match &self.java_home {
                Some(java_home) => format!(
                    "\
export JAVA_HOME=\"{java_home}\"
export PATH=\"$JAVA_HOME/bin:$PATH\""
                ),
                None => String::new(),
            };

            format!(
                "\
#!/usr/bin/env bash
{java_home_script}

echo Using Java: $JAVA_HOME
java --version
{start_command}",
                java_home_script = java_home_script,
                start_command = self.start_command.0
            )
        };

        Ok(script)
    }

    /// Check
    /// * the validity of the `mcerv` config
    /// * the server jar file (by comparing the hash). If changed, update the properties in the config.
    pub fn check_validity_and_update(&mut self, server_name: &str) -> anyhow::Result<()> {
        // Check start command
        self.start_command.check_valid()?;

        // Check if user maually changed the server jar file
        let server_dir = server_dir(server_name);
        let current_hash = single_jar_hash(&server_dir)?;
        let old_hash = &self.server_jar_hash;

        if current_hash != *old_hash {
            println!("Server jar file has changed.");
            // Update the config with the new jar file information
            let current_jar = single_jar(&server_dir)?;
            self.set_jar(current_jar)?;
            self.save(server_name)?;
        }

        Ok(())
    }
}

/// Returns the hash of the first `.jar` file found in the server directory.
///
/// See also [`single_jar`].
fn single_jar_hash(server_dir: impl AsRef<Path>) -> anyhow::Result<String> {
    let jar_path = single_jar(server_dir)?;
    let mut jar_file = File::open(jar_path)?;
    let jar_hash = jar_parser::calculate_hash(&mut jar_file)?;

    Ok(jar_hash)
}

/// Returns the first `.jar` file found in the server directory.
///
/// # Errors
/// * If there are multiple `.jar` files, returns [`InvalidServerDirError::MultipleJars`].
/// * If no `.jar` file is found, returns [`InvalidServerDirError::NoJar`].
/// * If trouble reading the directory, returns the underlying [`io::Error`].
fn single_jar(server_dir: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let mut jars = jar_files(server_dir)?.into_iter();
    let jar = jars.next();

    if jars.next().is_some() {
        anyhow::bail!(InvalidServerDirError::MultipleJars);
    }

    jar.ok_or(anyhow::anyhow!(InvalidServerDirError::NoJar))
}

fn jar_files(server_dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
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

    #[test]
    fn test_create_start_script() {
        // Test with JAVA_HOME set
        let config = Config {
            start_command: StartCommand("java -Xmx2G -Xms1G -jar server.jar nogui".to_string()),
            java_home: Some("/path/to/java".to_string()),
            server_fork: ServerFork::Fabric,
            game_version: "1.21.1".to_string(),
            server_jar_hash: "test_hash".to_string(),
        };

        let script = config.create_start_script().unwrap();
        
        if cfg!(target_os = "windows") {
            assert!(script.contains("@echo off"));
            assert!(script.contains("set JAVA_HOME=/path/to/java"));
            assert!(script.contains("set PATH=%JAVA_HOME%\\bin;%PATH%"));
            assert!(script.contains("java -Xmx2G -Xms1G -jar server.jar nogui"));
        } else {
            assert!(script.contains("#!/usr/bin/env bash"));
            assert!(script.contains("export JAVA_HOME=\"/path/to/java\""));
            assert!(script.contains("export PATH=\"$JAVA_HOME/bin:$PATH\""));
            assert!(script.contains("echo Using Java: $JAVA_HOME"));
            assert!(script.contains("java -Xmx2G -Xms1G -jar server.jar nogui"));
        }

        // Test without JAVA_HOME
        let config_no_java = Config {
            start_command: StartCommand("java -Xmx2G -Xms1G -jar server.jar nogui".to_string()),
            java_home: None,
            server_fork: ServerFork::Fabric,
            game_version: "1.21.1".to_string(),
            server_jar_hash: "test_hash".to_string(),
        };

        let script_no_java = config_no_java.create_start_script().unwrap();
        
        if cfg!(target_os = "windows") {
            assert!(script_no_java.contains("@echo off"));
            assert!(!script_no_java.contains("set JAVA_HOME="));
        } else {
            assert!(script_no_java.contains("#!/usr/bin/env bash"));
            assert!(!script_no_java.contains("export JAVA_HOME="));
        }
    }
}
