use anyhow::{Context, Result};
use std::path::Path;

use crate::config::Config;
use crate::ssh;

pub fn run(
    file: &str,
    dest: Option<&str>,
    host: Option<&str>,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let (config, _config_path) = Config::find_and_load()?;
    let (host_name, host_config) = config.get_host(host)?;

    // Resolve local file path
    let local_path = if Path::new(file).is_absolute() {
        Path::new(file).to_path_buf()
    } else {
        std::env::current_dir()?.join(file)
    };

    if !local_path.exists() && !dry_run {
        anyhow::bail!("Local file does not exist: {}", local_path.display());
    }

    // Determine remote destination
    let remote_filename = dest.unwrap_or_else(|| {
        local_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file)
    });

    let remote_path = format!("{}/{}", host_config.path, remote_filename);

    if verbose {
        eprintln!("Uploading to host: {} ({})", host_name, host_config.hostname);
        eprintln!("Local file: {}", local_path.display());
        eprintln!("Remote path: {}", remote_path);
    }

    // Ensure remote directory exists (skip in dry-run)
    if !dry_run {
        ssh::ensure_remote_dir(&host_config.hostname, &host_config.path, &host_config.shell, verbose)?;
    }

    ssh::upload_to_remote(
        local_path.to_str().context("Local path contains invalid UTF-8")?,
        &host_config.hostname,
        &remote_path,
        dry_run,
        verbose,
    )?;

    if !dry_run {
        println!("Upload complete: {} -> {}", file, remote_path);
    }

    Ok(())
}
