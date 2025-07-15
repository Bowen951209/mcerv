use std::{fmt::Display, process, time::SystemTime};

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
    fabric_meta::{self, PrintVersionMode},
    state::State,
};

type Handler = fn(&CommandManager, &mut State, &[String]) -> Result<(), String>;

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

struct Command {
    name: &'static str,
    sub_commands: Vec<SubCommand>,
    options: Vec<CommandOption>,
    help: &'static str,
    handler: Option<Handler>,
}

struct SubCommand {
    name: &'static str,
    sub_commands: Vec<SubCommand>,
    options: Vec<CommandOption>,
    help: &'static str,
    handler: Option<Handler>,
}

struct CommandOption {
    name: &'static str,
    help: &'static str,
}

pub struct CommandManager {
    commands: Vec<Command>,
    async_runtime: tokio::runtime::Runtime,
}

impl CommandManager {
    pub fn new() -> Self {
        CommandManager {
            commands: Self::create_commands(),
            async_runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    pub fn get_async_runtime(&self) -> &tokio::runtime::Runtime {
        &self.async_runtime
    }

    fn create_commands() -> Vec<Command> {
        vec![
            Command {
                name: "list",
                sub_commands: vec![
                    SubCommand {
                        name: "versions",
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
                        name: "servers",
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
                name: "set",
                sub_commands: vec![
                    SubCommand {
                        name: "max-memory",
                        sub_commands: vec![],
                        help: "Set the maximum memory for the server.",
                        options: vec![],
                        handler: Some(Self::set_max_memory_handler),
                    },
                    SubCommand {
                        name: "min-memory",
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
                name: "download",
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
                ],
                help: "Download the selected versions for the server.",
                handler: Some(Self::download_server_handler),
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
        cmd_manager: &CommandManager,
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
            .get_async_runtime()
            .block_on(fabric_meta::print_versions(mode))
            .map_err(|e| format!("print versions failed. {}", e))?;

        let end = SystemTime::now();
        let duration = end.duration_since(start).unwrap();

        println!("Took {}ms", duration.as_millis());

        Ok(())
    }

    fn list_servers_handler(
        _: &CommandManager,
        state: &mut State,
        _: &[String],
    ) -> anyhow::Result<(), String> {
        for (server_name, _) in state.get_config().get_servers() {
            println!("{server_name}");
        }

        Ok(())
    }

    fn select_handler(
        _: &CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let server_name = tokens
            .get(1)
            .ok_or_else(|| "No server name provided.".to_string())?;

        state.set_selected_server(server_name.to_owned());

        Ok(())
    }

    fn set_max_memory_handler(
        _: &CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let max_memory = match tokens.get(1) {
            Some(memory) => memory,
            None => return Err("No max memory provided.".to_string()),
        };

        let selected_server = match state.get_selected_server() {
            Some(server) => server,
            None => return Err("No server selected.".to_string()),
        };

        let server_config = state
            .get_config_mut()
            .get_servers_mut()
            .get_mut(&selected_server)
            .unwrap();

        server_config.set_max_memory(max_memory).unwrap();

        state
            .get_config()
            .save()
            .map_err(|e| format!("Failed to save config. Error: {}", e))
    }

    fn set_min_memory_handler(
        _: &CommandManager,
        state: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let min_memory = match tokens.get(1) {
            Some(memory) => memory,
            None => return Err("No min memory provided.".to_string()),
        };

        let selected_server = match state.get_selected_server() {
            Some(server) => server,
            None => return Err("No server selected.".to_string()),
        };

        let server_config = state
            .get_config_mut()
            .get_servers_mut()
            .get_mut(&selected_server)
            .unwrap();

        server_config.set_min_memory(min_memory).unwrap();

        state
            .get_config()
            .save()
            .map_err(|e| format!("Failed to save config. Error: {}", e))
    }

    fn download_server_handler(
        cmd_manager: &CommandManager,
        _: &mut State,
        tokens: &[String],
    ) -> Result<(), String> {
        let game_version = match Self::get_option_value("game", tokens) {
            Ok(val) => val,
            Err(e) => {
                return Err(format!("Missing or invalid --game option: {}", e));
            }
        };

        let fabric_loader_version = match Self::get_option_value("loader", tokens) {
            Ok(val) => val,
            Err(e) => {
                return Err(format!("Missing or invalid --loader option: {}", e));
            }
        };

        let installer_version = match Self::get_option_value("installer", tokens) {
            Ok(val) => val,
            Err(e) => {
                return Err(format!("Missing or invalid --installer option: {}", e));
            }
        };

        println!("Downloading executable server...");
        let start = SystemTime::now();
        cmd_manager
            .get_async_runtime()
            .block_on(fabric_meta::download_server(
                game_version,
                fabric_loader_version,
                installer_version,
            ))
            .map_err(|e| format!("Download executable server (.jar) failed. {}", e))?;
        let end = SystemTime::now();
        let duration = end.duration_since(start).unwrap();
        println!("Download complete. Took {}ms", duration.as_millis());

        Ok(())
    }

    fn exit_handler(_: &CommandManager, _: &mut State, _: &[String]) -> anyhow::Result<(), String> {
        process::exit(0)
    }

    pub fn execute(&self, line: &str, state: &mut State) -> Result<(), String> {
        let tokens = shlex::split(line).unwrap();

        if let Some(main_cmd) = self.commands.iter().find(|c| c.name == tokens[0]) {
            let deepest_sub_cmd = Self::find_deepest_subcommand(&main_cmd.sub_commands, &tokens);

            match deepest_sub_cmd {
                Some(deepest_sub_cmd) => match deepest_sub_cmd.handler {
                    Some(handler) => handler(&self, state, &tokens)?,
                    None => {
                        eprintln!(
                            "Command does not have a handler. May have to provide subcommands."
                        )
                    }
                },
                None => match main_cmd.handler {
                    Some(handler) => handler(&self, state, &tokens)?,
                    None => {
                        eprintln!(
                            "Command does not have a handler. May have to provide subcommands."
                        )
                    }
                },
            }
        } else {
            eprintln!("Unknown command: {}", line);
        }

        Ok(())
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
        let maybe_cmd = self.commands.iter().find(|cmd| cmd.name == cmd_name);

        match maybe_cmd {
            Some(cmd) => {
                let last_sub_cmd = Self::find_deepest_subcommand(&cmd.sub_commands, &tokens);

                // Suggest subcommands or options
                let suggestions = match last_sub_cmd {
                    Some(last_sub_cmd) => {
                        if !last_sub_cmd.sub_commands.is_empty() {
                            Self::suggest_subcommands(
                                &last_sub_cmd.sub_commands,
                                tokens.last(),
                                input,
                            )
                        } else {
                            Self::suggest_options(&last_sub_cmd.options, &tokens, input)
                        }
                    }

                    None => {
                        if !cmd.sub_commands.is_empty() {
                            Self::suggest_subcommands(&cmd.sub_commands, tokens.last(), input)
                        } else {
                            Self::suggest_options(&cmd.options, &tokens, input)
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

pub fn create_editor() -> Result<Editor<CommandManager, FileHistory>, ReadlineError> {
    let mut editor = Editor::new()?;
    editor.set_completion_show_all_if_ambiguous(true);
    editor.set_helper(Some(CommandManager::new()));

    Ok(editor)
}

#[cfg(test)]
mod tests {

    use rustyline::history::History;

    use super::*;

    #[test]
    fn test_smart_completer() {
        let completer = CommandManager {
            commands: vec![
                Command {
                    name: "cmd1",
                    sub_commands: vec![SubCommand {
                        name: "sub1",
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
