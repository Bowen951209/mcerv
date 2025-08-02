use std::{
    env,
    error::Error,
    fmt::Display,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    process::{self, Stdio},
    time::SystemTime,
};

use rustyline::{
    Context, Editor, ExternalPrinter, Helper,
    completion::{Candidate, Completer},
    config::Configurer,
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    history::FileHistory,
    validate::Validator,
};

use crate::{
    network::{
        self,
        fabric_meta::{self, PrintVersionMode},
        modrinth,
    },
    system::{
        config::{Config, StartScript},
        jar_parser,
        state::State,
    },
};

type Handler<P> = fn(&mut State<P>, &[String]) -> Result<(), String>;

#[derive(Copy, Clone, Debug)]
enum OptionError {
    MissingValue,
    InvalidOption,
}

impl Display for OptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionError::MissingValue => write!(f, "Missing value for option."),
            OptionError::InvalidOption => write!(f, "Invalid option."),
        }
    }
}

impl Error for OptionError {}

pub struct Command<P: ExternalPrinter + Send + Sync + 'static> {
    pub name: &'static str,
    pub sub_commands: Vec<SubCommand<P>>,
    pub options: Vec<CommandOption>,
    pub help: &'static str,
    pub handler: Option<Handler<P>>,
}

impl<P: ExternalPrinter + Send + Sync + 'static> Default for Command<P> {
    fn default() -> Self {
        Command {
            name: "",
            sub_commands: vec![],
            options: vec![],
            help: "",
            handler: None,
        }
    }
}

pub struct SubCommand<P: ExternalPrinter + Send + Sync + 'static> {
    // This is not a &'static str because we will define subcommands
    // at runtime. For example, the command `select` will have a variable
    // list of server names as subcommands for auto completion.
    pub name: String,
    pub sub_commands: Vec<SubCommand<P>>,
    pub options: Vec<CommandOption>,
    pub help: &'static str,
    pub handler: Option<Handler<P>>,
}

impl<P: ExternalPrinter + Send + Sync + 'static> Default for SubCommand<P> {
    fn default() -> Self {
        SubCommand {
            name: String::new(),
            sub_commands: vec![],
            options: vec![],
            help: "",
            handler: None,
        }
    }
}

pub struct CommandOption {
    pub name: &'static str,
    pub help: &'static str,
}

pub struct CommandManager<P: ExternalPrinter + Send + Sync + 'static> {
    pub commands: Vec<Command<P>>,
}

impl<P: ExternalPrinter + Send + Sync + 'static> CommandManager<P> {
    fn create_commands() -> Vec<Command<P>> {
        vec![
            Command {
                name: "list",
                sub_commands: vec![
                    SubCommand {
                        name: "servers".to_string(),
                        help: "List the server names.",
                        handler: Some(Self::list_servers_handler),
                        ..Default::default()
                    },
                    SubCommand {
                        name: "mods".to_string(),
                        help: "List installed mods with their current versions and check for updates on Modrinth.",
                        options: vec![CommandOption {
                            name: "update",
                            help: "Whether to update the mod if available.",
                        }],
                        handler: Some(Self::list_mods_handler),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            Command {
                name: "search",
                sub_commands: vec![
                    SubCommand {
                        name: "server-versions".to_string(),
                        options: vec![
                            CommandOption {
                                name: "all",
                                help: "List all versions, stable and unstable.",
                            },
                            CommandOption {
                                name: "stable-only",
                                help: "List only stable versions.",
                            },
                        ],
                        help: "List Minecraft, Fabric Loader, and Fabric Installer versions from fabric-meta.",
                        handler: Some(Self::search_server_versions_handler),
                        ..Default::default()
                    },
                    SubCommand {
                        name: "mods".to_string(),
                        sub_commands: vec![/*Subcommand is the query*/],
                        options: vec![
                            CommandOption {
                                name: "facets",
                                help: "The search facets. For example, \"categories:fabric, versions:1.17.1\".",
                            },
                            CommandOption {
                                name: "index",
                                help: "The index to sort by, e.g., 'downloads'.",
                            },
                            CommandOption {
                                name: "limit",
                                help: "The maximum number of results to return.",
                            },
                        ],
                        help: "Search for mods on Modrinth.",
                        handler: Some(Self::search_mods_handler),
                    },
                    SubCommand {
                        name: "mod-versions".to_string(),
                        sub_commands: vec![/*Subcommand is the project slug*/],
                        options: vec![CommandOption {
                            name: "featured",
                            help: "Allows to filter for featured or non-featured versions only.",
                        }],
                        help: "Get versions of a mod from Modrinth.",
                        handler: Some(Self::search_mod_versions_handler),
                    },
                ],
                ..Default::default()
            },
            Command {
                name: "select",
                help: "Select a server from the servers list to operate on.",
                handler: Some(Self::select_handler),
                ..Default::default()
            },
            Command {
                name: "selected",
                help: "Get the currently selected server.",
                handler: Some(Self::selected_handler),
                ..Default::default()
            },
            Command {
                name: "set",
                sub_commands: vec![
                    SubCommand {
                        name: "max-memory".to_string(),
                        help: "Set the maximum memory for the server.",
                        handler: Some(Self::set_max_memory_handler),
                        ..Default::default()
                    },
                    SubCommand {
                        name: "min-memory".to_string(),
                        help: "Set the minimum memory for the server.",
                        handler: Some(Self::set_min_memory_handler),
                        ..Default::default()
                    },
                    SubCommand {
                        name: "java".to_string(),
                        help: "Set the JAVA_HOME for the server. This will change the start command to use the specified Java.",
                        handler: Some(Self::set_java_home_handler),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            Command {
                name: "add",
                sub_commands: vec![
                    SubCommand {
                        name: "server".to_string(),
                        sub_commands: vec![/*Subcommand is the name of the server to create */],
                        options: vec![
                            CommandOption {
                                name: "game",
                                help: "The Minecraft version.",
                            },
                            CommandOption {
                                name: "loader",
                                help: "The Fabric Loader version.",
                            },
                            CommandOption {
                                name: "installer",
                                help: "The Fabric Installer version.",
                            },
                            CommandOption {
                                name: "latest-stable",
                                help: "Use the latest stable versions for the unspecified.",
                            },
                        ],
                        help: "Download the selected versions for the server.",
                        handler: Some(Self::add_server_handler),
                    },
                    SubCommand {
                        name: "mod".to_string(),
                        sub_commands: vec![/*Subcommand is the mod version ID*/],
                        help: "Download a mod version from Modrinth.",
                        handler: Some(Self::add_mod_handler),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            Command {
                name: "generate",
                sub_commands: vec![SubCommand {
                    name: "start-script".to_string(),
                    help: "Generate a start script for the selected server.",
                    handler: Some(Self::generate_start_script_handler),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Command {
                name: "update",
                sub_commands: vec![SubCommand {
                    name: "server".to_string(),
                    options: vec![
                        CommandOption {
                            name: "game",
                            help: "The Minecraft version.",
                        },
                        CommandOption {
                            name: "loader",
                            help: "The Fabric Loader version.",
                        },
                        CommandOption {
                            name: "installer",
                            help: "The Fabric Installer version.",
                        },
                        CommandOption {
                            name: "latest-stable",
                            help: "Use the latest stable versions for the unspecified.",
                        },
                    ],
                    help: "Update the server executable jar to the specified versions.",
                    handler: Some(Self::update_server_handler),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Command {
                name: "check",
                sub_commands: vec![SubCommand {
                    name: "mods-support".to_string(),
                    sub_commands: vec![/*Subcommand is the game version*/],
                    help: "Check if the mods in the selected server have availible version for the specified game version on Modrinth.",
                    handler: Some(Self::check_mods_support_handler),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Command {
                name: "accept-eula",
                help: "Accept the EULA for the selected server. This will modify the eula.txt file.",
                handler: Some(Self::accept_eula_handler),
                ..Default::default()
            },
            Command {
                name: "start",
                help: "Start the selected server.",
                handler: Some(Self::start_server_handler),
                ..Default::default()
            },
            Command {
                name: "exit",
                help: "Exit the program.",
                handler: Some(Self::exit_handler),
                ..Default::default()
            },
        ]
    }

    fn find_deepest_subcommand<'a>(
        subcommands: &'a [SubCommand<P>],
        tokens: &[String],
    ) -> Option<&'a SubCommand<P>> {
        let mut current = subcommands
            .iter()
            .find(|s| tokens.contains(&s.name.to_string()))?;

        while let Some(next) = current
            .sub_commands
            .iter()
            .find(|s| tokens.contains(&s.name.to_string()))
        {
            current = next;
        }

        Some(current)
    }

    fn list_servers_handler(state: &mut State<P>, _: &[String]) -> anyhow::Result<(), String> {
        if state.server_names.is_empty() {
            println!("Server list is empty.");
            return Ok(());
        }

        for server_name in &state.server_names {
            println!("{server_name}");
        }

        Ok(())
    }

    /// List installed mods in the selected server's mods directory.
    /// Will also check for updates on Modrinth.
    /// If there are updates available, will ask the user if they want to update.
    fn list_mods_handler(state: &mut State<P>, tokens: &[String]) -> anyhow::Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        let mods_path = format!("instances/{}/mods", selected_server.name);

        let jar_paths = std::fs::read_dir(&mods_path)
            .map_err(|e| format!("Failed to read mods directory: {e}"))?
            .map(|entry| entry.expect("Failed to read entry").path())
            .filter(|path| path.extension().expect("Failed to get extension") == "jar")
            .collect::<Vec<_>>();

        let mut jar_files = jar_paths
            .iter()
            .map(File::open)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to open jar files: {e}"))?;

        let jar_hashes = jar_files
            .iter_mut()
            .map(jar_parser::calculate_hash)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to calculate hashes: {e}"))?;

        let game_versions = [selected_server.config.game_version.as_str()];

        let (latest_versions_res, old_versions_res) = state.async_runtime.block_on(async {
            tokio::join!(
                modrinth::get_latest_versions(&state.reqwest_client, &jar_hashes, &game_versions),
                modrinth::get_versions(&state.reqwest_client, &jar_hashes,)
            )
        });

        let latest_versions =
            latest_versions_res.map_err(|e| format!("Failed to get latest versions: {e}"))?;

        let old_versions =
            old_versions_res.map_err(|e| format!("Failed to get old versions: {e}"))?;

        let slug_map = state
            .async_runtime
            .block_on(modrinth::get_project_slug_map(
                &state.reqwest_client,
                old_versions.iter().map(|v| v.project_id.as_str()),
            ))
            .map_err(|e| format!("Failed to get project slugs: {e}"))?;

        let mut available_updates = Vec::new();

        for ((latest_version, old_version), jar_path) in latest_versions
            .into_iter()
            .zip(old_versions.into_iter())
            .zip(jar_paths.iter())
        {
            let project_slug = slug_map.get(&old_version.project_id).unwrap();
            print!("{}: `{}` ", project_slug, old_version.version_name);

            if latest_version.hash == old_version.hash {
                println!("[OK] up-to-date");
            } else {
                println!("-> `{}`", latest_version.version_name);
                available_updates.push((jar_path, latest_version));
            }
        }

        println!("You have {} mods installed.", jar_files.len());
        println!("You have {} available updates:", available_updates.len());

        let should_update = tokens.contains(&"--update".to_string());
        if should_update {
            println!("Updating mods...");

            let downloads = available_updates.iter().map(|(_, version)| {
                let url = version.file_url.clone();
                let save_path = format!(
                    "instances/{}/mods/{}",
                    selected_server.name, version.file_name
                )
                .into();
                (url, save_path)
            });

            state
                .async_runtime
                .block_on(network::download_files(&state.reqwest_client, downloads))
                .map_err(|e| format!("Failed to download updates: {e}"))?;

            // Delete old jar files
            for (jar_path, _) in &available_updates {
                if let Err(e) = fs::remove_file(jar_path) {
                    // Do not return error here, because we want to delete the rest.
                    eprintln!("Failed to delete old jar file: {e}");
                }
            }

            println!("Mods updated successfully.");
        }

        Ok(())
    }

    fn search_server_versions_handler(
        state: &mut State<P>,
        tokens: &[String],
    ) -> Result<(), String> {
        let start = SystemTime::now();

        let mode = match (
            tokens.contains(&"--stable-only".to_string()),
            tokens.contains(&"--all".to_string()),
        ) {
            (true, true) => {
                return Err("--stable-only and --all are mutually exclusive.".to_string());
            }
            (false, false) => PrintVersionMode::StableOnly,
            (_, true) => PrintVersionMode::All,
            (true, _) => PrintVersionMode::StableOnly,
        };

        state
            .async_runtime
            .block_on(fabric_meta::print_versions(&state.reqwest_client, mode))
            .map_err(|e| format!("print versions failed. {e}"))?;

        let end = SystemTime::now();
        let duration = end.duration_since(start).unwrap();

        println!("Took {}ms", duration.as_millis());

        Ok(())
    }

    fn search_mods_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        let query = match tokens.get(2) {
            Some(q) if !q.starts_with("-") => q,
            _ => return Err("No query provided.".to_string()),
        };

        // Add game version & fabric facets to the search
        let version_facet = format!("versions:{}", selected_server.config.game_version);
        let fabric_facet = "categories:fabric";

        let facets = match Self::get_option_value("facets", tokens) {
            Ok(f) => f
                .split(',')
                .map(|f| f.trim())
                .chain(std::iter::once(version_facet.as_str()))
                .chain(std::iter::once(fabric_facet))
                .collect(),
            Err(OptionError::InvalidOption) => vec![version_facet.as_str(), fabric_facet],
            Err(OptionError::MissingValue) => {
                return Err("Missing --facets option value.".to_string());
            }
        };

        let index = match Self::get_option_value("index", tokens) {
            Ok(i) => Some(i.as_str()),
            Err(OptionError::InvalidOption) => None,
            Err(OptionError::MissingValue) => {
                return Err("Missing --index option value.".to_string());
            }
        };

        let limit = match Self::get_option_value("limit", tokens) {
            Ok(l) => Some(l.as_str()),
            Err(OptionError::InvalidOption) => None,
            Err(OptionError::MissingValue) => {
                return Err("Missing --limit option value.".to_string());
            }
        };

        let response = state
            .async_runtime
            .block_on(modrinth::search(
                &state.reqwest_client,
                query,
                &facets,
                index,
                limit,
            ))
            .map_err(|e| format!("Search failed: {e}"))?;

        println!("{response}");

        Ok(())
    }

    fn search_mod_versions_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        let game_version = &selected_server.config.game_version;

        let project_slug = match tokens.get(2) {
            Some(s) if !s.starts_with("-") => s,
            _ => return Err("No project slug provided.".to_string()),
        };

        let featured = match Self::get_option_value("featured", tokens) {
            Ok(f) if f == "true" => Some(true),
            Ok(f) if f == "false" => Some(false),
            Ok(_) => return Err("Invalid value for --featured option.".to_string()),
            Err(OptionError::InvalidOption) => None,
            Err(OptionError::MissingValue) => {
                return Err("Missing --featured option value.".to_string());
            }
        };

        let response = state
            .async_runtime
            .block_on(modrinth::get_project_versions(
                &state.reqwest_client,
                project_slug,
                &[game_version],
                featured,
            ))
            .map_err(|e| format!("Failed to get project versions: {e}"))?;

        println!("{response}");
        Ok(())
    }

    fn select_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let server_name = tokens
            .get(1)
            .ok_or_else(|| "No server name provided.".to_string())?;

        state
            .select_server(server_name.to_owned())
            .map_err(|e| format!("Failed to select server. Error: {e}"))?;

        let selected_server = state.selected_server.as_ref().unwrap();

        println!("Selected server: {server_name}");
        println!(
            "JAR: {}",
            selected_server.config.start_command.get_jar_name()
        );
        println!("Fork: {:?}", selected_server.config.server_fork);
        println!("Game version: {}", selected_server.config.game_version);

        Ok(())
    }

    fn selected_handler(state: &mut State<P>, _: &[String]) -> Result<(), String> {
        let server_name = &state
            .selected_server
            .as_ref()
            .ok_or("No server is selected.".to_string())?
            .name;
        println!("{server_name}");

        Ok(())
    }

    fn set_max_memory_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let max_memory = tokens.get(1).ok_or("No max memory provided.")?;
        let selected_server = state
            .selected_server
            .as_mut()
            .ok_or("No server selected.")?;

        selected_server
            .config
            .start_command
            .set_max_memory(max_memory)
            .unwrap();

        selected_server
            .config
            .save(&selected_server.name)
            .map_err(|e| format!("Failed to save config. Error: {e}"))
    }

    fn set_min_memory_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let min_memory = tokens.get(1).ok_or("No min memory provided.")?;
        let selected_server = state
            .selected_server
            .as_mut()
            .ok_or("No server selected.")?;

        selected_server
            .config
            .start_command
            .set_min_memory(min_memory)
            .unwrap();

        selected_server
            .config
            .save(&selected_server.name)
            .map_err(|e| format!("Failed to save config. Error: {e}"))
    }

    fn set_java_home_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let java_home = tokens.get(2).ok_or("No JAVA_HOME provided.")?;

        // Check if the path exists
        if fs::metadata(java_home).is_err() {
            return Err(format!("The path {java_home} does not exist."));
        }

        let selected_server = state
            .selected_server
            .as_mut()
            .ok_or("No server selected.")?;

        selected_server.config.java_home = Some(java_home.to_string());

        selected_server
            .config
            .save(&selected_server.name)
            .map_err(|e| format!("Failed to save config. Error: {e}"))
    }

    fn add_server_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let server_name = match tokens.get(2) {
            Some(s) if !s.starts_with("-") => s,
            _ => return Err("No server name provided.".to_string()),
        };

        let save_dir_path = format!("instances/{server_name}");

        let start_time = SystemTime::now();

        println!("Fetching versions...");
        let (game_version, loader_version, installer_version) = state
            .async_runtime
            .block_on(Self::get_versions(&state.reqwest_client, tokens))
            .map_err(|e| format!("Failed to get versions: {e}"))?;

        println!("Downloading server jar...");
        let filename = state
            .async_runtime
            .block_on(fabric_meta::download_server(
                &state.reqwest_client,
                &game_version,
                &loader_version,
                &installer_version,
                save_dir_path.clone(),
            ))
            .map_err(|e| format!("Failed to download server jar: {e}"))?;

        let elapsed_time = start_time.elapsed().unwrap();
        println!(
            "Download complete. Duration: {}ms",
            elapsed_time.as_millis()
        );

        let config = Config::new(format!("{save_dir_path}/{filename}"))
            .map_err(|e| format!("Failed to create a new config. Error: {e}."))?;
        config
            .save(server_name)
            .map_err(|e| format!("Failed to save config: {e}"))?;
        println!("Config created and saved");

        state
            .update_server_names()
            .map_err(|e| format!("Failed to update server names. Error: {e}"))?;

        println!("Server added: {server_name}");
        Ok(())
    }

    fn add_mod_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let version_id = match tokens.get(2) {
            Some(id) if !id.starts_with("-") => id,
            _ => return Err("No mod version ID provided.".to_string()),
        };

        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        println!("Downloading mod version {version_id}...");

        let file_name = state
            .async_runtime
            .block_on(modrinth::download_version(
                &state.reqwest_client,
                version_id,
                format!("instances/{}/mods", selected_server.name),
            ))
            .map_err(|e| format!("Failed to download mod version: {e}"))?;

        println!("Mod version downloaded: {file_name}");
        Ok(())
    }

    fn generate_start_script_handler(state: &mut State<P>, _: &[String]) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        let start_script = selected_server
            .config
            .create_start_script()
            .map_err(|e| format!("Failed to create start script: {e}"))?;

        // Write the start script to a file
        let (content, extension) = match start_script {
            StartScript::Windows(script) => (script, "bat"),
            StartScript::Unix(script) => (script, "sh"),
        };

        let script_path = format!(
            "instances/{}/start_script.{}",
            selected_server.name, extension
        );
        let mut file = fs::File::create(&script_path)
            .map_err(|e| format!("Failed to create start script file: {e}"))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write start script to file: {e}"))?;

        Ok(())
    }

    fn update_server_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_mut()
            .ok_or("No server selected.")?;

        println!("Updating server jar...");

        let start_time = SystemTime::now();

        println!("Fetching versions...");

        let (game_version, loader_version, installer_version) = state
            .async_runtime
            .block_on(Self::get_versions(&state.reqwest_client, tokens))
            .map_err(|e| format!("Failed to get versions: {e}"))?;

        println!("Downloading new server jar...");

        let file_name = state
            .async_runtime
            .block_on(fabric_meta::download_server(
                &state.reqwest_client,
                &game_version,
                &loader_version,
                &installer_version,
                format!("instances/{}", selected_server.name),
            ))
            .map_err(|e| format!("Failed to download server jar: {e}"))?;

        println!("Deleting old server jar...");

        let old_jar_name = selected_server.config.start_command.get_jar_name();
        let old_jar_path = format!("instances/{}/{}", selected_server.name, old_jar_name);
        fs::remove_file(&old_jar_path)
            .map_err(|e| format!("Failed to delete old server jar: {e}"))?;

        println!("Updating config...");
        selected_server
            .config
            .start_command
            .set_jar(file_name)
            .map_err(|e| format!("Failed to set new jar in config: {e}."))?;

        selected_server
            .config
            .save(&selected_server.name)
            .map_err(|e| format!("Failed to save config: {e}"))?;

        let elapsed_time = start_time.elapsed().unwrap();
        println!("Update complete. Duration: {}ms", elapsed_time.as_millis());

        Ok(())
    }

    fn check_mods_support_handler(state: &mut State<P>, tokens: &[String]) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        let game_version = tokens.get(2).ok_or("No game version specified.")?;

        let mods_path = format!("instances/{}/mods", selected_server.name);

        let jar_hashes = std::fs::read_dir(&mods_path)
            .map_err(|e| format!("Failed to read mods directory: {e}"))?
            .map(|entry| entry.expect("Failed to read entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "jar"))
            .map(|path| {
                let mut file = File::open(&path)
                    .map_err(|e| format!("Failed to open jar file {}: {e}", path.display()))?;
                jar_parser::calculate_hash(&mut file)
                    .map_err(|e| format!("Failed to calculate hash for {}: {e}", path.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mod_versions = state
            .async_runtime
            .block_on(modrinth::get_versions(&state.reqwest_client, &jar_hashes))
            .map_err(|e| format!("Failed to get mod versions: {e}"))?;

        let slug_map = state
            .async_runtime
            .block_on(modrinth::get_project_slug_map(
                &state.reqwest_client,
                mod_versions.iter().map(|v| v.project_id.as_str()),
            ))
            .map_err(|e| format!("Failed to get project slugs: {e}"))?;

        // Prepare all futures for concurrent execution
        let futures = mod_versions.iter().map(|version| {
            let project_slug = slug_map.get(&version.project_id).unwrap();
            let client = &state.reqwest_client;
            async move {
                let response =
                    modrinth::get_project_versions(client, project_slug, &[game_version], None)
                        .await;
                (project_slug, response)
            }
        });

        // Run all futures concurrently
        let results = state
            .async_runtime
            .block_on(async { futures::future::join_all(futures).await });

        let mut supported_count = 0;
        let mut unsupported_count = 0;

        for (project_slug, response) in results {
            match response {
                Ok(resp) => {
                    let supported = !resp.versions().is_empty();
                    if supported {
                        println!("{project_slug}: [OK] supported");
                        supported_count += 1;
                    } else {
                        println!("{project_slug}: unsupported");
                        unsupported_count += 1;
                    }
                }
                Err(e) => {
                    println!("{project_slug}: failed to check support: {e}");
                    // Do not return error here, because we want to check the rest.
                    unsupported_count += 1;
                }
            }
        }

        println!("Supported mods: {supported_count}, Unsupported mods: {unsupported_count}");

        Ok(())
    }

    /// Start the selected server and exit with code 0
    fn start_server_handler(state: &mut State<P>, _: &[String]) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        // Set the current directory to the server's instance directory,
        // or else the server will generate files in the wrong place
        env::set_current_dir(format!("instances/{}", selected_server.name))
            .map_err(|e| format!("Failed to set current directory: {e}"))?;

        let start_cmd = selected_server.config.start_command.split();

        // Start the server in this terminal
        println!("Starting server...");

        let mut child_command = process::Command::new(&start_cmd[0]);
        child_command.args(&start_cmd[1..]);

        if let Some(java_home) = &selected_server.config.java_home {
            println!("Using JAVA_HOME: {java_home}");
            let default_path = env::var("PATH").expect("Every system should have a PATH variable");
            let new_path = if cfg!(target_os = "windows") {
                format!("{java_home}\\bin;{default_path}")
            } else {
                format!("{java_home}/bin:{default_path}")
            };
            child_command.env("PATH", new_path);
        } else {
            println!("Using system default Java");
        }

        let mut child = child_command
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start server: {e}"))?;

        let stdout = child.stdout.take().expect("Failed to get server's stdout");
        let reader = BufReader::new(stdout);

        // Thread that reads the server's output and prints
        let printer = state.external_printer.clone();
        std::thread::spawn(move || {
            let mut printer = printer.lock().unwrap();
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        printer.print(l).expect("Failed to print line");
                    }
                    Err(e) => eprintln!("Failed to read line from server: {e}"),
                }
            }
        });

        Ok(())
    }

    fn accept_eula_handler(state: &mut State<P>, _: &[String]) -> Result<(), String> {
        let eula_path = format!(
            "instances/{}/eula.txt",
            state
                .selected_server
                .as_ref()
                .ok_or("No server selected.")?
                .name
        );
        let content =
            fs::read_to_string(&eula_path).map_err(|e| format!("Failed to read file: {e}"))?;
        let new_content = content.replace("eula=false", "eula=true");
        fs::write(&eula_path, new_content).map_err(|e| format!("Failed to write file: {e}"))?;

        println!(
            "You ran the accept-eula command. This means you agree to the Minecraft EULA. \
        multi-server will automatically set 'eula=true' in eula.txt for this server. \
        Please ensure you have read and understood the EULA at: https://aka.ms/MinecraftEULA"
        );

        Ok(())
    }

    fn exit_handler(_: &mut State<P>, _: &[String]) -> anyhow::Result<(), String> {
        process::exit(0)
    }

    pub fn execute(line: &str, state: &mut State<P>) -> Result<(), String> {
        if line.trim().is_empty() {
            return Ok(());
        }

        let tokens = shlex::split(line).ok_or("Failed to parse command".to_string())?;

        let command = state
            .commands()
            .iter()
            .find(|cmd| cmd.name == tokens[0])
            .ok_or(format!("Unknown command: {}", tokens[0]))?;

        let deepest_sub_command = Self::find_deepest_subcommand(&command.sub_commands, &tokens);

        let handler = match deepest_sub_command {
            Some(subcommand) => subcommand.handler.or(command.handler),
            None => command.handler,
        }
        .ok_or("Command does not have a handler.".to_string())?;

        handler(state, &tokens)
    }

    fn suggest_subcommands(
        subs: &[SubCommand<P>],
        last_token: Option<&String>,
        input: &str,
    ) -> Vec<SmartCandidate> {
        subs.iter()
            .filter(|s| {
                input.chars().last().unwrap().is_whitespace()
                    || last_token.is_none_or(|t| s.name.starts_with(t))
            })
            .map(|s| SmartCandidate {
                word: s.name.to_string(),
                desc: s.help,
            })
            .collect()
    }

    fn suggest_options(
        options: &[CommandOption],
        tokens: &[String],
        input: &str,
    ) -> Vec<SmartCandidate> {
        let last_char = input.chars().last().unwrap();

        options
            .iter()
            .filter(|opt| {
                last_char.is_whitespace()
                    || last_char == '-'
                    || tokens
                        .last()
                        .is_none_or(|t| opt.name.starts_with(t.trim_start_matches("-")))
            })
            .filter(|opt| !tokens.contains(&format!("--{}", opt.name)))
            .map(|opt| SmartCandidate {
                word: format!("--{}", opt.name),
                desc: opt.help,
            })
            .collect()
    }

    fn get_option_value<'a>(
        option_name: &str,
        tokens: &'a [String],
    ) -> Result<&'a String, OptionError> {
        tokens
            .iter()
            .position(|t| t == &format!("--{option_name}"))
            .ok_or(OptionError::InvalidOption)
            .and_then(|i| {
                tokens
                    .get(i + 1)
                    .ok_or(OptionError::MissingValue)
                    .and_then(|v| {
                        if v.starts_with("--") {
                            Err(OptionError::MissingValue)
                        } else {
                            Ok(v)
                        }
                    })
            })
    }

    async fn get_versions(
        client: &reqwest::Client,
        tokens: &[String],
    ) -> Result<(String, String, String), String> {
        let use_latest_stable = tokens.contains(&"--latest-stable".to_string());
        let (latest_stable_game, latest_stable_loader, latest_stable_installer) =
            if use_latest_stable {
                let versions = fabric_meta::fetch_latest_stable_versions(client)
                    .await
                    .map_err(|e| format!("Failed to fetch latest stable versions: {e}"))?;

                (Some(versions.0), Some(versions.1), Some(versions.2))
            } else {
                (None, None, None)
            };

        let game_version = Self::get_option_value("game", tokens)
            .ok()
            .or(latest_stable_game.as_ref())
            .ok_or("Missing or invalid --game option")?;

        let loader_version = Self::get_option_value("loader", tokens)
            .ok()
            .or(latest_stable_loader.as_ref())
            .ok_or("Missing or invalid --loader option")?;

        let installer_version = Self::get_option_value("installer", tokens)
            .ok()
            .or(latest_stable_installer.as_ref())
            .ok_or("Missing or invalid --installer option")?;

        Ok((
            game_version.to_owned(),
            loader_version.to_owned(),
            installer_version.to_owned(),
        ))
    }
}

impl<P: ExternalPrinter + Send + Sync + 'static> Helper for CommandManager<P> {}
impl<P: ExternalPrinter + Send + Sync + 'static> Hinter for CommandManager<P> {
    type Hint = String;
}
impl<P: ExternalPrinter + Send + Sync + 'static> Highlighter for CommandManager<P> {}
impl<P: ExternalPrinter + Send + Sync + 'static> Validator for CommandManager<P> {}
impl<P: ExternalPrinter + Send + Sync + 'static> Completer for CommandManager<P> {
    type Candidate = SmartCandidate;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
        let input = &line[..pos];
        let tokens = shlex::split(input).unwrap_or_default();
        let start_pos = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);

        if tokens.is_empty() {
            return Ok((0, vec![]));
        }

        let cmd_name = &tokens[0];
        let maybe_cmd = self.commands.iter().find(|cmd| cmd.name == cmd_name);

        match maybe_cmd {
            Some(cmd) => {
                let last_sub_cmd = Self::find_deepest_subcommand(&cmd.sub_commands, &tokens);

                // Suggest subcommands or options
                let suggestions = match last_sub_cmd {
                    // From last subcommand
                    Some(last_sub_cmd) => {
                        if last_sub_cmd.sub_commands.is_empty() {
                            Self::suggest_options(&last_sub_cmd.options, &tokens, input)
                        } else {
                            Self::suggest_subcommands(
                                &last_sub_cmd.sub_commands,
                                tokens.last(),
                                input,
                            )
                        }
                    }

                    // From main command
                    None => {
                        if cmd.sub_commands.is_empty() {
                            Self::suggest_options(&cmd.options, &tokens, input)
                        } else {
                            Self::suggest_subcommands(&cmd.sub_commands, tokens.last(), input)
                        }
                    }
                };

                Ok((start_pos, suggestions))
            }
            None => {
                // Top-level command suggestions
                let suggestions = self
                    .commands
                    .iter()
                    .filter(|cmd| cmd.name.starts_with(cmd_name))
                    .map(|cmd| SmartCandidate {
                        word: cmd.name.to_string(),
                        desc: cmd.help,
                    })
                    .collect();

                Ok((0, suggestions))
            }
        }
    }
}
impl<P: ExternalPrinter + Send + Sync + 'static> Default for CommandManager<P> {
    fn default() -> Self {
        Self {
            commands: Self::create_commands(),
        }
    }
}

pub struct SmartCandidate {
    word: String,
    desc: &'static str,
}

impl Candidate for SmartCandidate {
    fn display(&self) -> &str {
        self.desc
    }

    fn replacement(&self) -> &str {
        &self.word
    }
}

pub fn create_editor<P: ExternalPrinter + Send + Sync + 'static>(
    helper: CommandManager<P>,
) -> Result<Editor<CommandManager<P>, FileHistory>, ReadlineError> {
    let mut editor = Editor::new()?;
    editor.set_completion_show_all_if_ambiguous(true);
    editor.set_helper(Some(helper));

    Ok(editor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::history::History;

    #[test]
    fn test_completer() {
        let cmd_manager = CommandManager::<DummyExternalPrinter> {
            commands: vec![
                Command {
                    name: "cmd1",
                    sub_commands: vec![SubCommand {
                        name: "sub1".to_string(),
                        sub_commands: vec![],
                        help: "",
                        options: vec![CommandOption {
                            name: "opt1",
                            help: "",
                        }],
                        handler: None,
                    }],
                    options: vec![],
                    help: "",
                    handler: None,
                },
                Command {
                    name: "cmd2",
                    sub_commands: vec![],
                    options: vec![],
                    help: "",
                    handler: None,
                },
            ],
        };

        let file_history = FileHistory::new();
        let history_ref: &dyn History = &file_history;
        let ctx = Context::new(history_ref);

        assert_suggestions(
            &cmd_manager,
            &ctx,
            "cmd",
            &[String::from("cmd1"), String::from("cmd2")],
        );

        assert_suggestions(&cmd_manager, &ctx, "cmd1 ", &[String::from("sub1")]);
        assert_suggestions(&cmd_manager, &ctx, "cmd1 s", &[String::from("sub1")]);

        assert_suggestions(&cmd_manager, &ctx, "cmd1 sub1 ", &[String::from("--opt1")]);
        assert_suggestions(&cmd_manager, &ctx, "cmd1 sub1 -", &[String::from("--opt1")]);
        assert_suggestions(
            &cmd_manager,
            &ctx,
            "cmd1 sub1 --",
            &[String::from("--opt1")],
        );
        assert_suggestions(
            &cmd_manager,
            &ctx,
            "cmd1 sub1 -o",
            &[String::from("--opt1")],
        );
        assert_suggestions(&cmd_manager, &ctx, "cmd1 sub1 -1", &[]);
    }

    fn assert_suggestions(
        completer: &CommandManager<impl ExternalPrinter + Send + Sync + 'static>,
        ctx: &Context,
        line: &str,
        expected: &[String],
    ) {
        let suggestions = completer
            .complete(line, line.len(), &ctx)
            .unwrap()
            .1
            .into_iter()
            .map(|sc| sc.word)
            .collect::<Vec<_>>();

        assert!(expected.iter().all(|ex| suggestions.contains(&ex)));
    }

    struct DummyExternalPrinter;
    impl ExternalPrinter for DummyExternalPrinter {
        fn print(&mut self, _msg: String) -> rustyline::Result<()> {
            Ok(())
        }
    }
}
