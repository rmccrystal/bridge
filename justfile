# Bridge CLI justfile

# Default recipe: show available commands
default:
    @just --list

# Build the project
build:
    cargo build --release

# Install the CLI binary
install-cli:
    cargo install --path .

# Install the skill for Claude Code and Codex CLI
install-skill:
    mkdir -p ~/.claude/skills/bridge
    cp -r skill/bridge/* ~/.claude/skills/bridge/
    mkdir -p ~/.agents/skills/bridge
    cp -r skill/bridge/* ~/.agents/skills/bridge/
    @echo "Skill installed to ~/.claude/skills/bridge and ~/.agents/skills/bridge"

# Install everything (CLI + skill)
install: install-cli install-skill
    @echo "Bridge CLI and skill installed successfully"

# Run tests
test:
    cargo test

# Clean build artifacts
clean:
    cargo clean

# Check for issues without building
check:
    cargo check

# Format code
fmt:
    cargo fmt

# Run clippy lints
lint:
    cargo clippy
