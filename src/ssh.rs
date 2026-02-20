use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};

use crate::config::Shell;
use crate::env_subst::substitute_env_vars;

/// Run a command on a remote host via SSH, streaming output in real-time.
/// Changes to the remote path and uses the configured shell to execute the command.
///
/// Processing order:
/// 1. Substitute local environment variables in command
/// 2. Substitute local environment variables in wrapper (if present)
/// 3. Apply wrapper template (command replaces {} placeholder)
/// 4. Wrap with shell-specific cd to remote path
/// 5. Execute via SSH
pub fn run_remote_command(
    hostname: &str,
    remote_path: &str,
    command: &str,
    shell: &Shell,
    wrapper: Option<&str>,
    strict_env: bool,
    env_vars: &HashMap<String, String>,
    interactive: bool,
    verbose: bool,
) -> Result<i32> {
    // Step 1: Substitute environment variables in the user command
    let command = substitute_env_vars(command, strict_env, env_vars)
        .context("Failed to substitute environment variables in command")?;

    // Step 2 & 3: Apply wrapper if configured
    let wrapped_command = apply_wrapper(&command, wrapper, strict_env, env_vars)?;

    // Step 4: Wrap with cd to remote path, based on shell type
    let full_cmd = match shell {
        Shell::Bash => format!(r#"cd "{}" && {}"#, remote_path, wrapped_command),
        Shell::Powershell => format!(
            r#"powershell -Command "cd '{}'; {}""#,
            remote_path,
            wrapped_command.replace('"', r#"\""#)
        ),
        Shell::Cmd => format!(
            r#"cd /d "{}" && {}"#,
            remote_path.replace('/', "\\"),
            wrapped_command
        ),
    };

    if verbose {
        eprintln!("Running: ssh {} {}", hostname, full_cmd);
    }

    // Step 5: Execute
    // Keepalive settings ensure SSH detects dead connections quickly (~15s)
    // rather than waiting for TCP timeout (can be minutes).
    let mut cmd = Command::new("ssh");
    if interactive {
        cmd.arg("-t");
    }
    cmd.args(["-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3"])
        .arg(hostname)
        .arg(&full_cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = cmd.spawn()
        .context("Failed to spawn SSH process")?;

    let status = child.wait().context("Failed to wait for SSH process")?;

    Ok(status.code().unwrap_or(1))
}

/// Apply wrapper template to command, with environment variable substitution.
fn apply_wrapper(
    command: &str,
    wrapper: Option<&str>,
    strict_env: bool,
    env_vars: &HashMap<String, String>,
) -> Result<String> {
    let Some(wrapper_template) = wrapper else {
        return Ok(command.to_string());
    };

    // Validate wrapper has placeholder before substitution
    if !wrapper_template.contains("{}") {
        anyhow::bail!(
            "Wrapper template must contain '{{}}' placeholder for command. Got: {}",
            wrapper_template
        );
    }

    // Substitute environment variables in wrapper
    let wrapper = substitute_env_vars(wrapper_template, strict_env, env_vars)
        .context("Failed to substitute environment variables in wrapper")?;

    // Replace placeholder with command
    Ok(wrapper.replace("{}", command))
}

/// Check if an SSH connection to the host can be established.
/// Returns true if the host is reachable, false otherwise.
pub fn check_connection(hostname: &str) -> bool {
    Command::new("ssh")
        .args(["-o", "ConnectTimeout=5", "-o", "BatchMode=yes", hostname, "exit 0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Ensure remote directory exists
pub fn ensure_remote_dir(hostname: &str, remote_path: &str, shell: &Shell, verbose: bool) -> Result<()> {
    let mkdir_cmd = match shell {
        Shell::Bash => format!(r#"mkdir -p "{}""#, remote_path),
        Shell::Powershell => format!(
            r#"powershell -Command "New-Item -ItemType Directory -Force -Path '{}' | Out-Null""#,
            remote_path
        ),
        Shell::Cmd => format!(r#"mkdir "{}" 2>nul || echo."#, remote_path.replace('/', "\\")),
    };

    if verbose {
        eprintln!("Ensuring remote directory exists: {}", remote_path);
        eprintln!("Running: ssh {} {}", hostname, mkdir_cmd);
    }

    let status = Command::new("ssh")
        .arg(hostname)
        .arg(&mkdir_cmd)
        .status()
        .context("Failed to create remote directory")?;

    if !status.success() {
        anyhow::bail!("Failed to create remote directory: {}", remote_path);
    }

    Ok(())
}

/// Sync local directory to remote using tar over SSH
pub fn sync_to_remote(
    source: &str,
    hostname: &str,
    remote_path: &str,
    excludes: &[String],
    shell: &Shell,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    // Build tar exclude arguments
    let mut tar_args = vec!["-czf".to_string(), "-".to_string()];
    for exclude in excludes {
        tar_args.push(format!("--exclude={}", exclude));
    }
    tar_args.push(".".to_string());

    // Build the extract command based on shell type
    let extract_cmd = match shell {
        Shell::Bash => format!(r#"cd "{}" && tar -xzf -"#, remote_path),
        Shell::Powershell => format!(r#"powershell -Command "cd '{}'; tar -xzf -""#, remote_path),
        Shell::Cmd => format!(r#"cd /d "{}" && tar -xzf -"#, remote_path.replace('/', "\\")),
    };

    if dry_run {
        eprintln!("Would sync {} to {}:{}", source, hostname, remote_path);
        eprintln!("  tar {}", tar_args.join(" "));
        eprintln!("  | ssh {} \"{}\"", hostname, extract_cmd);
        return Ok(());
    }

    if verbose {
        eprintln!("Syncing {} to {}:{}", source, hostname, remote_path);
    }

    // Create tar process
    // COPYFILE_DISABLE prevents macOS from creating ._* AppleDouble files in the archive
    let mut tar = Command::new("tar")
        .args(&tar_args)
        .current_dir(source)
        .env("COPYFILE_DISABLE", "1")
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to spawn tar process")?;

    let tar_stdout = tar.stdout.take().context("Failed to get tar stdout")?;

    let mut ssh = Command::new("ssh")
        .arg(hostname)
        .arg(&extract_cmd)
        .stdin(tar_stdout)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn SSH process")?;

    let tar_status = tar.wait().context("Failed to wait for tar")?;
    let ssh_status = ssh.wait().context("Failed to wait for SSH")?;

    if !tar_status.success() {
        anyhow::bail!("tar failed with exit code: {}", tar_status.code().unwrap_or(1));
    }

    if !ssh_status.success() {
        anyhow::bail!("SSH/extract failed with exit code: {}", ssh_status.code().unwrap_or(1));
    }

    Ok(())
}

/// Convert a Windows path (C:/foo or C:\foo) to Cygwin format (/cygdrive/c/foo)
fn to_cygwin_path(path: &str) -> String {
    // Check for Windows drive letter pattern: C:/ or C:\
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        let drive = path.chars().next().unwrap().to_ascii_lowercase();
        let rest = &path[2..].replace('\\', "/");
        format!("/cygdrive/{}{}", drive, rest)
    } else {
        path.to_string()
    }
}

/// Sync local directory to remote using rsync (incremental, deletes removed files)
pub fn rsync_to_remote(
    source: &str,
    hostname: &str,
    remote_path: &str,
    excludes: &[String],
    shell: &Shell,
    delete_excluded: bool,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    // Build rsync arguments
    let mut args = vec![
        "-az".to_string(),      // archive mode + compress
        "--delete".to_string(), // delete files on remote that don't exist locally
    ];

    if delete_excluded {
        args.push("--delete-excluded".to_string());
    }

    // Disable permission preservation for Windows to avoid DENY ACL issues
    if matches!(shell, Shell::Powershell | Shell::Cmd) {
        args.push("--no-perms".to_string());
    }

    if verbose {
        args.push("-v".to_string());
    }

    if dry_run {
        args.push("--dry-run".to_string());
    }

    for exclude in excludes {
        args.push(format!("--exclude={}", exclude));
    }

    // Source must end with / to sync contents, not the directory itself
    let source_path = if source.ends_with('/') {
        source.to_string()
    } else {
        format!("{}/", source)
    };
    args.push(source_path.clone());

    // Convert Windows path to Cygwin format for rsync compatibility
    let cygwin_path = to_cygwin_path(remote_path);

    // Destination: host:path
    let dest = format!("{}:{}", hostname, cygwin_path);
    args.push(dest.clone());

    if dry_run {
        eprintln!("Would rsync {} to {}", source_path, dest);
    }

    if verbose {
        eprintln!("Running: rsync {}", args.join(" "));
    }

    let status = Command::new("rsync")
        .args(&args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run rsync")?;

    if !status.success() {
        anyhow::bail!("rsync failed with exit code: {}", status.code().unwrap_or(1));
    }

    Ok(())
}

/// Download file or directory from remote using scp
pub fn download_from_remote(
    hostname: &str,
    remote_path: &str,
    local_path: &str,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let source = format!("{}:{}", hostname, remote_path);

    if dry_run {
        eprintln!("Would download {} to {}", source, local_path);
        return Ok(());
    }

    if verbose {
        eprintln!("Downloading {} to {}", source, local_path);
    }

    let status = Command::new("scp")
        .arg("-r")
        .arg(&source)
        .arg(local_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run scp")?;

    if !status.success() {
        anyhow::bail!("scp failed with exit code: {}", status.code().unwrap_or(1));
    }

    Ok(())
}

/// Upload file to remote using scp
pub fn upload_to_remote(
    local_path: &str,
    hostname: &str,
    remote_path: &str,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let dest = format!("{}:{}", hostname, remote_path);

    if dry_run {
        eprintln!("Would upload {} to {}", local_path, dest);
        return Ok(());
    }

    if verbose {
        eprintln!("Uploading {} to {}", local_path, dest);
    }

    let status = Command::new("scp")
        .arg("-r")
        .arg(local_path)
        .arg(&dest)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run scp")?;

    if !status.success() {
        anyhow::bail!("scp failed with exit code: {}", status.code().unwrap_or(1));
    }

    Ok(())
}
