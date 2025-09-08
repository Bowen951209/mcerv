use crate::{system::jar_parser::single_jar, try_server_dir};
use serde::{Deserialize, Serialize};
use shlex::QuoteError;
use std::{
    error::Error,
    fmt::{Debug, Display},
    fs::{self, File},
    path::Path,
};

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

#[derive(Serialize, Deserialize)]
pub struct StartCommand(String);

impl StartCommand {
    pub fn jar_name(&self) -> String {
        // Find the .jar in start_command and return it
        self.split()
            .iter()
            .find(|t| t.ends_with(".jar"))
            .unwrap()
            .clone()
    }

    pub fn max_memory(&self) -> String {
        // Find the Xmx in start_command and return it
        self.split()
            .iter()
            .find(|t| t.contains("-Xmx"))
            .unwrap()
            .replace("-Xmx", "")
    }

    pub fn min_memory(&self) -> String {
        // Find the Xms in start_command and return it
        self.split()
            .iter()
            .find(|t| t.contains("-Xms"))
            .unwrap()
            .replace("-Xms", "")
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

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub start_command: StartCommand,
    pub java_home: Option<String>,
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
        };

        config.set_jar(jar_path)?;

        Ok(config)
    }

    /// Load the config from the server directory.
    /// If the config file does not exist, create a new one with default values.
    pub fn load_or_create(server_name: &str) -> anyhow::Result<Config> {
        let server_dir = try_server_dir(server_name)?;
        let path = server_dir.join("mcerv_config.json");

        if !path.exists() {
            println!("mcerv config file does not exist, creating a new one with default values...");
            let jar = single_jar(&server_dir)?;
            return Config::new(jar);
        }

        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;
        config.check_validity()?;

        Ok(config)
    }

    pub fn save(&self, server_name: &str) -> anyhow::Result<()> {
        let path = try_server_dir(server_name)?.join("mcerv_config.json");
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

    /// Check the validity of the `mcerv` config
    pub fn check_validity(&mut self) -> anyhow::Result<()> {
        // Check start command
        self.start_command.check_valid()?;

        Ok(())
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Max Memory: {}", self.start_command.max_memory())?;
        writeln!(f, "Min Memory: {}", self.start_command.min_memory())?;
        writeln!(f, "Executable Jar: {}", self.start_command.jar_name())?;
        writeln!(
            f,
            "Java Home: {}",
            self.java_home.as_deref().unwrap_or("Not Set")
        )?;
        Ok(())
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

    #[test]
    fn test_create_start_script() {
        // Test with JAVA_HOME set
        let config = Config {
            start_command: StartCommand("java -Xmx2G -Xms1G -jar server.jar nogui".to_string()),
            java_home: Some("/path/to/java".to_string()),
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
