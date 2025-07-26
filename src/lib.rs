mod network;
mod system;

use rustyline::error::ReadlineError;
use std::fs;

use crate::system::{
    command::{self, CommandManager},
    state::State,
};

pub fn run() -> anyhow::Result<()> {
    // Create instances directory if it doesn't exist
    fs::create_dir_all("instances")?;

    let mut cmd_manager = CommandManager::new();

    let mut state = State::default();
    state.update_server_names(&mut cmd_manager)?;

    let mut editor = command::create_editor(cmd_manager)?;

    loop {
        let readline = editor.readline(">> ");
        match readline {
            Ok(line) => {
                let line = line.trim();

                editor.add_history_entry(line)?;

                let cmd_manager = editor.helper_mut().unwrap();
                cmd_manager
                    .execute(line, &mut state)
                    .unwrap_or_else(|e| eprintln!("Error executing command: {e}"));
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
