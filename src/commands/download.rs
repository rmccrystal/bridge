use anyhow::Result;
use std::path::Path;

use crate::config::{self, Config};
use crate::ssh;

pub fn run(
    file: &str,
    dest: Option<&str>,
    host: Option<&str>,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let (config, config_path) = Config::find_and_load()?;
    let (host_name, host_config) = config.get_host(host)?;
    let project_root = Config::project_root(&config_path);
    let remote_root = config::effective_remote_path(host_config, &project_root);

    // Build remote path
    let remote_path = if file.starts_with('/') || file.starts_with('~') || file.contains(':') {
        file.to_string()
    } else {
        format!("{}/{}", remote_root, file)
    };

    // Determine local destination
    let local_path = match dest {
        Some(d) => d.to_string(),
        None => {
            Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file)
                .to_string()
        }
    };

    if verbose {
        eprintln!("Downloading from host: {} ({})", host_name, host_config.hostname);
        eprintln!("Remote path: {}", remote_path);
        eprintln!("Local path: {}", local_path);
    }

    ssh::download_from_remote(
        &host_config.hostname,
        &remote_path,
        &local_path,
        dry_run,
        verbose,
    )?;

    if !dry_run {
        println!("Download complete: {} -> {}", remote_path, local_path);
    }

    Ok(())
}
