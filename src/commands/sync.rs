use anyhow::{Context, Result};

use crate::config::{self, Config, SyncMethod};
use crate::ssh;

pub fn run(host: Option<&str>, no_auto_exclude: bool, delete_excluded: bool, dry_run: bool, verbose: bool) -> Result<()> {
    let (config, config_path) = Config::find_and_load()?;
    let project_root = Config::project_root(&config_path);

    let (host_name, host) = config.get_host(host)?;

    // Merge auto-excludes with config excludes (unless --no-auto-exclude)
    let excludes = if no_auto_exclude {
        config.sync.exclude.clone()
    } else {
        let mut excludes = config::auto_excludes();
        excludes.extend(config.sync.exclude.clone());
        excludes
    };

    if verbose {
        eprintln!("Project root: {}", project_root.display());
        eprintln!("Syncing to host: {} ({})", host_name, host.hostname);
        eprintln!("Remote path: {}", host.path);
        eprintln!("Sync method: {:?}", host.sync_method);
        eprintln!("Excludes: {:?}", excludes);
    }

    // Ensure remote directory exists (skip in dry-run, rsync creates it automatically)
    if !dry_run && host.sync_method == SyncMethod::Tar {
        ssh::ensure_remote_dir(&host.hostname, &host.path, &host.shell, verbose)?;
    }

    let source = project_root.to_str().context("Invalid project path")?;

    match host.sync_method {
        SyncMethod::Tar => {
            ssh::sync_to_remote(
                source,
                &host.hostname,
                &host.path,
                &excludes,
                &host.shell,
                dry_run,
                verbose,
            )?;
        }
        SyncMethod::Rsync => {
            ssh::rsync_to_remote(
                source,
                &host.hostname,
                &host.path,
                &excludes,
                &host.shell,
                delete_excluded,
                dry_run,
                verbose,
            )?;
        }
    }

    if !dry_run {
        println!("Sync complete.");
    }

    Ok(())
}
