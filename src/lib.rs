mod network;
mod system;

use rustyline::ExternalPrinter;
use rustyline::error::ReadlineError;
use std::fs;
use std::io::Write;
use std::sync::mpsc;

use crate::system::{
    command::{self, CommandManager},
    state::{Context, State},
};

pub fn run() -> anyhow::Result<()> {
    // Create instances directory if it doesn't exist
    fs::create_dir_all("instances")?;

    let cmd_manager = CommandManager::default();
    let mut editor = command::create_editor(cmd_manager)?;
    let (print_tx, print_rx) = mpsc::channel();
    let external_printer = editor.create_external_printer()?;
    let mut state = State::new(editor, print_tx);

    state.update_server_names()?; // Update server names for auto-completion

    // Start the print thread to handle printing messages
    start_print_thread(print_rx, external_printer);

    let mut context = Context::Default;

    loop {
        // If the context is updated, we will receive a message from the context channel.
        // In default context, we execute the input command.
        // In Minecraft server context, we write the command to the server's stdin.
        if let Ok(ctx) = state.context_rx.try_recv() {
            context = ctx;
        }

        let readline = match context {
            Context::Default => state.editor.readline("multi-server> "),
            Context::MinecraftServer(_) => {
                let selected_server_name = &state.selected_server.as_ref().unwrap().name;
                state.editor.readline(&format!("{selected_server_name}> "))
            }
        };

        match readline {
            Ok(line) => {
                let line = line.trim();

                state.editor.add_history_entry(line)?;

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
                stop_if_minecraft_server(&mut context, state.context_rx)?;
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                stop_if_minecraft_server(&mut context, state.context_rx)?;
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

/// Stops the Minecraft server if the current context is a Minecraft server.
fn stop_if_minecraft_server(
    context: &mut Context,
    context_rx: mpsc::Receiver<Context>,
) -> Result<(), mpsc::RecvError> {
    if let Context::MinecraftServer(writer) = context {
        writeln!(writer, "stop").expect("Failed to write 'stop' command to Minecraft server");
        writer
            .flush()
            .expect("Failed to flush 'stop' command to Minecraft server");
    }

    // Wait for the server to stop. This allows us to see the server's shutdown messages.
    // We will receive a context update when the server stops.
    context_rx.recv()?;

    Ok(())
}

/// Starts a dedicated thread to handle printing messages using `ExternalPrinter`.
fn start_print_thread(
    print_rx: mpsc::Receiver<String>,
    mut external_printer: impl ExternalPrinter + Send + Sync + 'static,
) {
    std::thread::spawn(move || {
        while let Ok(line) = print_rx.recv() {
            external_printer.print(line).expect("Failed to print line");
        }
    });
}
