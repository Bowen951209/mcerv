use crate::{
    network::{
        PrintVersionMode,
        fabric_meta::{self},
        forge_meta, vanilla_meta,
    },
    server_dir,
    system::cli,
    system::jar_parser,
};
use anyhow::anyhow;
use clap::{Subcommand, command};
use reqwest::Client;
use std::{
    error::Error,
    fmt::Display,
    io::{Read, Seek},
    process::Command,
};
use zip::ZipArchive;

/// This macro defines server forks by:
/// 1. Creating [`ServerFork`] enum for matching convenience.
/// 2. Creating empty structs for [`Fork`] implementations.
/// 3. Creating [`detect_fork_from_main_class`] function.
/// 4. Creating [`FetchCommands`] and [`InstallCommands`] enums for CLI.
///
/// # Usage
///
/// ```
/// __define_forks!(
///    ForkName1 => ( InstallArgsType1, FetchFilterType1 ),
///    ForkName2 => ( InstallArgsType2, FetchFilterType2 ),
///    ForkName3 => ( InstallArgsType3 ),
/// )
/// ```
///
/// Install argument should be in first place, and fetch filter should be in second.
/// Intsall argument should implement [`cli::Versions`], and fetch filter should implement [`cli::FetchFilter`].
/// If a fork doesn't have fetch filter, you can simply not provide it. But notice that you still
/// have to put install arguments in parantheses.
macro_rules! __define_forks {
    (
        $(
            $variant:ident => ( $install_args:ty $(,$fetch_filter:ty)? ) ),*
        $(,)?
    ) => {
        #[derive(Debug, Clone, Copy)]
        pub enum ServerFork {
            $($variant),*
        }

        $(
            pub struct $variant;
        )*

        fn detect_fork_from_main_class(
            main_class: &str
        ) -> anyhow::Result<ServerFork> {
            $(
                if $variant::is_this_fork(main_class) {
                    return Ok(ServerFork::$variant);
                }
            )*

            anyhow::bail!(DetectServerInfoError::UnknownServerFork);
        }

        #[derive(Subcommand)]
        pub enum InstallCommands {
            $(
                $variant {
                    #[command(flatten)]
                    version_args: $install_args,
                },
            )*
        }

        #[derive(Subcommand)]
        pub enum FetchCommands {
            $(
                $variant {
                    $(
                        #[command(flatten)]
                        filter: $fetch_filter,
                    )?
                },
            )*
        }

        // Assert trait implementations by creating and calling anonymous empty generics functions
        $(
            const _: () = {
                // Check 1: $install_args must implement cli::Versions
                const fn assert_is_versions<T: cli::Versions>() {}
                assert_is_versions::<$install_args>();

                // Check 2: If there is $fetch_filter, it must implement cli::FetchFilter
                $(
                    const fn assert_is_filter<T: cli::FetchFilter>() {}
                    assert_is_filter::<$fetch_filter>();
                )?
            };
        )*
    };
}

macro_rules! define_forks {
    // Rule A: Termination - when input queue is empty
    (@parse outputs=[$($output:tt)*] input=[$(,)?]) => {
        // Pass all accumulated outputs to the backend macro
        __define_forks!( $($output)* );
    };

    // Rule B: Special case - encounter `(Args, ())`
    (@parse
        outputs=[$($output:tt)*]
        // Match `()` literally, instead as :ty
        input=[ $name:ident => ($args:ty, ()), $($rest:tt)* ]
    ) => {
        define_forks!(
            @parse
            // Discard the `()`, keep only ($args)
            outputs=[ $($output)* $name => ($args), ]
            input=[ $($rest)* ]
        );
    };

    // Rule C: General case - encounter `(Args, Filter)`
    (@parse
        outputs=[$($output:tt)*]
        input=[ $name:ident => ($args:ty, $filter:ty), $($rest:tt)* ]
    ) => {
        define_forks!(
            @parse
            outputs=[ $($output)* $name => ($args, $filter), ]
            input=[ $($rest)* ]
        );
    };

    // Entry point
    ($($input:tt)*) => {
        // Initialize the state machine: outputs is empty, input is user input
        define_forks!(@parse outputs=[] input=[$($input)*,]);
    };
}

define_forks!(
    Vanilla => (cli::VanillaVersionArgs, cli::VersionsFilter),
    Fabric => (cli::FabricVersionArgs, cli::VersionsFilter),
    Forge => (cli::ForgeVersionArgs, ()),
);

#[derive(Debug, Clone)]
pub enum DetectServerInfoError {
    MainClassNotFound,
    UnknownServerFork,
    GameVersionNotFound,
}

impl Display for DetectServerInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectServerInfoError::MainClassNotFound => {
                write!(f, "Main-Class not found in MANIFEST.MF")
            }
            DetectServerInfoError::UnknownServerFork => {
                write!(
                    f,
                    "Detected an unknown server fork. Probably not supported by mcerv"
                )
            }
            DetectServerInfoError::GameVersionNotFound => {
                write!(f, "Game version not found in install.properties")
            }
        }
    }
}

impl Error for DetectServerInfoError {}

pub trait Fork {
    type FetchConfig;
    type Version;

    fn is_this_fork(main_class: &str) -> bool;

    fn game_version<R: Read + Seek>(archive: &mut ZipArchive<R>) -> anyhow::Result<String>;

    async fn install(
        server_name: &str,
        version: Self::Version,
        client: &Client,
    ) -> anyhow::Result<String>;

    async fn fetch_availables(config: Self::FetchConfig, client: &Client)
    -> anyhow::Result<String>;
}

impl Fork for Vanilla {
    type FetchConfig = bool;
    type Version = String;

    fn is_this_fork(main_class: &str) -> bool {
        main_class.contains("net.minecraft.")
    }

    fn game_version<R: Read + Seek>(archive: &mut ZipArchive<R>) -> anyhow::Result<String> {
        // Game version property is stored in `version.json`
        let content = jar_parser::read_file(archive, "version.json")?;
        let v: serde_json::Value = serde_json::from_str(&content)?;
        let name = v
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;

        Ok(name.to_string())
    }

    async fn install(
        server_name: &str,
        version: Self::Version,
        client: &Client,
    ) -> anyhow::Result<String> {
        let server_dir = server_dir(server_name);
        vanilla_meta::download_server(client, &version, &server_dir).await
    }

    async fn fetch_availables(all: bool, client: &Client) -> anyhow::Result<String> {
        let mode = PrintVersionMode::from_all_flag(all);
        vanilla_meta::versions(client, mode).await
    }
}

impl Fork for Fabric {
    type FetchConfig = bool;
    type Version = (String, String, String); // (game_version, loader_version, installer_version)

    fn is_this_fork(main_class: &str) -> bool {
        main_class.contains("net.fabricmc.")
    }

    fn game_version<R: Read + Seek>(archive: &mut ZipArchive<R>) -> anyhow::Result<String> {
        // Game version property is stored in `install.properties`
        let content = jar_parser::read_file(archive, "install.properties")?;
        let mut install_properties = jar_parser::parse_properties(&content);

        let version = install_properties
            .remove("game-version") // Use remove to get owned String
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;

        Ok(version)
    }

    async fn install(
        server_name: &str,
        version: Self::Version,
        client: &Client,
    ) -> anyhow::Result<String> {
        let server_dir = server_dir(server_name);
        fabric_meta::download_server(client, &version.0, &version.1, &version.2, &server_dir).await
    }

    async fn fetch_availables(all: bool, client: &Client) -> anyhow::Result<String> {
        let mode = PrintVersionMode::from_all_flag(all);
        fabric_meta::versions(client, mode).await
    }
}

impl Fork for Forge {
    type FetchConfig = ();
    type Version = String;

    fn is_this_fork(main_class: &str) -> bool {
        main_class.contains("net.minecraftforge.")
    }

    fn game_version<R: Read + Seek>(archive: &mut ZipArchive<R>) -> anyhow::Result<String> {
        // Game version property is stored in `bootstrap-shim.list`
        // The line format goes like:
        // HASH net.minecraftforge:forge:1.21.8-58.1.0:server net/minecraftforge/forge/1.21.8-58.1.0/forge-1.21.8-58.1.0-server.jar

        let content = jar_parser::read_file(archive, "bootstrap-shim.list")?;
        let line = content
            .lines()
            .find(|line| line.contains("net.minecraftforge:forge:") && line.contains(":server"))
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;
        let long_version = line
            .split(':')
            .nth(2)
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;
        let game_version = long_version
            .split('-')
            .next()
            .ok_or(anyhow!(DetectServerInfoError::GameVersionNotFound))?;

        Ok(game_version.to_string())
    }

    async fn install(
        server_name: &str,
        version: Self::Version,
        client: &Client,
    ) -> anyhow::Result<String> {
        let server_dir = server_dir(server_name);
        let installer_name = forge_meta::download_installer(client, &version, &server_dir).await?;

        let status = Command::new("java")
            .arg("-jar")
            .arg(&installer_name)
            .arg("--installServer")
            .current_dir(&server_dir)
            .status()
            .expect("Failed to execute Forge installer");

        if !status.success() {
            anyhow::bail!("Forge installer failed with status: {:?}", status);
        }

        // Delete the installer jar
        std::fs::remove_file(server_dir.join(installer_name))?;

        // Delete default start scripts generated by Forge installer
        // See https://github.com/Bowen951209/mcerv/issues/19#issuecomment-3268600074
        std::fs::remove_file(server_dir.join("run.bat"))?;
        std::fs::remove_file(server_dir.join("run.sh"))?;
        std::fs::remove_file(server_dir.join("user_jvm_args.txt"))?;

        println!("Removed installer stuff");

        // Return the server jar file name
        Ok(format!("forge-{version}-shim.jar"))
    }

    async fn fetch_availables(_config: (), client: &Client) -> anyhow::Result<String> {
        forge_meta::versions(client).await
    }
}

pub fn detect_server_fork<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> anyhow::Result<ServerFork> {
    let content = jar_parser::read_file(archive, "META-INF/MANIFEST.MF")?;
    let manifest = jar_parser::parse_manifest(&content);
    let main_class = manifest
        .get("Main-Class")
        .ok_or(anyhow!(DetectServerInfoError::MainClassNotFound))?;

    detect_fork_from_main_class(main_class)
}

pub fn detect_game_version<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    fork: ServerFork,
) -> anyhow::Result<String> {
    match fork {
        ServerFork::Fabric => Fabric::game_version(archive),
        ServerFork::Forge => Forge::game_version(archive),
        ServerFork::Vanilla => Vanilla::game_version(archive),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::jar_parser::archive;

    #[test]
    fn test_detect_vanilla_fork() {
        let jar_path = "testdata/vanilla-1.21.8.jar";
        let mut archive = archive(jar_path).unwrap();
        let fork = detect_server_fork(&mut archive).unwrap();

        assert!(matches!(fork, ServerFork::Vanilla))
    }

    #[test]
    fn test_detect_fabric_fork() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let mut archive = archive(jar_path).unwrap();
        let fork = detect_server_fork(&mut archive).unwrap();

        assert!(matches!(fork, ServerFork::Fabric));
    }

    #[test]
    fn test_detect_forge_fork() {
        let jar_path = "testdata/forge-1.21.8-58.1.0-shim.jar";
        let mut archive = archive(jar_path).unwrap();
        let fork = detect_server_fork(&mut archive).unwrap();

        assert!(matches!(fork, ServerFork::Forge));
    }

    #[test]
    fn test_detect_game_version_vanilla() {
        let jar_path = "testdata/vanilla-1.21.8.jar";
        let mut archive = archive(jar_path).unwrap();
        let version = detect_game_version(&mut archive, ServerFork::Vanilla).unwrap();

        assert_eq!(version, "1.21.8")
    }

    #[test]
    fn test_detect_game_version_fabric() {
        let jar_path = "testdata/fabric-server-mc.1.21.8-loader.0.16.14-launcher.1.0.3.jar";
        let mut archive = archive(jar_path).unwrap();
        let version = detect_game_version(&mut archive, ServerFork::Fabric).unwrap();

        assert_eq!(version, "1.21.8")
    }

    #[test]
    fn test_detect_game_version_forge() {
        let jar_path = "testdata/forge-1.21.8-58.1.0-shim.jar";
        let mut archive = archive(jar_path).unwrap();
        let version = detect_game_version(&mut archive, ServerFork::Forge).unwrap();

        assert_eq!(version, "1.21.8")
    }
}
