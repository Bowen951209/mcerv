mod command;
mod config;
mod fabric_meta;
mod state;
use std::path::Path;

use crate::{command::CommandManager, config::Config, state::State};
use rustyline::error::ReadlineError;

pub fn run() -> anyhow::Result<()> {
    let mut editor = command::create_editor()?;
    let cmd_manager = CommandManager::new();

    // load default config
    let config_path = Path::new("config.json");
    let mut config = Config::load(config_path).expect("Failed to load config");
    println!("Loaded config: {:?}", config_path);

    // Check if config is valid (might have been changed by the user manually)
    config.check_validity()?;

    // If user added new folders to instances, add them to the config
    config.add_new_folders_to_config()?;

    // Remove any missing servers from the config
    config.retain_valid()?;

    // Save the config
    config.save()?;

    let mut state = State::new(config);

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
