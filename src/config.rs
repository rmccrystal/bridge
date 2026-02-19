use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
