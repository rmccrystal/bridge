use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CONFIG_FILENAME: &str = "bridge.toml";

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub default_host: Option<String>,
    #[serde(default)]
    pub hosts: HashMap<String, Host>,
    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Host {
    pub hostname: String,
    pub path: String,
    #[serde(default)]
    pub shell: Shell,
    /// Sync method: "tar" (default) or "rsync" (incremental, deletes removed files)
    #[serde(default)]
    pub sync_method: SyncMethod,
    /// If true, linked git worktrees use path-worktree_name as the remote path. Default: true.
    #[serde(default = "default_true")]
    pub worktree_rename: bool,
    /// Optional command wrapper template. Use `{}` as placeholder for the command.
    /// Supports ${VAR} syntax for local environment variable substitution.
    pub wrapper: Option<String>,
    /// If true, fail when ${VAR} references cannot be resolved. Default: true.
    #[serde(default = "default_true")]
    pub strict_env: bool,
    /// Additional env files to load after .env (which is loaded by default).
    /// Files are loaded in order; later files override earlier ones.
    #[serde(default)]
    pub env_files: Vec<String>,
    /// Command to run after reconnecting from an unexpected SSH disconnect.
    /// If not set, reconnect behavior is disabled.
    pub reconnect_command: Option<String>,
    /// Seconds to wait for reconnection before giving up. Default: 90.
    #[serde(default = "default_reconnect_timeout")]
    pub reconnect_timeout: u64,
    /// Lock configuration: false (default), true (lock with default name), or string (named lock)
    #[serde(default)]
    pub lock: LockSetting,
    /// Seconds to wait for lock acquisition before giving up. Default: 600.
    #[serde(default = "default_lock_timeout")]
    pub lock_timeout: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    #[default]
    Tar,
    Rsync,
}

fn default_true() -> bool {
    true
}

fn default_reconnect_timeout() -> u64 {
    90
}

fn default_lock_timeout() -> u64 {
    600
}

/// Lock configuration: off (default), on with default name, or named lock.
#[derive(Debug, Clone, PartialEq)]
pub enum LockSetting {
    /// Lock not configured
    Off,
    /// lock = true → uses "default" lock name
    Default,
    /// lock = "kernel" → uses "kernel" lock name
    Named(String),
}

impl std::default::Default for LockSetting {
    fn default() -> Self {
        LockSetting::Off
    }
}

impl Serialize for LockSetting {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            LockSetting::Off => serializer.serialize_bool(false),
            LockSetting::Default => serializer.serialize_bool(true),
            LockSetting::Named(name) => serializer.serialize_str(name),
        }
    }
}

impl<'de> Deserialize<'de> for LockSetting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        use serde::de;

        struct LockSettingVisitor;

        impl<'de> de::Visitor<'de> for LockSettingVisitor {
            type Value = LockSetting;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a boolean or a string")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<LockSetting, E> {
                if v {
                    Ok(LockSetting::Default)
                } else {
                    Ok(LockSetting::Off)
                }
            }

            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<LockSetting, E> {
                Ok(LockSetting::Named(v.to_string()))
            }

            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<LockSetting, E> {
                Ok(LockSetting::Named(v))
            }
        }

        deserializer.deserialize_any(LockSettingVisitor)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    #[default]
    Bash,
    Powershell,
    Cmd,
}

impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Shell::Bash => write!(f, "bash"),
            Shell::Powershell => write!(f, "powershell"),
            Shell::Cmd => write!(f, "cmd"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SyncConfig {
    #[serde(default = "default_excludes")]
    pub exclude: Vec<String>,
}

fn default_excludes() -> Vec<String> {
    vec![
        ".git".to_string(),
        "target".to_string(),
        "node_modules".to_string(),
        "__pycache__".to_string(),
    ]
}

/// Auto-excludes for Mac-specific files that cause issues on remote systems
pub fn auto_excludes() -> Vec<String> {
    vec![
        ".DS_Store".to_string(),
        "._*".to_string(),
    ]
}

/// Return the remote path Bridge should use for this checkout.
pub fn effective_remote_path(host: &Host, project_root: &Path) -> String {
    if !host.worktree_rename || !is_linked_worktree(project_root) {
        return host.path.clone();
    }

    let Some(worktree_name) = worktree_name(project_root) else {
        return host.path.clone();
    };

    remote_path_with_worktree_suffix(&host.path, &worktree_name)
}

fn is_linked_worktree(project_root: &Path) -> bool {
    let Some(git_dir) = git_output(project_root, &["rev-parse", "--git-dir"]) else {
        return false;
    };
    let Some(common_dir) = git_output(project_root, &["rev-parse", "--git-common-dir"]) else {
        return false;
    };

    git_dir != common_dir
}

fn worktree_name(project_root: &Path) -> Option<String> {
    let worktree_root = git_output(project_root, &["rev-parse", "--show-toplevel"])?;
    Path::new(&worktree_root)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

fn remote_path_with_worktree_suffix(remote_path: &str, worktree_name: &str) -> String {
    let base = remote_path.trim_end_matches(['/', '\\']);
    let base = if base.is_empty() { remote_path } else { base };
    format!("{}-{}", base, worktree_name)
}

fn git_output(project_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let value = stdout.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            default_host: None,
            hosts: HashMap::new(),
            sync: SyncConfig::default(),
        }
    }
}

impl Config {
    /// Find and load config by walking up from current directory
    pub fn find_and_load() -> Result<(Config, PathBuf)> {
        let config_path = find_config_file()?;
        let config = load_config(&config_path)?;
        Ok((config, config_path))
    }

    /// Get the project root directory (where bridge.toml is located)
    pub fn project_root(config_path: &Path) -> PathBuf {
        config_path.parent().unwrap_or(config_path).to_path_buf()
    }

    /// Get a host by name, or the default host
    pub fn get_host(&self, name: Option<&str>) -> Result<(&String, &Host)> {
        let host_name = match name {
            Some(n) => n.to_string(),
            None => self
                .default_host
                .clone()
                .context("No default host configured. Use --host or set default_host in bridge.toml")?,
        };

        let host = self
            .hosts
            .get(&host_name)
            .with_context(|| format!("Host '{}' not found in configuration", host_name))?;

        // Return a reference to the key in the map
        let key = self.hosts.keys()
            .find(|k| *k == &host_name)
            .expect("host key must exist after successful get");
        Ok((key, host))
    }
}

/// Find config file by walking up directory tree
fn find_config_file() -> Result<PathBuf> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let mut dir = current_dir.as_path();

    loop {
        let config_path = dir.join(CONFIG_FILENAME);
        if config_path.exists() {
            return Ok(config_path);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => {
                anyhow::bail!(
                    "No bridge.toml found in current directory or any parent. Run 'bridge init' to create one."
                )
            }
        }
    }
}

/// Load and parse config from a file
fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}

/// Generate a template config file
pub fn generate_template() -> String {
    r#"default_host = "dev-server"

[hosts.dev-server]
hostname = "dev-server"        # SSH alias (from ~/.ssh/config) or IP
path = "/home/user/projects/myproject"
# shell = "bash"               # bash (default), powershell, or cmd
# sync_method = "rsync"        # tar (default) or rsync (incremental, deletes removed files)
# worktree_rename = true       # Linked git worktrees use path-worktree_name (default: true)
# wrapper = "source ~/.profile && {}"  # Optional: wrap all commands
# strict_env = true            # Fail on missing ${VAR} references (default: true)
# env_files = [".env.prod"]    # Additional env files to load after .env
# reconnect_command = "get-crash-dump.sh"  # Run after SSH reconnects from unexpected disconnect
# reconnect_timeout = 90       # Seconds to wait for reconnection (default: 90)
# lock = true                  # Acquire exclusive lock before running commands
# lock = "kernel"              # Named lock (only blocks commands with same lock name)
# lock_timeout = 600           # Seconds to wait for lock (default: 600)

# Windows example with environment loading:
# [hosts.windows-pc]
# hostname = "192.168.1.100"
# path = "C:/Users/name/dev/myproject"
# shell = "powershell"
# wrapper = "net use \\\\server\\share /user:${DOMAIN_USER} ${DOMAIN_PASS:-}; {}"

# Conda environment example:
# [hosts.ml-server]
# hostname = "ml-box"
# path = "/home/user/ml-project"
# wrapper = "source ~/miniconda3/bin/activate ml && {}"

[sync]
exclude = [".git", "target", "node_modules", "__pycache__"]
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn worktree_rename_defaults_to_true() {
        let config: Config = toml::from_str(
            r#"
default_host = "dev"

[hosts.dev]
hostname = "dev"
path = "/home/user/project"
"#,
        )
        .unwrap();

        let host = config.hosts.get("dev").unwrap();
        assert!(host.worktree_rename);
    }

    #[test]
    fn worktree_rename_can_be_disabled() {
        let config: Config = toml::from_str(
            r#"
default_host = "dev"

[hosts.dev]
hostname = "dev"
path = "/home/user/project"
worktree_rename = false
"#,
        )
        .unwrap();

        let host = config.hosts.get("dev").unwrap();
        assert!(!host.worktree_rename);
    }

    #[test]
    fn remote_path_suffix_handles_unix_windows_and_trailing_separators() {
        assert_eq!(
            remote_path_with_worktree_suffix("/home/user/project", "project-fix"),
            "/home/user/project-project-fix"
        );
        assert_eq!(
            remote_path_with_worktree_suffix("/home/user/project/", "project-fix"),
            "/home/user/project-project-fix"
        );
        assert_eq!(
            remote_path_with_worktree_suffix("C:/Users/name/project", "project-fix"),
            "C:/Users/name/project-project-fix"
        );
        assert_eq!(
            remote_path_with_worktree_suffix(r"C:\Users\name\project\\", "project-fix"),
            r"C:\Users\name\project-project-fix"
        );
    }

    #[test]
    fn primary_worktree_uses_configured_path() {
        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init"]);

        let host = test_host(true);
        assert_eq!(effective_remote_path(&host, dir.path()), "/remote/project");
    }

    #[test]
    fn linked_worktree_uses_worktree_directory_suffix() {
        let dir = TempDir::new().unwrap();
        let main = dir.path().join("repo");
        let linked = dir.path().join("repo-codex-1");

        fs::create_dir(&main).unwrap();
        git(&main, &["init"]);
        git(&main, &["config", "user.email", "bridge@example.com"]);
        git(&main, &["config", "user.name", "Bridge Tests"]);
        git(&main, &["commit", "--allow-empty", "-m", "init"]);
        git(&main, &["worktree", "add", linked.to_str().unwrap()]);

        let host = test_host(true);
        assert_eq!(
            effective_remote_path(&host, &linked),
            "/remote/project-repo-codex-1"
        );
    }

    #[test]
    fn disabled_worktree_rename_uses_configured_path_in_linked_worktree() {
        let dir = TempDir::new().unwrap();
        let main = dir.path().join("repo");
        let linked = dir.path().join("repo-codex-2");

        fs::create_dir(&main).unwrap();
        git(&main, &["init"]);
        git(&main, &["config", "user.email", "bridge@example.com"]);
        git(&main, &["config", "user.name", "Bridge Tests"]);
        git(&main, &["commit", "--allow-empty", "-m", "init"]);
        git(&main, &["worktree", "add", linked.to_str().unwrap()]);

        let host = test_host(false);
        assert_eq!(effective_remote_path(&host, &linked), "/remote/project");
    }

    fn test_host(worktree_rename: bool) -> Host {
        Host {
            hostname: "dev".to_string(),
            path: "/remote/project".to_string(),
            shell: Shell::Bash,
            sync_method: SyncMethod::Tar,
            worktree_rename,
            wrapper: None,
            strict_env: true,
            env_files: Vec::new(),
            reconnect_command: None,
            reconnect_timeout: default_reconnect_timeout(),
            lock: LockSetting::Off,
            lock_timeout: default_lock_timeout(),
        }
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "git -C {} {} failed\nstdout:\n{}\nstderr:\n{}",
            cwd.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
