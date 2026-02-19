use anyhow::Result;

use crate::config::Config;

pub fn run(verbose: bool) -> Result<()> {
    let (config, config_path) = Config::find_and_load()?;

    if verbose {
        eprintln!("Config loaded from: {}", config_path.display());
    }

    if config.hosts.is_empty() {
        println!("No hosts configured.");
        println!("Edit bridge.toml to add hosts.");
        return Ok(());
    }

    let default_host = config.default_host.as_deref();

    for (name, host) in &config.hosts {
        let is_default = default_host == Some(name.as_str());
        let default_marker = if is_default { " (default)" } else { "" };

        println!("{}{}", name, default_marker);
        println!("  hostname: {}", host.hostname);
        println!("  path: {}", host.path);
        println!("  shell: {}", host.shell);
        println!();
    }

    Ok(())
}
