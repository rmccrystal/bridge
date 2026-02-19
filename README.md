# Bridge

One config file replaces your ad-hoc SSH scripts for syncing, building, and testing on remote hosts.

![bridge demo](demo.gif)

- **One command to sync + run** — `bridge run --sync "cargo build"` replaces manual rsync/ssh chains
- **Multi-shell support** — bash, PowerShell, and cmd with automatic command wrapping
- **Environment variable substitution** — loads `.env` files locally, substitutes into remote commands
- **Locking and auto-reconnect** — resilient, conflict-free workflows across multiple sessions

## Installation

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (for building from source)
- SSH client with key-based authentication configured
- `tar` (included on macOS/Linux)
- Optional: `rsync` (for incremental sync), [`just`](https://github.com/casey/just) (for install recipes)

### From source

```bash
# From a local clone
cargo install --path .

# Or directly from GitHub
cargo install --git https://github.com/rmccrystal/bridge
```

Or using `just` (also installs an AI coding assistant integration):

```bash
just install
```

## Quick Start

1. Initialize a config file in your project:

```bash
# Verify SSH access first
ssh dev-server "echo connected"

cd myproject
bridge init
```

2. Edit `bridge.toml` with your remote host details:

```toml
default_host = "dev-server"

[hosts.dev-server]
hostname = "dev-server"        # SSH alias or IP address
path = "/home/user/myproject"  # Remote working directory
```

3. Sync and run commands:

```bash
bridge run --sync "cargo build"   # Sync project, then build remotely
bridge run "cargo test"           # Run tests (no sync needed)
bridge download target/debug/app  # Grab the binary
```

## Usage

```
Remote development tool for syncing code and running commands

Usage: bridge [OPTIONS] <COMMAND>

Commands:
  sync      Sync current directory to remote
  run       Run command on remote
  upload    Upload single file to remote
  download  Download file from remote
  init      Create bridge.toml in current directory
  hosts     List configured hosts
  help      Print this message or the help of the given subcommand(s)

Options:
      --host <HOST>  Override default host
  -v, --verbose      Detailed output
      --dry-run      Preview without executing
  -h, --help         Print help
  -V, --version      Print version
```

### sync

Sync the current project directory to the remote host.

```bash
bridge sync                     # Sync to default host
bridge sync --host work         # Sync to a specific host
bridge sync --dry-run           # Preview what would happen
bridge sync --no-auto-exclude   # Include .DS_Store and ._* files
bridge sync --delete-excluded   # Delete excluded files on remote (rsync only)
```

<details>
<summary>Full options</summary>

```
Usage: bridge sync [OPTIONS]

Options:
      --host <HOST>      Override default host
      --no-auto-exclude  Disable auto-exclusion of Mac-specific files (.DS_Store, ._*)
      --delete-excluded  Delete excluded files from remote (rsync only)
  -v, --verbose          Detailed output
      --dry-run          Preview without executing
```

</details>

### run

Execute a command on the remote host.

```bash
bridge run "cargo test"                          # Run on default host
bridge run --sync "make build"                   # Sync first, then run
bridge run --host gpu "python train.py"          # Target a specific host
bridge run --lock "make install"                 # Acquire exclusive lock
bridge run --reconnect-command "dump.sh" "start" # Auto-reconnect and run command on disconnect
```

<details>
<summary>Full options</summary>

```
Usage: bridge run [OPTIONS] <COMMAND>

Arguments:
  <COMMAND>  Command to execute

Options:
      --host <HOST>                              Override default host
  -s, --sync                                     Sync before running
      --reconnect-command <RECONNECT_COMMAND>     Command to run after reconnecting from unexpected SSH disconnect (overrides config)
      --reconnect-timeout <RECONNECT_TIMEOUT>     Seconds to wait for reconnection (overrides config, default: 90)
      --lock [<LOCK>]                             Acquire exclusive lock before running (optional lock name)
      --lock-timeout <LOCK_TIMEOUT>               Seconds to wait for lock (default: 600)
  -v, --verbose                                  Detailed output
      --dry-run                                  Preview without executing
```

</details>

### upload

Upload a single file to the remote host's project directory.

```bash
bridge upload data.csv                    # Upload to remote project dir
bridge upload data.csv --dest input.csv   # Upload with a different name
```

<details>
<summary>Full options</summary>

```
Usage: bridge upload [OPTIONS] <FILE>

Arguments:
  <FILE>  File to upload

Options:
      --dest <DEST>  Remote destination filename
      --host <HOST>  Override default host
  -v, --verbose      Detailed output
      --dry-run      Preview without executing
```

</details>

### download

Download a file from the remote host.

```bash
bridge download output.log                    # Download to current dir
bridge download dist/ --dest ./build/         # Download a directory
bridge download /tmp/debug.log                # Absolute remote path
```

<details>
<summary>Full options</summary>

```
Usage: bridge download [OPTIONS] <FILE>

Arguments:
  <FILE>  File to download

Options:
      --dest <DEST>  Local destination path
      --host <HOST>  Override default host
  -v, --verbose      Detailed output
      --dry-run      Preview without executing
```

</details>

### init

Create a `bridge.toml` template in the current directory.

```bash
bridge init
```

### hosts

List all configured hosts, marking the default.

```bash
bridge hosts             # List hosts
bridge hosts --verbose   # Also show config file path
```

## Configuration

Bridge looks for `bridge.toml` in the current directory, then walks up parent directories (like git). Run `bridge init` to create a template.

```toml
default_host = "dev-server"

[hosts.dev-server]
hostname = "dev-server"                        # SSH alias or IP address
path = "/home/user/project"                    # Remote working directory
shell = "bash"                                 # bash (default), powershell, or cmd
sync_method = "rsync"                          # tar (default) or rsync (incremental)
wrapper = "source ~/.profile && {}"            # Wrap all commands (see Command Wrappers)
strict_env = true                              # Fail on missing ${VAR} (default: true)
env_files = [".env.prod"]                      # Additional env files to load after .env
reconnect_command = "get-crash-dump.sh"        # Run after SSH reconnects from disconnect
reconnect_timeout = 90                         # Seconds to wait for reconnection (default: 90)
lock = true                                    # Acquire exclusive lock before commands
lock_timeout = 600                             # Seconds to wait for lock (default: 600)

[hosts.windows-pc]
hostname = "192.168.1.100"
path = "C:/Users/name/project"
shell = "powershell"
wrapper = "touch C:/test.txt"

[sync]
exclude = [".git", "target", "node_modules", "__pycache__"]
```

### Configuration Reference

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `default_host` | Yes | — | Host to use when `--host` is not specified |
| `hosts.<name>.hostname` | Yes | — | SSH alias (from `~/.ssh/config`) or IP/hostname |
| `hosts.<name>.path` | Yes | — | Remote working directory |
| `hosts.<name>.shell` | No | `bash` | `bash`, `powershell`, or `cmd` |
| `hosts.<name>.sync_method` | No | `tar` | `tar` or `rsync` |
| `hosts.<name>.wrapper` | No | — | Command wrapper template with `{}` placeholder |
| `hosts.<name>.strict_env` | No | `true` | Fail when `${VAR}` references cannot be resolved |
| `hosts.<name>.env_files` | No | `[]` | Additional env files to load after `.env` |
| `hosts.<name>.reconnect_command` | No | — | Command to run after SSH reconnects from disconnect |
| `hosts.<name>.reconnect_timeout` | No | `90` | Seconds to wait for reconnection |
| `hosts.<name>.lock` | No | `false` | `true` (default lock name) or `"name"` (named lock) |
| `hosts.<name>.lock_timeout` | No | `600` | Seconds to wait for lock acquisition |
| `sync.exclude` | No | `[".git", "target", "node_modules", "__pycache__"]` | Patterns to exclude from sync |

### Sync Methods

**tar** (default) — Sends all files via tar over SSH. Works everywhere, but transfers the entire project each time.

**rsync** — Incremental sync that only transfers changed files. Automatically deletes files on the remote that no longer exist locally. Requires `rsync` on both ends (install via `choco install rsync` on Windows).

```toml
[hosts.dev-server]
sync_method = "rsync"
```

Mac-specific files (`.DS_Store`, `._*`) are automatically excluded from both methods. Use `--no-auto-exclude` to disable this.

With rsync, excluded files already on the remote are preserved by default. Use `--delete-excluded` to remove them.

### Shell Support

| Shell | Platform | Command wrapping |
|-------|----------|-----------------|
| `bash` | Linux, macOS, Git Bash | `cd "path" && command` |
| `powershell` | Windows PowerShell | `powershell -Command "cd 'path'; command"` |
| `cmd` | Windows Command Prompt | `cd /d "path" && command` |

## Command Wrappers

The `wrapper` field lets you wrap every remote command with setup commands. Use `{}` as the placeholder for the actual command.

```toml
# Load shell profile before each command
wrapper = "source ~/.profile && {}"

# Activate a conda environment
wrapper = "source ~/miniconda3/bin/activate ml && {}"

# Set an API key with a default fallback
wrapper = "$env:API_KEY='${API_KEY:-development}'; {}"
```

Environment variables in the wrapper are substituted locally before the command is sent to the remote (see [Environment Variables](#environment-variables)).

## Environment Variables

### Substitution Syntax

Bridge substitutes `${VAR}` patterns in commands and wrappers with local environment variable values before sending them to the remote.

| Syntax | Behavior |
|--------|----------|
| `${VAR}` | Substituted with the value of `VAR`. Errors if unset (when `strict_env = true`). |
| `${VAR:-default}` | Substituted with `VAR` if set, otherwise uses `default`. |
| `$${VAR}` | Escaped. Becomes the literal string `${VAR}` on the remote. |

### .env File Loading

Bridge automatically loads a `.env` file from the project directory (the directory containing `bridge.toml`). No manual sourcing needed:

```bash
# Before: set -a && source .env && set +a && bridge run "..."
# After:  bridge run "..."
```

Load additional env files per-host with `env_files`:

```toml
[hosts.prod]
hostname = "prod-server"
path = "/app"
env_files = [".env.prod"]   # Loads .env first, then .env.prod
```

### Priority Order

When the same variable is defined in multiple places (highest priority wins):

1. Process environment (`API_KEY=x bridge run "..."`)
2. Files listed in `env_files` (later files override earlier ones)
3. Default `.env` file

## Command Locking

When multiple processes (e.g., two terminal sessions or CI jobs) run Bridge commands targeting the same host, they can conflict. The lock feature provides mutual exclusion using local file-based advisory locks.

### Configuration

```toml
[hosts.dev-server]
lock = true          # Lock with default name (blocks all locked commands on this host)
lock_timeout = 600   # Seconds to wait for lock (default: 600)

[hosts.ml-server]
lock = "kernel"      # Named lock (only blocks commands with the same lock name)
```

### CLI flags

```bash
bridge run --lock "make test"                     # Default lock name
bridge run --lock kernel "make test"              # Named lock
bridge run --lock --lock-timeout 60 "make test"   # Custom timeout
```

### Behavior

- Lock files are stored at `/tmp/bridge-{hostname}-{lock_name}.lock`
- When a lock is held, the waiting process prints a message and polls every 2 seconds
- If the timeout expires, the command fails with an error
- Locks are released automatically when the process exits

## Auto-Reconnect

If an SSH connection drops unexpectedly (e.g., remote host reboots), Bridge can wait for the host to come back and run a recovery command.

### Configuration

```toml
[hosts.dev-server]
reconnect_command = "get-crash-dump.bat"   # Runs after reconnection
reconnect_timeout = 90                      # Seconds to wait (default: 90)
```

### CLI flags

```bash
bridge run --reconnect-command "get-crash-dump.bat" "load-driver.bat"
bridge run --reconnect-timeout 120 --reconnect-command "dump.sh" "start-service"
```

### Behavior

- Triggered when SSH exits with code 255 (connection failure) and a reconnect command is configured
- Bridge polls the host every 5 seconds until the connection is restored
- On reconnection, runs the reconnect command with the same wrapper, shell, and path settings
- If the timeout expires, Bridge exits with code 255

## Example Workflows

### Windows remote

```toml
[hosts.windows]
hostname = "win-pc"
path = "C:/Dev/myproject"
shell = "powershell"
```

```bash
bridge sync --host windows
bridge run --host windows "cargo build --release"
bridge download --host windows "target/release/myapp.exe"
```

### ML training with locking and reconnect

```toml
[hosts.gpu]
hostname = "gpu-box"
path = "/home/user/ml"
wrapper = "source ~/miniconda3/bin/activate ml && {}"
lock = "training"
reconnect_command = "nvidia-smi > /tmp/gpu-state.txt"
reconnect_timeout = 120
```

```bash
bridge run --sync "python train.py"   # Locked, reconnect-aware, env activated
```

## Requirements

| Platform | Requirements |
|---|---|
| **Local machine** | macOS or Linux, `ssh`, `scp`, `tar`. Optional: `rsync`. |
| **Remote (Linux)** | SSH server, `tar`. Optional: `rsync`. |
| **Remote (Windows)** | SSH server (OpenSSH), `tar` (included in Windows 10+, Git Bash, or WSL). Optional: `rsync` (via `choco install rsync`). |

## Development

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo install --path .   # Install to ~/.cargo/bin
just install             # Install CLI + Claude Code skill
just install-skill       # Install skill only
```
