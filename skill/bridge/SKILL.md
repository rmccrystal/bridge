---
name: bridge
description: |
  Remote development CLI tool for syncing code, running commands, and transferring files between local and remote machines. Use when working with Bridge CLI, configuring bridge.toml, syncing projects to remote hosts, running remote commands, or troubleshooting remote development workflows.
---

# Bridge CLI

Bridge syncs code, runs commands, and transfers files between local (Mac/Linux) and remote machines (Windows/Linux).

## Commands

```
bridge init                        # Create bridge.toml template
bridge sync                        # Sync project to remote
bridge sync --dry-run              # Preview sync
bridge sync --delete-excluded      # Also delete excluded files from remote (rsync only)
bridge run "<command>"             # Run command on remote
bridge run --sync "<command>"      # Sync first, then run
bridge run --reconnect-command "cmd" "<command>"  # Run cmd after SSH reconnects
bridge run --reconnect-timeout 120 --reconnect-command "cmd" "<command>"
bridge run --lock "<command>"              # Acquire exclusive lock (default name)
bridge run --lock kernel "<command>"       # Named lock (only blocks same name)
bridge run --lock --lock-timeout 60 "<command>"  # Custom lock timeout
bridge upload <file>               # Upload file to remote
bridge download <file>             # Download file from remote
bridge hosts                       # List configured hosts
```

Global flags: `--host <name>`, `--verbose`, `--dry-run`

## Configuration

Create `bridge.toml` in project root (or run `bridge init`):

```toml
default_host = "dev-server"

[hosts.dev-server]
hostname = "dev-server"          # SSH alias or IP address
path = "/home/user/project"      # Remote working directory
shell = "bash"                   # bash (default), powershell, or cmd
wrapper = "source ~/.profile && {}"  # Optional: wrap all commands

[hosts.windows-pc]
hostname = "192.168.1.100"
path = "C:/Users/name/project"
shell = "powershell"
wrapper = "net use \\\\server\\share /user:${DOMAIN_USER} ${DOMAIN_PASS:-}; {}"
strict_env = true

[sync]
exclude = [".git", "target", "node_modules", "__pycache__"]
```

### Config Fields

| Field | Required | Description |
|-------|----------|-------------|
| `default_host` | Yes | Host to use when `--host` not specified |
| `hosts.<name>.hostname` | Yes | SSH alias (from ~/.ssh/config) or IP/hostname |
| `hosts.<name>.path` | Yes | Remote working directory |
| `hosts.<name>.shell` | No | `bash` (default), `powershell`, or `cmd` |
| `hosts.<name>.sync_method` | No | `tar` (default) or `rsync` (incremental) |
| `hosts.<name>.wrapper` | No | Command wrapper template with `{}` placeholder |
| `hosts.<name>.strict_env` | No | Fail on missing `${VAR}` (default: true) |
| `hosts.<name>.env_files` | No | Additional env files to load after `.env` |
| `hosts.<name>.reconnect_command` | No | Command to run after SSH reconnects from unexpected disconnect |
| `hosts.<name>.reconnect_timeout` | No | Seconds to wait for reconnection (default: 90) |
| `hosts.<name>.lock` | No | `true` (default lock) or `"name"` (named lock) for mutual exclusion |
| `hosts.<name>.lock_timeout` | No | Seconds to wait for lock acquisition (default: 600) |
| `sync.exclude` | No | Patterns to exclude from sync |

### Sync Methods

- `tar` (default) - Sends all files every sync using tar over SSH. Works everywhere.
- `rsync` - Incremental sync, only transfers changed files. Deletes files on remote that don't exist locally. Requires `rsync` on both local and remote (install via `choco install rsync` on Windows).

**Auto-excludes**: Mac-specific files (`.DS_Store`, `._*`) are automatically excluded. Use `--no-auto-exclude` to disable.

**Note**: With rsync, excluded files on remote are preserved by default. Use `--delete-excluded` to remove them.

### Command Wrapper

The `wrapper` field lets you wrap all remote commands with setup commands (e.g., load credentials, activate environments). Use `{}` as placeholder for the actual command.

**Environment variable substitution** (from local machine):
- `${VAR}` - Required variable (error if not set when `strict_env=true`)
- `${VAR:-default}` - Optional with fallback value
- `$${VAR}` - Escaped, becomes literal `${VAR}` on remote

Examples:
```toml
# Load shell profile
wrapper = "source ~/.profile && {}"

# Conda/virtualenv activation
wrapper = "source ~/miniconda3/bin/activate ml && {}"

# Windows credentials with local env vars
wrapper = "net use \\\\server\\share /user:${DOMAIN_USER} ${DOMAIN_PASS}; {}"

# API key with default
wrapper = "$env:API_KEY='${API_KEY:-development}'; {}"
```

### Environment Files

Bridge automatically loads `.env` from the project directory (same location as `bridge.toml`). Variables are available for `${VAR}` substitution in commands and wrappers.

```toml
[hosts.prod]
hostname = "prod-server"
path = "/app"
env_files = [".env.prod"]  # Load .env + .env.prod
```

**Priority** (highest to lowest):
1. Process environment (`API_KEY=x bridge run ...`)
2. Files in `env_files` list
3. Default `.env` file

This eliminates the need for manual sourcing:
```bash
# Before: set -a && source .env && set +a && bridge run "..."
# After:  bridge run "..."
```

### SSH Reconnect on Disconnect

If the SSH connection drops unexpectedly (e.g., remote machine blue screens), Bridge can automatically wait for the host to come back and run a recovery command.

Configure per-host in `bridge.toml`:
```toml
[hosts.dev-server]
hostname = "dev-server"
path = "C:/dev/project"
reconnect_command = "get-crash-dump.bat"  # runs after reconnection
reconnect_timeout = 90                     # seconds to wait (default: 90)
```

Or via CLI flags (override config):
```bash
bridge run --reconnect-command "get-crash-dump.bat" "load-driver.bat"
bridge run --reconnect-timeout 120 --reconnect-command "dump.sh" "start-service"
```

Behavior: When SSH exits with code 255 (connection failure) and a reconnect command is set, Bridge polls the host every 5 seconds. On reconnection, it runs the reconnect command with the same wrapper/shell/path settings. If the timeout expires, it exits with code 255.

### Command Locking

When multiple processes (e.g., two Claude Code instances) run Bridge commands on the same host, they can conflict. The lock feature provides mutual exclusion using file-based advisory locks.

Configure per-host in `bridge.toml`:
```toml
[hosts.dev-server]
hostname = "dev-server"
path = "/home/user/project"
lock = true              # Lock with default name (blocks all locked commands on this host)
lock_timeout = 600       # Seconds to wait for lock (default: 600)

[hosts.ml-server]
hostname = "ml-box"
path = "/home/user/ml"
lock = "kernel"          # Named lock (only blocks commands with same lock name)
```

Or via CLI flags (override config):
```bash
bridge run --lock "make test"                    # Default lock name
bridge run --lock kernel "make test"             # Named lock
bridge run --lock --lock-timeout 60 "make test"  # Custom timeout
```

Behavior: Lock files are stored at `/tmp/bridge-{hostname}-{lock_name}.lock`. When a lock is held, the waiting process prints a message and polls every 2 seconds. If the timeout expires, the command fails with an error. Locks are released automatically when the process exits.

### Shell Options

- `bash` - Linux, macOS, Windows Git Bash (default)
- `powershell` - Windows PowerShell
- `cmd` - Windows Command Prompt

## Typical Workflow

```bash
# Setup
cd myproject
bridge init
# Edit bridge.toml with host details

# Development cycle
bridge run --sync "cargo build"
bridge run --sync "cargo test"
bridge download target/debug/myapp
```

## Troubleshooting

- **Config not found**: Bridge walks up directories looking for `bridge.toml`
- **SSH errors**: Ensure SSH key auth is configured for the hostname
- **Sync issues**: Check exclude patterns, use `--verbose` for details
- **Windows paths**: Use forward slashes in bridge.toml (e.g., `C:/Users/name`)

## Requirements

- Local: `tar`, `ssh`, `scp` (standard on Mac/Linux)
- Remote: SSH server, `tar` (included in Windows 10+, Git Bash, WSL)
