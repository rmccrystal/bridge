use anyhow::{Context, Result};
use std::env;
use std::fs;

use crate::config;

const CONFIG_FILENAME: &str = "bridge.toml";

pub fn run(verbose: bool) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let config_path = current_dir.join(CONFIG_FILENAME);

    if config_path.exists() {
        anyhow::bail!(
            "bridge.toml already exists in this directory. Delete it first if you want to reinitialize."
        );
    }

    let template = config::generate_template();

    if verbose {
        eprintln!("Creating {} in {}", CONFIG_FILENAME, current_dir.display());
    }

    fs::write(&config_path, &template)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    println!("Created bridge.toml");
    println!("Edit it to configure your remote hosts.");

    Ok(())
}
