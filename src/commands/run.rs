use std::time::{Duration, Instant};
use std::thread;

use anyhow::Result;

use crate::config::{Config, LockSetting};
use crate::env_loader;
use crate::lock;
use crate::ssh;
use super::sync;

pub fn run(
    host: Option<&str>,
    command: &str,
    do_sync: bool,
    dry_run: bool,
    verbose: bool,
    reconnect_command_override: Option<&str>,
    reconnect_timeout_override: Option<u64>,
    lock_override: Option<String>,
    lock_timeout_override: Option<u64>,
) -> Result<i32> {
    // Sync first if requested
    if do_sync {
        sync::run(host, false, false, dry_run, verbose)?;
    }

    let (config, config_path) = Config::find_and_load()?;
    let (host_name, host) = config.get_host(host)?;

    // Load environment variables from .env files
    let project_root = Config::project_root(&config_path);
    let env_vars = env_loader::load_env_files(&project_root, &host.env_files)?;

    // Resolve reconnect settings: CLI flags override config
    let reconnect_command = reconnect_command_override
        .map(|s| s.to_string())
        .or_else(|| host.reconnect_command.clone());
    let reconnect_timeout = reconnect_timeout_override.unwrap_or(host.reconnect_timeout);

    // Resolve lock settings: CLI overrides config
    let lock_name = match lock_override {
        Some(name) => Some(name),
        None => match host.lock {
            LockSetting::Off => None,
            LockSetting::Default => Some("default".to_string()),
            LockSetting::Named(ref n) => Some(n.clone()),
        },
    };

    if verbose {
        eprintln!("Running on host: {} ({})", host_name, host.hostname);
        eprintln!("Remote path: {}", host.path);
        if let Some(ref wrapper) = host.wrapper {
            eprintln!("Wrapper: {}", wrapper);
        }
        if !env_vars.is_empty() {
            eprintln!("Loaded {} env vars from .env files", env_vars.len());
        }
        if let Some(ref rc) = reconnect_command {
            eprintln!("Reconnect command: {} (timeout: {}s)", rc, reconnect_timeout);
        }
        if let Some(ref name) = lock_name {
            eprintln!("Lock: {} (timeout: {}s)", name, lock_timeout_override.unwrap_or(host.lock_timeout));
        }
        eprintln!("Command: {}", command);
    }

    // Acquire lock if configured
    let _lock_guard = if let Some(ref name) = lock_name {
        let timeout = lock_timeout_override.unwrap_or(host.lock_timeout);
        Some(lock::acquire_lock(&host.hostname, name, Duration::from_secs(timeout), verbose)?)
    } else {
        None
    };

    if dry_run {
        eprintln!("Would run: ssh {} cd \"{}\" && {}", host.hostname, host.path, command);
        return Ok(0);
    }

    let exit_code = ssh::run_remote_command(
        &host.hostname,
        &host.path,
        command,
        &host.shell,
        host.wrapper.as_deref(),
        host.strict_env,
        &env_vars,
        verbose,
    )?;

    // Check for unexpected SSH disconnect with reconnect configured
    if exit_code == 255 {
        if let Some(ref reconnect_cmd) = reconnect_command {
            eprintln!("SSH connection lost. Waiting for reconnection (timeout: {}s)...", reconnect_timeout);

            let start = Instant::now();
            let timeout = Duration::from_secs(reconnect_timeout);
            let poll_interval = Duration::from_secs(5);

            loop {
                if start.elapsed() >= timeout {
                    eprintln!("Timed out waiting for reconnection after {}s", reconnect_timeout);
                    return Ok(255);
                }

                thread::sleep(poll_interval);

                eprint!(".");
                if ssh::check_connection(&host.hostname) {
                    eprintln!();
                    eprintln!("Reconnected. Running reconnect command...");

                    let rc_exit = ssh::run_remote_command(
                        &host.hostname,
                        &host.path,
                        reconnect_cmd,
                        &host.shell,
                        host.wrapper.as_deref(),
                        host.strict_env,
                        &env_vars,
                        verbose,
                    )?;

                    return Ok(rc_exit);
                }
            }
        }
    }

    Ok(exit_code)
}
