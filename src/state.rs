use std::{error::Error, fmt::Display, fs, path::Path};

use crate::{
    Config,
    command::{CommandManager, SubCommand},
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

        self.selected_server = Some(Server {
            name: server_name.clone(),
            config: Config::load(Path::new(&format!(
                "instances/{server_name}/multi_server_config.json",
            )))?,
        });

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
