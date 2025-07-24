use std::{
    env,
    error::Error,
    fmt::Display,
    fs::{self},
    process::{self},
    time::SystemTime,
};

use rustyline::{
    Context, Editor, Helper,
    completion::{Candidate, Completer},
    config::Configurer,
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    history::FileHistory,
    validate::Validator,
};

use crate::{
    config::Config,
    fabric_meta::{self, PrintVersionMode},
    state::State,
};

type Handler = fn(&mut CommandManager, &mut State, &[String]) -> Result<(), String>;

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

pub struct Command {
    pub name: &'static str,
    pub sub_commands: Vec<SubCommand>,
    pub options: Vec<CommandOption>,
    pub help: &'static str,
    pub handler: Option<Handler>,
}

pub struct SubCommand {
    // This is not a &'static str because we will define subcommands
    // at runtime. For example, the command `select` will have a variable
    // list of server names as subcommands for auto completion.
    pub name: String,
    pub sub_commands: Vec<SubCommand>,
    pub options: Vec<CommandOption>,
    pub help: &'static str,
    pub handler: Option<Handler>,
}

pub struct CommandOption {
    pub name: &'static str,
    pub help: &'static str,
}

pub struct CommandManager {
    pub commands: Vec<Command>,
    pub async_runtime: tokio::runtime::Runtime,
}

impl CommandManager {
    pub fn new() -> Self {
        CommandManager {
            commands: Self::create_commands(),
            async_runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    fn create_commands() -> Vec<Command> {
        vec![
            Command {
                name: "list",
                sub_commands: vec![
                    SubCommand {
                        name: "versions".to_string(),
                        sub_commands: vec![],
                        help: "List Minecraft, Fabric Loader, and Fabric Installer versions from fabric-meta.",
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
                        handler: Some(Self::list_versions_handler),
                    },
                    SubCommand {
                        name: "servers".to_string(),
                        sub_commands: vec![],
                        help: "List the servers in the config file.",
                        options: vec![],
                        handler: Some(Self::list_servers_handler),
                    },
                ],
                options: vec![],
                help: "",
                handler: None,
            },
            Command {
                name: "select",
                sub_commands: vec![],
                options: vec![],
                help: "Select a server from the servers list to operate on.",
                handler: Some(Self::select_handler),
            },
            Command {
                name: "selected",
                sub_commands: vec![],
                options: vec![],
                help: "Get the currently selected server.",
                handler: Some(Self::selected_handler),
            },
            Command {
                name: "set",
                sub_commands: vec![
                    SubCommand {
                        name: "max-memory".to_string(),
                        sub_commands: vec![],
                        help: "Set the maximum memory for the server.",
                        options: vec![],
                        handler: Some(Self::set_max_memory_handler),
                    },
                    SubCommand {
                        name: "min-memory".to_string(),
                        sub_commands: vec![],
                        help: "Set the minimum memory for the server.",
                        options: vec![],
                        handler: Some(Self::set_min_memory_handler),
                    },
                ],
                options: vec![],
                help: "",
                handler: None,
            },
            Command {
                name: "add",
                sub_commands: vec![],
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
                        name: "name",
                        help: "The name of the server folder to create.",
                    },
                    CommandOption {
                        name: "latest-stable",
                        help: "Use the latest stable versions for the unspecified.",
                    },
                ],
                help: "Download the selected versions for the server.",
                handler: Some(Self::add_server_handler),
            },
            Command {
                name: "accept-eula",
                sub_commands: vec![],
                options: vec![],
                help: "Accept the EULA for the selected server. This will modify the eula.txt file.",
                handler: Some(Self::accept_eula_handler),
            },
            Command {
                name: "start",
                sub_commands: vec![],
                options: vec![],
                help: "Start the selected server.",
                handler: Some(Self::start_server_handler),
            },
            Command {
                name: "exit",
                sub_commands: vec![],
                options: vec![],
                help: "Exit the program.",
                handler: Some(Self::exit_handler),
            },
        ]
    }

    fn find_deepest_subcommand<'a>(
        subcommands: &'a [SubCommand],
        tokens: &[String],
    ) -> Option<&'a SubCommand> {
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

    fn list_versions_handler(
        cmd_manager: &mut CommandManager,
        _: &mut State,
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

        cmd_manager
            .async_runtime
            .block_on(fabric_meta::print_versions(mode))
            .map_err(|e| format!("print versions failed. {}", e))?;

        let end = SystemTime::now();
        let duration = end.duration_since(start).unwrap();

        println!("Took {}ms", duration.as_millis());

        Ok(())
    }

    fn list_servers_handler(
        _: &mut CommandManager,
        state: &mut State,
        _: &[String],
    ) -> anyhow::Result<(), String> {
        if state.server_names.is_empty() {
            println!("Server list is empty.");
            return Ok(());
        }

        for server_name in &state.server_names {
            println!("{server_name}");
        }

        Ok(())
    }

    fn select_handler(
        _: &mut CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let server_name = tokens
            .get(1)
            .ok_or_else(|| "No server name provided.".to_string())?;

        state
            .select_server(server_name.to_owned())
            .map_err(|e| format!("Failed to select server. Error: {e}"))?;

        Ok(())
    }

    fn selected_handler(
        _: &mut CommandManager,
        state: &mut State,
        _: &[String],
    ) -> Result<(), String> {
        let server_name = &state
            .selected_server
            .as_ref()
            .ok_or("No server is selected.".to_string())?
            .name;
        println!("{server_name}");

        Ok(())
    }

    fn set_max_memory_handler(
        _: &mut CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let max_memory = tokens.get(1).ok_or("No max memory provided.")?;
        let selected_server = state
            .selected_server
            .as_mut()
            .ok_or("No server selected.")?;

        selected_server.config.set_max_memory(max_memory).unwrap();

        selected_server
            .config
            .save(&selected_server.name)
            .map_err(|e| format!("Failed to save config. Error: {e}"))
    }

    fn set_min_memory_handler(
        _: &mut CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let min_memory = tokens.get(1).ok_or("No min memory provided.")?;
        let selected_server = state
            .selected_server
            .as_mut()
            .ok_or("No server selected.")?;

        selected_server.config.set_min_memory(min_memory).unwrap();

        selected_server
            .config
            .save(&selected_server.name)
            .map_err(|e| format!("Failed to save config. Error: {e}"))
    }

    fn add_server_handler(
        cmd_manager: &mut CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let use_latest_stable = tokens.contains(&"--latest-stable".to_string());
        let (latest_stable_game, latest_stable_loader, latest_stable_installer) =
            if use_latest_stable {
                let versions = cmd_manager
                    .async_runtime
                    .block_on(fabric_meta::fetch_latest_stable_versions())
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

        let server_name = Self::get_option_value("name", tokens)
            .map_err(|e| format!("Missing or invalid --name option: {e}"))?;

        let save_dir_path = format!("instances/{server_name}");

        let start_time = SystemTime::now();

        println!("Downloading server jar...");
        let filename = cmd_manager
            .async_runtime
            .block_on(fabric_meta::download_server(
                game_version,
                loader_version,
                installer_version,
                save_dir_path.clone(),
            ))
            .map_err(|e| format!("Failed to download server jar: {e}"))?;

        let elapsed_time = start_time.elapsed().unwrap();
        println!(
            "Download complete. Duration: {}ms",
            elapsed_time.as_millis()
        );

        let config = Config::new(&filename);
        config
            .save(&server_name)
            .map_err(|e| format!("Failed to save config: {e}"))?;

        state
            .update_server_names(cmd_manager)
            .map_err(|e| format!("Failed to update server names. Error: {e}"))?;

        println!("Server added: {server_name}");
        Ok(())
    }

    /// Start the selected server and exit with code 0
    fn start_server_handler(
        _: &mut CommandManager,
        state: &mut State,
        _: &[String],
    ) -> Result<(), String> {
        let selected_server = state
            .selected_server
            .as_ref()
            .ok_or("No server selected.")?;

        env::set_current_dir(format!("instances/{}", selected_server.name))
            .map_err(|e| format!("Failed to set current directory: {e}"))?;

        let start_cmd = shlex::split(&selected_server.config.start_command)
            .ok_or("The start command is invalid")?;

        // Start the server in this terminal
        println!("Starting server...");
        let mut child = process::Command::new(&start_cmd[0])
            .args(&start_cmd[1..])
            .spawn()
            .map_err(|e| format!("Failed to start server: {e}"))?;

        // This wait is necessary. Without this, the cursor in the termminal will have some glitches
        child.wait().unwrap();

        println!("Exiting multi-server.");
        process::exit(0);
    }

    fn accept_eula_handler(
        _: &mut CommandManager,
        state: &mut State,
        _: &[String],
    ) -> Result<(), String> {
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

    fn exit_handler(
        _: &mut CommandManager,
        _: &mut State,
        _: &[String],
    ) -> anyhow::Result<(), String> {
        process::exit(0)
    }

    pub fn execute(&mut self, line: &str, state: &mut State) -> Result<(), String> {
        let tokens = shlex::split(line).ok_or("Failed to parse command".to_string())?;

        let command = self
            .commands
            .iter()
            .find(|cmd| cmd.name == tokens[0])
            .ok_or(format!("Unknown command: {}", tokens[0]))?;

        let deepest_sub_command = Self::find_deepest_subcommand(&command.sub_commands, &tokens);

        let handler = match deepest_sub_command {
            Some(subcommand) => subcommand.handler.or(command.handler),
            None => command.handler,
        }
        .ok_or("Command does not have a handler.".to_string())?;

        handler(self, state, &tokens)
    }

    fn suggest_subcommands<'a>(
        subs: &'a [SubCommand],
        last_token: Option<&String>,
        input: &str,
    ) -> Vec<SmartCandidate> {
        subs.iter()
            .filter(|s| {
                input.chars().last().unwrap().is_whitespace()
                    || last_token.map_or(true, |t| s.name.starts_with(t))
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
                        .map_or(true, |t| opt.name.starts_with(t.trim_start_matches("-")))
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
}

impl Helper for CommandManager {}
impl Hinter for CommandManager {
    type Hint = String;
}
impl Highlighter for CommandManager {}
impl Validator for CommandManager {}
impl Completer for CommandManager {
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
        let maybe_cmd = self.commands.iter().find(|cmd| &cmd.name == cmd_name);

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

pub fn create_editor(
    helper: CommandManager,
) -> Result<Editor<CommandManager, FileHistory>, ReadlineError> {
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
    fn test_smart_completer() {
        let completer = CommandManager {
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
            async_runtime: tokio::runtime::Runtime::new().unwrap(),
        };

        let file_history = FileHistory::new();
        let history_ref: &dyn History = &file_history;
        let ctx = Context::new(history_ref);

        assert_suggestions(
            &completer,
            &ctx,
            "cmd",
            &[String::from("cmd1"), String::from("cmd2")],
        );

        assert_suggestions(&completer, &ctx, "cmd1 ", &[String::from("sub1")]);
        assert_suggestions(&completer, &ctx, "cmd1 s", &[String::from("sub1")]);

        assert_suggestions(&completer, &ctx, "cmd1 sub1 ", &[String::from("--opt1")]);
        assert_suggestions(&completer, &ctx, "cmd1 sub1 -", &[String::from("--opt1")]);
        assert_suggestions(&completer, &ctx, "cmd1 sub1 --", &[String::from("--opt1")]);
        assert_suggestions(&completer, &ctx, "cmd1 sub1 -o", &[String::from("--opt1")]);
        assert_suggestions(&completer, &ctx, "cmd1 sub1 -1", &[]);
    }

    fn assert_suggestions(
        completer: &CommandManager,
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
}
