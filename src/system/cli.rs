use crate::{
    network::{fabric_meta, forge_meta, modrinth::SearchIndex, vanilla_meta},
    system::forks::{FetchCommands, InstallCommands},
};
use clap::{ArgAction, Args, Parser, Subcommand, command};
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
#[derive(Args, Debug)]
pub struct VanillaVersionArgs {
    /// Use the latest stable versions (no need to specify versions)
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
#[derive(Args, Debug)]
pub struct FabricVersionArgs {
    /// Use the latest stable versions (no need to specify versions)
    #[arg(long, action = ArgAction::SetTrue, default_value_t = false, conflicts_with_all = ["game_version", "loader_version", "installer_version"])]
    pub latest_stable: bool,

    /// Minecraft game version
    #[arg(
        required_unless_present = "latest_stable",
        conflicts_with = "latest_stable"
    )]
    pub game_version: Option<String>,

    /// Fabric loader version
    #[arg(
        required_unless_present = "latest_stable",
        conflicts_with = "latest_stable"
    )]
    pub loader_version: Option<String>,

    /// Fabric installer version
    #[arg(
        required_unless_present = "latest_stable",
        conflicts_with = "latest_stable"
    )]
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
#[derive(Args, Debug)]
pub struct ForgeVersionArgs {
    /// Use the latest versions (no need to specify versions)
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
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
        command: FetchCommands,
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
        command: InstallCommands,
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
        #[command(subcommand)]
        command: InstallCommands,
        server_name: String,
    },
    /// Accept the EULA for the target server. This will create or modify the eula.txt file
    AcceptEula { server_name: String },
    /// Start the target server
    Start,
    /// Show the info of the target server
    Info { server_name: String },
}
