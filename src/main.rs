use clap::{Parser, Subcommand};
use std::process::ExitCode;

mod commands;
mod config;
mod env_loader;
mod env_subst;
mod lock;
mod ssh;

#[derive(Parser)]
#[command(name = "bridge")]
#[command(about = "Remote development tool for syncing code and running commands")]
#[command(version)]
struct Cli {
    /// Override default host
    #[arg(long, global = true)]
    host: Option<String>,

    /// Detailed output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Preview without executing
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync current directory to remote
    Sync {
        /// Disable auto-exclusion of Mac-specific files (.DS_Store, ._*)
        #[arg(long)]
        no_auto_exclude: bool,

        /// Delete excluded files from remote (rsync only)
        #[arg(long)]
        delete_excluded: bool,
    },

    /// Run command on remote
    Run {
        /// Command to execute
        command: String,

        /// Sync before running
        #[arg(short, long)]
        sync: bool,

        /// Command to run after reconnecting from unexpected SSH disconnect (overrides config)
        #[arg(long)]
        reconnect_command: Option<String>,

        /// Seconds to wait for reconnection (overrides config, default: 90)
        #[arg(long)]
        reconnect_timeout: Option<u64>,

        /// Acquire exclusive lock before running (optional lock name)
        #[arg(long, num_args = 0..=1, default_missing_value = "default")]
        lock: Option<String>,

        /// Seconds to wait for lock (default: 600)
        #[arg(long)]
        lock_timeout: Option<u64>,
    },

    /// Upload single file to remote
    Upload {
        /// File to upload
        file: String,

        /// Remote destination filename
        #[arg(long)]
        dest: Option<String>,
    },

    /// Download file from remote
    Download {
        /// File to download
        file: String,

        /// Local destination path
        #[arg(long)]
        dest: Option<String>,
    },

    /// Create bridge.toml in current directory
    Init,

    /// List configured hosts
    Hosts,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Sync { no_auto_exclude, delete_excluded } => {
            commands::sync::run(cli.host.as_deref(), no_auto_exclude, delete_excluded, cli.dry_run, cli.verbose)
        }
        Commands::Run { command, sync, reconnect_command, reconnect_timeout, lock, lock_timeout } => {
            match commands::run::run(cli.host.as_deref(), &command, sync, cli.dry_run, cli.verbose, reconnect_command.as_deref(), reconnect_timeout, lock, lock_timeout) {
                Ok(exit_code) => {
                    return ExitCode::from(exit_code.min(255) as u8);
                }
                Err(e) => Err(e),
            }
        }
        Commands::Upload { file, dest } => commands::upload::run(
            &file,
            dest.as_deref(),
            cli.host.as_deref(),
            cli.dry_run,
            cli.verbose,
        ),
        Commands::Download { file, dest } => commands::download::run(
            &file,
            dest.as_deref(),
            cli.host.as_deref(),
            cli.dry_run,
            cli.verbose,
        ),
        Commands::Init => commands::init::run(cli.verbose),
        Commands::Hosts => commands::hosts::run(cli.verbose),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {:#}", e);
            ExitCode::FAILURE
        }
    }
}
