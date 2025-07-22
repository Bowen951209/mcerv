mod command;
mod config;
mod fabric_meta;
mod state;

use crate::{command::CommandManager, config::Config, state::State};
use rustyline::error::ReadlineError;
use std::fs;

pub fn run() -> anyhow::Result<()> {
    let mut editor = command::create_editor()?;
    let cmd_manager = CommandManager::new();

    // Create instances directory if it doesn't exist
    fs::create_dir_all("instances")?;

    let mut state = State::default();
    state.update_server_names()?;

    loop {
        let readline = editor.readline(">> ");
        match readline {
            Ok(line) => {
                let line = line.trim();

                editor.add_history_entry(line)?;
                cmd_manager
                    .execute(line, &mut state)
                    .unwrap_or_else(|e| eprintln!("Error executing command: {}", e));
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}
