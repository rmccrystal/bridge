use anyhow::Result;

use crate::config::{Config, Shell};
use crate::env_loader;
use crate::ssh;
use super::sync;

pub fn run(host: Option<&str>, do_sync: bool, verbose: bool) -> Result<i32> {
    if do_sync {
        sync::run(host, false, false, false, verbose)?;
    }

    let (config, config_path) = Config::find_and_load()?;
    let (host_name, host) = config.get_host(host)?;

    let project_root = Config::project_root(&config_path);
    let env_vars = env_loader::load_env_files(&project_root, &host.env_files)?;

    let shell_cmd = match host.shell {
        Shell::Bash => "bash",
        Shell::Powershell => "powershell",
        Shell::Cmd => "cmd",
    };

    if verbose {
        eprintln!("Opening SSH session on host: {} ({})", host_name, host.hostname);
        eprintln!("Remote path: {}", host.path);
        eprintln!("Shell: {}", shell_cmd);
    }

    let exit_code = ssh::run_remote_command(
        &host.hostname,
        &host.path,
        shell_cmd,
        &host.shell,
        host.wrapper.as_deref(),
        host.strict_env,
        &env_vars,
        true,
        verbose,
    )?;

    Ok(exit_code)
}
