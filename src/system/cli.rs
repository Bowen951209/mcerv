use crate::{
    network::{fabric_meta, forge_meta, modrinth::SearchIndex, vanilla_meta},
    system::forks::{FetchCommand, InstallCommand},
    *,
};
use clap::{ArgAction, Args, Parser, Subcommand};
use reqwest::Client;

pub trait Versions {
    type V;
    async fn versions(&self, client: &Client) -> anyhow::Result<Self::V>;
}

pub trait FetchFilter {}

#[derive(Args, Debug)]
pub struct VersionsFilter {
    /// List all versions, stable and unstable.
    #[arg(long, action = ArgAction::SetTrue, default_value_t = false)]
    pub all: bool,
}

impl FetchFilter for VersionsFilter {}

#[derive(Args, Debug)]
pub struct YesArgs {
    #[arg(short, long, action = ArgAction::SetTrue, default_value_t = false)]
    pub yes: bool,
}

/// Shared vanilla version arguments for Install and UpdateServerJar
#[derive(Parser, Debug)]
pub struct VanillaVersionArgs {
    /// Use the latest stable game version
    #[arg(long, action = ArgAction::SetTrue, default_value_t = false, conflicts_with = "version")]
    pub latest_stable: bool,

    /// Minecraft game version
    #[arg(
        required_unless_present = "latest_stable",
        conflicts_with = "latest_stable"
    )]
    pub version: Option<String>,
}

impl Versions for VanillaVersionArgs {
    type V = String;
    async fn versions(&self, client: &Client) -> anyhow::Result<Self::V> {
        let version = if self.latest_stable {
            vanilla_meta::fetch_latest_stable_version(client).await?
        } else {
            self.version.clone().unwrap()
        };

        Ok(version)
    }
}

/// Shared fabric version arguments for Install and UpdateServerJar
#[derive(Parser, Debug)]
pub struct FabricVersionArgs {
    /// Set the unset versions to latest stable
    #[arg(long,action = ArgAction::SetTrue,default_value_t = false)]
    pub latest_stable: bool,

    /// Minecraft game version
    #[arg(required_unless_present = "latest_stable")]
    pub game_version: Option<String>,

    /// Fabric loader version
    #[arg(required_unless_present = "latest_stable")]
    pub loader_version: Option<String>,

    /// Fabric installer version
    #[arg(required_unless_present = "latest_stable")]
    pub installer_version: Option<String>,
}

impl Versions for FabricVersionArgs {
    type V = (String, String, String);
    async fn versions(&self, client: &Client) -> anyhow::Result<Self::V> {
        let versions = if self.latest_stable {
            let (game_version, loader_version, installer_version) =
                fabric_meta::fetch_latest_stable_versions(client).await?;
            (
                self.game_version.clone().unwrap_or(game_version),
                self.loader_version.clone().unwrap_or(loader_version),
                self.installer_version.clone().unwrap_or(installer_version),
            )
        } else {
            (
                self.game_version.clone().unwrap(),
                self.loader_version.clone().unwrap(),
                self.installer_version.clone().unwrap(),
            )
        };

        Ok(versions)
    }
}

/// Shared forge version arguments for Install and UpdateServerJar
#[derive(Parser, Debug)]
pub struct ForgeVersionArgs {
    /// Use the latest forge installer version. It's also the latest game version.
    #[arg(long, action = ArgAction::SetTrue, default_value_t = false, conflicts_with = "version")]
    pub latest: bool,

    /// Forge installer version. For example: `1.21.8-58.1.1`.
    #[arg(required_unless_present = "latest", conflicts_with = "latest")]
    pub version: Option<String>,
}

impl Versions for ForgeVersionArgs {
    type V = String;
    async fn versions(&self, client: &Client) -> anyhow::Result<Self::V> {
        let version = if self.latest {
            forge_meta::fetch_latest_version(client).await?
        } else {
            self.version.clone().unwrap()
        };

        Ok(version)
    }
}

#[derive(Parser)]
#[command(name = "mcerv")]
#[command(about = "A Minecraft server instance manager.")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List the installed servers
    LsServers,
    /// List the mods in the target server and check for updates
    LsMods {
        server_name: String,
        #[command(flatten)]
        want_update: YesArgs,
    },
    /// Get the versions of the mod
    FetchModVersions {
        name: String,
        /// List only featured versions
        #[arg(long, action = ArgAction::SetTrue, default_value_t = false)]
        featured: bool,
    },
    /// List availible versions for the target Minecraft server fork
    Fetch {
        #[command(subcommand)]
        command: FetchCommand,
    },
    /// Search for a mod with the given name
    SearchMod {
        name: String,
        /// Example: `open_source`, `license:mit`.
        ///
        /// See https://docs.modrinth.com/api/operations/searchprojects for details.
        ///
        /// Note: `mcerv` automatically adds `server_side:required` & `server_side:optional`.
        #[arg(long, num_args = 0..)]
        facets: Vec<String>,
        /// The sorting method used for sorting search results
        #[arg(long)]
        index: Option<SearchIndex>,
        /// The number of results returned by the search
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Set the max/min memory, or JAVA_HOME of the target server
    Set {
        server_name: String,
        #[arg(long)]
        max_memory: Option<String>,
        #[arg(long)]
        min_memory: Option<String>,
        #[arg(long)]
        java_home: Option<String>,
    },
    /// Install the server with the given versions
    Install {
        #[command(subcommand)]
        command: InstallCommand,
        server_name: String,
        #[command(flatten)]
        accept_eula: YesArgs,
    },
    /// Install a mod to the target server
    InstallMod {
        server_name: String,
        /// The mod version ID in the form of "IIJJKKLL"
        mod_id: String,
    },
    /// Generate a start script for the target server
    GenStartScript { server_name: String },
    /// Replace the server jar with the specified version
    UpdateServerJar {
        server_name: String,
        /// Version arguments specific to the server fork
        #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
        version_args: Vec<String>, // This will be parsed at runtime depending on the server fork
    },
    /// Accept the EULA for the target server. This will create or modify the eula.txt file
    AcceptEula { server_name: String },
    /// Start the target server
    Start,
    /// Show the info of the target server
    Info { server_name: String },
}

impl Command {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Command::LsServers => list_servers(),
            Command::LsMods {
                server_name,
                want_update,
            } => {
                list_mods(&server_name, want_update.yes, &Client::new()).await?;
            }
            Command::FetchModVersions { name, featured } => {
                fetch_mod_versions(&name, featured, &Client::new()).await?;
            }
            Command::Fetch { command } => {
                let s = match command {
                    FetchCommand::Vanilla { filter } => {
                        forks::Vanilla::fetch_availables(filter.all, &Client::new()).await?
                    }
                    FetchCommand::Fabric { filter } => {
                        forks::Fabric::fetch_availables(filter.all, &Client::new()).await?
                    }
                    FetchCommand::Forge {} => {
                        forks::Forge::fetch_availables((), &Client::new()).await?
                    }
                };
                println!("{s}");
            }
            Command::SearchMod {
                name,
                facets,
                index,
                limit,
            } => search_mod(&name, &facets, index, limit, &Client::new()).await?,
            Command::Set {
                server_name,
                max_memory,
                min_memory,
                java_home,
            } => set_config(&server_name, max_memory, min_memory, java_home)?,
            Command::Install {
                command,
                server_name,
                accept_eula,
            } => install(command, &server_name, accept_eula.yes, &Client::new()).await?,
            Command::InstallMod {
                server_name,
                mod_id,
            } => install_mod(&server_name, &mod_id, &Client::new()).await?,
            Command::GenStartScript { server_name } => generate_start_script(&server_name)?,
            Command::UpdateServerJar {
                server_name,
                version_args,
            } => {
                update_server_jar(&version_args, &server_name, &Client::new()).await?;
            }
            Command::AcceptEula { server_name } => generate_eula_accept_file(&server_name)?,
            Command::Start => todo!(),
            Command::Info { server_name } => show_server_info(&server_name)?,
        }

        Ok(())
    }
}
