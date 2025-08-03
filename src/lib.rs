mod network;
mod system;

use rustyline::error::ReadlineError;
use std::io::Write;
use std::{
    fs,
    sync::{Arc, Mutex},
};

use crate::system::{
    command::{self, CommandManager},
    state::{Context, State},
};

pub fn run() -> anyhow::Result<()> {
    // Create instances directory if it doesn't exist
    fs::create_dir_all("instances")?;

    let cmd_manager = CommandManager::default();
    let mut editor = command::create_editor(cmd_manager)?;
    let external_printer = Arc::new(Mutex::new(editor.create_external_printer()?));
    let mut state = State::new(editor, external_printer);

    state.update_server_names()?; // Update server names for auto-completion

    let mut context = Context::Default;

    loop {
        let readline = state.editor.readline(">> ");
        match readline {
            Ok(line) => {
                let line = line.trim();

                state.editor.add_history_entry(line)?;

                // If the context is updated, we will receive a message from the context channel.
                // In default context, we execute the input command.
                // In Minecraft server context, we write the command to the server's stdin.
                if let Ok(ctx) = state.context_rx.try_recv() {
                    context = ctx;
                }

                match context {
                    Context::Default => {
                        CommandManager::execute(line, &mut state)
                            .unwrap_or_else(|e| eprintln!("Error executing command: {e}"));
                    }
                    Context::MinecraftServer(ref mut writer) => {
                        writeln!(writer, "{line}").expect("Failed to write to stdout");
                        writer.flush().expect("Failed to flush to Minecraft server");
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(e) => {
                println!("Error: {e}");
                break;
            }
        }
    }

    Ok(())
}
