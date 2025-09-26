use crate::{
    system::jar_parser::{InvalidServerDirError, single_jar},
    try_server_dir,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    fs::{self, File},
};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub min_memory: String,
    pub max_memory: String,
    pub jar_name: String,
    pub java_home: Option<String>,
}

impl Config {
    /// Create a new config with max and min memory set to 4GB.
    pub fn new_4gb(jar_name: String) -> anyhow::Result<Config> {
        Ok(Self {
            min_memory: "4G".to_string(),
            max_memory: "4G".to_string(),
            jar_name,
            java_home: None,
        })
    }

    /// Loads the configuration from the server directory.
    /// If the config file does not exist, creates a new one with default values.
    ///
    /// Behavior:
    /// - If exactly one jar is found in the server directory, its name is stored in the config.
    ///   This allows automatic updates if the user manually replaces the jar file.
    /// - If multiple jars are found, the config keeps the previously set jar name.
    ///   If the config is being created for the first time and multiple jars exist, an error is returned.
    pub fn load_or_create(server_name: &str) -> anyhow::Result<Config> {
        let server_dir = try_server_dir(server_name)?;
        let path = server_dir.join("mcerv_config.json");

        if !path.exists() {
            println!("mcerv config file does not exist, creating a new one with default values...");
            let jar_name = single_jar(&server_dir)?
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            return Self::new_4gb(jar_name);
        }

        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;

        // If single jar replaced, update config
        match single_jar(server_dir) {
            Ok(new_jar) => {
                let new_jar_name = new_jar.file_name().unwrap().to_string_lossy();
                let old_jar_name = &config.jar_name;
                if old_jar_name != &new_jar_name {
                    println!(
                        "Detected jar file change: {old_jar_name} -> {new_jar_name}, updating config..."
                    );
                    config.jar_name = new_jar_name.to_string();
                    config.save(server_name)?;
                }
            }
            Err(e) => {
                // Multiple jars is fine, just keep the old config
                if !matches!(
                    e.downcast_ref::<InvalidServerDirError>(),
                    Some(InvalidServerDirError::MultipleJars)
                ) {
                    return Err(e);
                }
            }
        }

        Ok(config)
    }

    pub fn save(&self, server_name: &str) -> anyhow::Result<()> {
        let path = try_server_dir(server_name)?.join("mcerv_config.json");
        let file = File::create(&path)?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }

    pub fn create_start_command(&self) -> String {
        format!(
            "java -Xmx{} -Xms{} -jar {} nogui",
            self.max_memory, self.min_memory, self.jar_name
        )
    }

    pub fn create_start_script(&self) -> String {
        if cfg!(target_os = "windows") {
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
                start_command = self.create_start_command()
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
                start_command = self.create_start_command()
            )
        }
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Max Memory: {}", self.max_memory)?;
        writeln!(f, "Min Memory: {}", self.min_memory)?;
        writeln!(f, "Executable Jar: {}", self.jar_name)?;
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
    fn test_create_start_script() {
        // Test with JAVA_HOME set
        let config = Config {
            max_memory: "2G".to_string(),
            min_memory: "1G".to_string(),
            jar_name: "server.jar".into(),
            java_home: Some("/path/to/java".to_string()),
        };

        let script = config.create_start_script();
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
            max_memory: "2G".to_string(),
            min_memory: "1G".to_string(),
            jar_name: "server.jar".into(),
            java_home: None,
        };

        let script_no_java = config_no_java.create_start_script();
        if cfg!(target_os = "windows") {
            assert!(script_no_java.contains("@echo off"));
            assert!(!script_no_java.contains("set JAVA_HOME="));
        } else {
            assert!(script_no_java.contains("#!/usr/bin/env bash"));
            assert!(!script_no_java.contains("export JAVA_HOME="));
        }
    }
}
