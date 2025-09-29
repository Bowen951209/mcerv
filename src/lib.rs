mod network;
mod system;

use crate::{
    network::{
        fabric_meta::{self, PrintVersionMode},
        modrinth::{self, SearchIndex},
    },
    system::{
        cli::{Cli, Commands, FetchCommands, InstallCommands, Versions},
        config::Config,
        forks::{self, Fork, ServerFork},
        jar_parser,
        server_info::ServerInfo,
    },
};
use clap::Parser;
use dialoguer::Confirm;
use directories::ProjectDirs;
use reqwest::Client;
use std::{error::Error, fmt::Display, fs, io::Write, path::PathBuf, time::Instant};

#[derive(Debug)]
pub enum DirectoryError {
    ServerDirDoesNotExist(PathBuf),
    ModsDirDoesNotExist(PathBuf),
}

impl Display for DirectoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirectoryError::ServerDirDoesNotExist(path) => {
                write!(f, "Server directory does not exist: {:?}", path)
            }
            DirectoryError::ModsDirDoesNotExist(path) => {
                write!(f, "Mods directory does not exist: {:?}", path)
            }
        }
    }
}

impl Error for DirectoryError {}

pub async fn run() -> anyhow::Result<()> {
    fs::create_dir_all(instances_dir()).expect("Unable to create instances directory");

    match Cli::parse().command {
        Commands::LsServers => list_servers(),
        Commands::LsMods {
            server_name,
            want_update,
        } => {
            list_mods(&server_name, want_update.yes, &Client::new()).await?;
        }
        Commands::FetchModVersions { name, featured } => {
            fetch_mod_versions(&name, featured, &Client::new()).await?;
        }
        Commands::Fetch { command } => {
            let s = match command {
                FetchCommands::Fabric {
                    all,
                    stable_only: _,
                } => forks::Fabric::fetch_availables(all, &Client::new()).await?,
                FetchCommands::Forge {} => {
                    forks::Forge::fetch_availables((), &Client::new()).await?
                }
            };
            println!("{s}");
        }
        Commands::SearchMod {
            name,
            facets,
            index,
            limit,
        } => search_mod(&name, &facets, index, limit, &Client::new()).await?,
        Commands::Set {
            server_name,
            max_memory,
            min_memory,
            java_home,
        } => set_config(&server_name, max_memory, min_memory, java_home)?,
        Commands::Install {
            command,
            server_name,
            accept_eula,
        } => install(command, &server_name, accept_eula.yes, &Client::new()).await?,
        Commands::InstallMod {
            server_name,
            mod_id,
        } => install_mod(&server_name, &mod_id, &Client::new()).await?,
        Commands::GenStartScript { server_name } => generate_start_script(&server_name)?,
        Commands::UpdateServerJar {
            command,
            server_name,
        } => {
            update_server_jar(command, &server_name, &Client::new()).await?;
        }
        Commands::AcceptEula { server_name } => generate_eula_accept_file(&server_name)?,
        Commands::Start => todo!(),
        Commands::Info { server_name } => show_server_info(&server_name)?,
    }

    Ok(())
}

/// List the directories in the instances directory
pub fn list_servers() {
    let instances_dir = instances_dir();
    let mut entries = std::fs::read_dir(&instances_dir)
        .expect("Unable to read instances directory")
        .peekable();

    // If there are no entries, print a message
    if entries.peek().is_none() {
        println!("No servers found.");
    }

    for entry in entries {
        let entry = entry.expect("Unable to read entry");
        if entry.path().is_dir() {
            println!("{}", entry.file_name().to_string_lossy());
        }
    }
}

/// Lists the installed mods in the target server's mods directory.
/// Also checks for updates on Modrinth.
/// If updates are available, prompts the user to confirm updating.
/// If the instance is vanilla and has no mods directory, displays a message to inform the user.
pub async fn list_mods(server_name: &str, update_arg: bool, client: &Client) -> anyhow::Result<()> {
    // Check if the server is vanilla
    let server_jar_name = Config::load_or_create(server_name)?.jar_name;
    let server_jar_path = server_dir(server_name).join(&server_jar_name);
    let mut archive = jar_parser::archive(server_jar_path)?;
    if matches!(
        forks::detect_server_fork(&mut archive)?,
        ServerFork::Vanilla
    ) {
        println!("{server_name} is a vanilla server and should not have any mods installed.");
        return Ok(());
    }

    // Process mods
    let mods_dir = try_mods_dir(server_name)?;

    let jar_paths = fs::read_dir(&mods_dir)?
        .map(|entry| entry.expect("Failed to read entry").path())
        .filter(|path| path.extension().expect("Failed to get extension") == "jar")
        .collect::<Vec<_>>();

    let mut jar_files = jar_paths
        .iter()
        .map(fs::File::open)
        .collect::<Result<Vec<_>, _>>()?;

    let jar_hashes = jar_files
        .iter_mut()
        .map(jar_parser::calculate_hash)
        .collect::<Result<Vec<_>, _>>()?;

    let server_info = ServerInfo::new(server_name)?;
    let game_versions = [server_info.game_version.as_str()];

    let (latest_versions_res, old_versions_res) = tokio::join!(
        modrinth::get_latest_versions(client, &jar_hashes, &game_versions),
        modrinth::get_versions(client, &jar_hashes)
    );

    let latest_versions = latest_versions_res?;
    let old_versions = old_versions_res?;

    let slug_map =
        modrinth::get_project_slug_map(client, old_versions.iter().map(|v| v.project_id.as_str()))
            .await?;

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

    if available_updates.is_empty() {
        return Ok(());
    }

    let should_update = update_arg
        || Confirm::new()
            .with_prompt("Do you want to update the mods?")
            .interact()?;

    if !should_update {
        return Ok(());
    }

    println!("Updating mods...");

    let downloads = available_updates.iter().map(|(_, version)| {
        let url = version.file_url.clone();
        let save_path = mods_dir.join(version.file_name.clone());
        (url, save_path)
    });

    network::download_files(client, downloads).await?;

    // Delete old jar files
    for (jar_path, _) in &available_updates {
        if let Err(e) = fs::remove_file(jar_path) {
            // Do not return error here, because we want to delete the rest.
            eprintln!("Failed to delete old jar file: {e}");
        }
    }

    println!("Mods updated successfully.");

    Ok(())
}

pub async fn fetch_mod_versions(
    project_slug: &str,
    featured: bool,
    client: &Client,
) -> anyhow::Result<()> {
    let response = modrinth::get_project_versions(client, project_slug, featured).await?;

    println!("{response}");
    Ok(())
}

pub async fn search_mod(
    name: &str,
    facets: &[String],
    index: Option<SearchIndex>,
    limit: Option<usize>,
    client: &Client,
) -> anyhow::Result<()> {
    let facets = facets.iter().map(|f| f.as_str()).collect::<Vec<_>>();
    let response = modrinth::search(client, name, &facets, index, limit).await?;
    println!("{response}");

    Ok(())
}

pub fn set_config(
    server_name: &str,
    max_mem: Option<String>,
    min_mem: Option<String>,
    java_home: Option<String>,
) -> anyhow::Result<()> {
    let mut config = Config::load_or_create(server_name)?;

    if let Some(max_mem) = max_mem {
        config.max_memory = max_mem;
    }

    if let Some(min_mem) = min_mem {
        config.min_memory = min_mem;
    }

    if let Some(java_home) = java_home {
        config.java_home = Some(java_home.to_string());
    }

    config.save(server_name)?;

    Ok(())
}

pub async fn install(
    command: InstallCommands,
    server_name: &str,
    accept_eula: bool,
    client: &Client,
) -> anyhow::Result<()> {
    let eula_agreed = accept_eula || Confirm::new()
                .with_prompt("Do you agree to Minecraft server EULA? Please ensure you have read and understood the EULA at: https://aka.ms/MinecraftEULA")
                .interact()
                .unwrap_or(false);

    let server_dir = server_dir(server_name);
    fs::create_dir_all(&server_dir)?;

    if eula_agreed {
        generate_eula_accept_file(server_name)?;
    }

    let start = Instant::now();
    let filename = install_from_command(server_name, command, client).await?;
    println!("Download complete. Duration: {:?}", start.elapsed());

    let config = Config::new_4gb(filename)?;
    config.save(server_name)?;
    println!("Config created and saved");
    println!("Server added: {server_name}");
    Ok(())
}

pub async fn install_mod(
    server_name: &str,
    version_id: &str,
    client: &Client,
) -> anyhow::Result<()> {
    println!("Downloading mod version {version_id}...");
    let mods_dir = mods_dir(server_name);
    fs::create_dir_all(&mods_dir)?;
    let file_name = modrinth::download_version(client, version_id, mods_dir).await?;
    println!("Mod version downloaded: {file_name}");

    Ok(())
}

pub fn generate_start_script(server_name: &str) -> anyhow::Result<()> {
    let start_script = Config::load_or_create(server_name)?.create_start_script();

    let filename = if cfg!(target_os = "windows") {
        "start_script.bat"
    } else {
        "start_script.sh"
    };

    let path = try_server_dir(server_name)?.join(filename);
    let mut file = fs::File::create(&path)?;
    file.write_all(start_script.as_bytes())?;

    Ok(())
}

pub fn generate_eula_accept_file(server_name: &str) -> anyhow::Result<()> {
    let eula_path = try_server_dir(server_name)?.join("eula.txt");

    fs::create_dir_all(eula_path.parent().unwrap())?;

    let content = "# This file is generated by mcerv and is generated because the user agreed to the Minecraft EULA (https://aka.ms/MinecraftEULA).\neula=true";

    fs::write(&eula_path, content)?;

    Ok(())
}

pub fn show_server_info(server_name: &str) -> anyhow::Result<()> {
    let config = Config::load_or_create(server_name)?;
    let jar_path = server_dir(server_name).join(&config.jar_name);
    let server_info = ServerInfo::new(jar_path)?;
    println!("{config}{server_info}");
    Ok(())
}

pub async fn update_server_jar(
    command: InstallCommands,
    server_name: &str,
    client: &Client,
) -> anyhow::Result<()> {
    println!("Updating server jar...");
    let start = Instant::now();

    // Find the old jar name before downloading the new one
    // to prevent multiple jars existing at once
    let server_dir = try_server_dir(server_name)?;
    let mut config = Config::load_or_create(server_name)?;
    let old_jar_name = config.jar_name;
    let old_jar_path = server_dir.join(old_jar_name);

    let filename = install_from_command(server_name, command, client).await?;

    println!("Deleting old server jar...");
    fs::remove_file(&old_jar_path)?;

    println!("Updating config...");
    config.jar_name = filename;

    config.save(server_name)?;

    println!("Update complete in {:?}", start.elapsed());

    Ok(())
}

pub fn try_mods_dir(server_name: &str) -> Result<PathBuf, DirectoryError> {
    let dir = mods_dir(server_name);

    if !dir.exists() {
        return Err(DirectoryError::ModsDirDoesNotExist(dir));
    }

    Ok(dir)
}

pub fn try_server_dir(server_name: &str) -> Result<PathBuf, DirectoryError> {
    let dir = server_dir(server_name);

    if !dir.exists() {
        return Err(DirectoryError::ServerDirDoesNotExist(dir));
    }

    Ok(dir)
}

pub fn mods_dir(server_name: &str) -> PathBuf {
    server_dir(server_name).join("mods")
}

pub fn server_dir(server_name: &str) -> PathBuf {
    instances_dir().join(server_name)
}

pub fn instances_dir() -> PathBuf {
    proj_dirs().data_dir().join("instances")
}

pub fn proj_dirs() -> ProjectDirs {
    ProjectDirs::from("", "", "mcerv").expect("Unable to determine project directory")
}

async fn install_from_command(
    server_name: &str,
    command: InstallCommands,
    client: &Client,
) -> anyhow::Result<String> {
    match command {
        InstallCommands::Fabric { version_args } => {
            println!("Fetching versions...");
            let versions = version_args.versions(client).await?;
            println!("Downloading server jar...");
            forks::Fabric::install(server_name, versions, client).await
        }
        InstallCommands::Forge { version_args } => {
            println!("Fetching versions...");
            let versions = version_args.versions(client).await?;
            println!("Installing server jar...");
            forks::Forge::install(server_name, versions, client).await
        }
    }
}
