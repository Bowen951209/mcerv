use std::{error::Error, fmt::Display, fs, io::BufReader};

use zip::ZipArchive;

use crate::{
    command::{CommandManager, SubCommand},
    system::{config::Config, jar_parser},
};

#[derive(Debug, Clone, Copy)]
pub enum SelectServerError {
    ServerNotFound,
}

impl Display for SelectServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for SelectServerError {}

pub struct Server {
    pub name: String,
    pub config: Config,
}

#[derive(Default)]
pub struct State {
    pub selected_server: Option<Server>,
    pub server_names: Vec<String>,
}

impl State {
    pub fn select_server(&mut self, server_name: String) -> anyhow::Result<()> {
        if !self.server_names.contains(&server_name) {
            anyhow::bail!(SelectServerError::ServerNotFound);
        }

        let instance_dir = format!("instances/{server_name}");

        let old_config = Config::load(&format!("{instance_dir}/multi_server_config.json"))?;

        // Check if user maually changed the server jar file.
        // If so, update the config.

        // Find .jar files in instances/server_name
        let mut jar_files_iter =
            fs::read_dir(&instance_dir)?
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().is_file()
                        && entry
                            .path()
                            .extension()
                            .map(|ext| ext == "jar")
                            .unwrap_or(false)
                });

        let server_jar_name = if let Some(jar_entry) = jar_files_iter.next() {
            if jar_files_iter.next().is_some() {
                eprintln!("Multiple .jar files found in {instance_dir}. Using the first one.");
            }

            jar_entry.file_name().to_string_lossy().to_string()
        } else {
            anyhow::bail!("No .jar file found in {instance_dir}");
        };

        let server_jar_path = format!("{instance_dir}/{server_jar_name}");

        let mut server_jar_file = fs::File::open(&server_jar_path)?;

        let server_jar_hash = jar_parser::calculate_hash(&mut server_jar_file)?;

        let config_jar_hash = &old_config.server_jar_hash;

        if server_jar_hash != *config_jar_hash {
            println!("Detected user manually changed server jar file. Updating the config file...");

            let mut archive = ZipArchive::new(BufReader::new(server_jar_file))?;

            let new_fork = jar_parser::detect_server_fork(&mut archive)?;
            let old_fork = old_config.server_fork;

            if new_fork != old_fork {
                eprintln!(
                    "Detected server fork changed from {:?} to {:?}. This may cause issues.",
                    old_fork, new_fork
                );
            }

            // We want to preserve the information in the old start command, memory settings, for example.
            let mut start_command = old_config.start_command;
            start_command.set_jar(server_jar_name)?;

            let new_config = Config::new_with_start_command(start_command, server_jar_path);

            self.selected_server = Some(Server {
                name: server_name,
                config: new_config?,
            });
        } else {
            self.selected_server = Some(Server {
                name: server_name,
                config: old_config,
            });
        }

        Ok(())
    }

    pub fn update_server_names(&mut self, cmd_manager: &mut CommandManager) -> anyhow::Result<()> {
        let dir_names = fs::read_dir("instances")?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .expect("Failed to get directory name")
                    .to_string()
            })
            .collect();

        self.server_names = dir_names;

        // Update server names to subcommands of "select" command
        let select_command = cmd_manager
            .commands
            .iter_mut()
            .find(|cmd| cmd.name == "select")
            .unwrap();

        select_command.sub_commands = self
            .server_names
            .iter()
            .map(|name| SubCommand {
                name: name.clone(),
                sub_commands: vec![],
                help: "",
                options: vec![],
                handler: None,
            })
            .collect();

        Ok(())
    }
}
