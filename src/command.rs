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

use crate::fabric_meta::{self, PrintVersionMode};

struct Command {
    name: &'static str,
    sub_commands: Vec<SubCommand>,
    options: Vec<CommandOption>,
    help: &'static str,
}

struct SubCommand {
    name: &'static str,
    sub_commands: Vec<SubCommand>,
    help: &'static str,
}

struct CommandOption {
    name: &'static str,
    help: &'static str,
}

pub struct SmartCompleter {
    commands: Vec<Command>,
}

impl SmartCompleter {
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

    fn suggest_subcommands<'a>(
        subs: &'a [SubCommand],
        last_token: Option<&String>,
        input: &str,
    ) -> Vec<SmartCandidate> {
        subs.iter()
            .filter(|s| {
                input.chars().last().unwrap().is_whitespace()
                    || last_token.map_or(true, |t| s.name.contains(t))
            })
            .map(|s| SmartCandidate {
                word: s.name.to_string(),
                desc: s.help,
            })
            .collect()
    }

    fn suggest_options<'a>(
        options: &'a [CommandOption],
        last_token: Option<&String>,
        input: &str,
    ) -> Vec<SmartCandidate> {
        let last_char = input.chars().last().unwrap();

        options
            .iter()
            .filter(|opt| {
                last_char.is_whitespace()
                    || last_char == '-'
                    || last_token.map_or(true, |t| opt.name.contains(t.trim_start_matches("-")))
            })
            .map(|opt| SmartCandidate {
                word: format!("--{}", opt.name),
                desc: opt.help,
            })
            .collect()
    }
}

impl Helper for SmartCompleter {}
impl Hinter for SmartCompleter {
    type Hint = String;
}
impl Highlighter for SmartCompleter {}
impl Validator for SmartCompleter {}
impl Completer for SmartCompleter {
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
                let last_sub_cmd =
                    SmartCompleter::find_deepest_subcommand(&cmd.sub_commands, &tokens);

                // Suggest subcommands or options
                let suggestions = match last_sub_cmd {
                    Some(last_sub_cmd) => {
                        if !last_sub_cmd.sub_commands.is_empty() {
                            SmartCompleter::suggest_subcommands(
                                &last_sub_cmd.sub_commands,
                                tokens.last(),
                                input,
                            )
                        } else {
                            SmartCompleter::suggest_options(&cmd.options, tokens.last(), input)
                        }
                    }

                    None => {
                        if !cmd.sub_commands.is_empty() {
                            SmartCompleter::suggest_subcommands(
                                &cmd.sub_commands,
                                tokens.last(),
                                input,
                            )
                        } else {
                            SmartCompleter::suggest_options(&cmd.options, tokens.last(), input)
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
                    .filter(|cmd| cmd.name.contains(cmd_name))
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

pub fn get_editor() -> Result<Editor<SmartCompleter, FileHistory>, ReadlineError> {
    let mut editor = Editor::new()?;
    editor.set_completion_show_all_if_ambiguous(true);
    editor.set_helper(Some(get_completer()));

    Ok(editor)
}

fn get_completer() -> SmartCompleter {
    let commands = vec![Command {
        name: "list",
        sub_commands: vec![SubCommand {
            name: "versions",
            sub_commands: vec![],
            help: "List Minecraft, Fabric Loader, and Fabric Installer versions from fabric-meta.",
        }],
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
        help: "",
    }];

    SmartCompleter { commands }
}

pub async fn execute(command: &str) {
    let tokens = shlex::split(command).unwrap();
    let str_tokens = tokens.iter().map(String::as_str).collect::<Vec<_>>();

    match str_tokens[0] {
        "list" if str_tokens[1] == "versions" => {
            let is_stable_only =
                str_tokens.contains(&"--stable-only") || str_tokens.contains(&"-s");
            let is_all = str_tokens.contains(&"--all") || str_tokens.contains(&"-a");

            match (is_stable_only, is_all) {
                (true, true) => {
                    eprintln!("--stable-only and --all are mutually exclusive.");
                    return;
                }
                (false, false) => {
                    fabric_meta::print_versions(PrintVersionMode::StableOnly)
                        .await
                        .unwrap();
                }
                _ => {
                    let mode = if is_all {
                        PrintVersionMode::All
                    } else {
                        PrintVersionMode::StableOnly
                    };
                    fabric_meta::print_versions(mode).await.unwrap();
                }
            }
        }

        _ => {
            eprintln!("Unknown command: {}", shlex::try_join(str_tokens).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {

    use rustyline::history::History;

    use super::*;

    #[test]
    fn test_smart_completer() {
        let completer = SmartCompleter {
            commands: vec![
                Command {
                    name: "cmd1",
                    sub_commands: vec![SubCommand {
                        name: "sub1",
                        sub_commands: vec![],
                        help: "",
                    }],
                    options: vec![CommandOption {
                        name: "opt1",
                        help: "",
                    }],
                    help: "",
                },
                Command {
                    name: "cmd2",
                    sub_commands: vec![],
                    options: vec![],
                    help: "",
                },
            ],
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
        assert_suggestions(&completer, &ctx, "cmd1 sub1 -1", &[String::from("--opt1")]);
    }

    fn assert_suggestions(
        completer: &SmartCompleter,
        ctx: &Context,
        line: &str,
        expected: &[String],
    ) {
        assert!(
            completer
                .complete(line, line.len(), &ctx)
                .unwrap()
                .1
                .into_iter()
                .map(|c| c.word)
                .into_iter()
                .all(|s| expected.contains(&s)),
        );
    }
}
