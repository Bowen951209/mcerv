use std::{
    error::Error,
    fmt::Display,
    fs,
    io::{BufReader, BufWriter},
    process::ChildStdin,
    sync::{Arc, Mutex, mpsc},
};

use rustyline::{Editor, ExternalPrinter, history::FileHistory};
use zip::ZipArchive;

use crate::{
    command::SubCommand,
    system::{
        command::{Command, CommandManager},
        config::Config,
        jar_parser,
    },
};

/// Represents the current operating context of multi-server.
/// Starts as `Default`, switches to `MinecraftServer` when a server is running,
/// and reverts to `Default` when the server process ends.
pub enum Context {
    Default,
    MinecraftServer(BufWriter<ChildStdin>),
}

#[derive(Debug, Clone, Copy)]
pub enum SelectServerError {
    ServerNotFound,
}

impl Display for SelectServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for SelectServerError {}

pub struct Server {
    pub name: String,
    pub config: Config,
}

pub struct State<EP: ExternalPrinter + Send + Sync + 'static> {
    pub editor: Editor<CommandManager<EP>, FileHistory>,
    pub selected_server: Option<Server>,
    pub server_names: Vec<String>,
    pub async_runtime: tokio::runtime::Runtime,
    pub reqwest_client: reqwest::Client,
    pub external_printer: Arc<Mutex<EP>>,
    pub context_tx: mpsc::Sender<Context>,
    pub context_rx: mpsc::Receiver<Context>,
}

impl<EP: ExternalPrinter + Send + Sync + 'static> State<EP> {
    pub fn new(
        editor: Editor<CommandManager<EP>, FileHistory>,
        external_printer: Arc<Mutex<EP>>,
    ) -> Self {
        let (context_tx, context_rx) = mpsc::channel();

        Self {
            editor,
            selected_server: None,
            server_names: vec![],
            async_runtime: tokio::runtime::Runtime::new().expect("Failed to create async runtime"),
            reqwest_client: reqwest::Client::new(),
            external_printer,
            context_tx,
            context_rx,
        }
    }

    pub fn command_manager(&self) -> &CommandManager<EP> {
        self.editor.helper().unwrap()
    }

    pub fn command_manager_mut(&mut self) -> &mut CommandManager<EP> {
        self.editor.helper_mut().unwrap()
    }

    pub fn commands(&self) -> &Vec<Command<EP>> {
        &self.command_manager().commands
    }

    pub fn commands_mut(&mut self) -> &mut Vec<Command<EP>> {
        &mut self.command_manager_mut().commands
    }

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
                    "Detected server fork changed from {old_fork:?} to {new_fork:?}. This may cause issues."
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

    /// Find all directories in the "instances" folder and put their names into `select` command's subcommands.
    /// This is for server name auto completion.
    pub fn update_server_names(&mut self) -> anyhow::Result<()> {
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
        let sub_commands = self
            .server_names
            .iter()
            .map(|name| SubCommand::<EP> {
                name: name.clone(),
                sub_commands: vec![],
                help: "",
                options: vec![],
                handler: None,
            })
            .collect::<Vec<_>>();

        let select_command = self
            .commands_mut()
            .iter_mut()
            .find(|cmd| cmd.name == "select")
            .unwrap();

        select_command.sub_commands = sub_commands;

        Ok(())
    }
}
