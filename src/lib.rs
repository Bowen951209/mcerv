mod network;
mod system;

use rustyline::error::ReadlineError;
use std::{
    fs,
    sync::{Arc, Mutex},
};

use crate::system::{
    command::{self, CommandManager},
    state::State,
};

pub fn run() -> anyhow::Result<()> {
    // Create instances directory if it doesn't exist
    fs::create_dir_all("instances")?;

    let cmd_manager = CommandManager::default();
    let mut editor = command::create_editor(cmd_manager)?;
    let external_printer = Arc::new(Mutex::new(editor.create_external_printer()?));
    let mut state = State {
        editor,
        async_runtime: tokio::runtime::Runtime::new()?,
        reqwest_client: reqwest::Client::new(),
        external_printer,
        selected_server: None,
        server_names: vec![],
    };

    state.update_server_names()?; // Update server names for auto-completion

    loop {
        let readline = state.editor.readline(">> ");
        match readline {
            Ok(line) => {
                let line = line.trim();

                state.editor.add_history_entry(line)?;

                CommandManager::execute(line, &mut state)
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
