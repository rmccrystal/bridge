use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const DEFAULT_ENV_FILE: &str = ".env";

/// Load environment variables from .env files in the project directory.
///
/// Loading order (later files override earlier):
/// 1. Default `.env` file (silently skipped if missing)
/// 2. Additional files from `env_files` config (error if missing)
///
/// Returns a HashMap of variable names to values.
pub fn load_env_files(project_root: &Path, additional_files: &[String]) -> Result<HashMap<String, String>> {
    let mut env_vars = HashMap::new();

    // Load default .env file (silently skip if missing)
    let default_env_path = project_root.join(DEFAULT_ENV_FILE);
    if default_env_path.exists() {
        let vars = parse_env_file(&default_env_path)
            .with_context(|| format!("Failed to parse {}", default_env_path.display()))?;
        env_vars.extend(vars);
    }

    // Load additional env files (error if missing)
    for file in additional_files {
        let path = project_root.join(file);
        if !path.exists() {
            anyhow::bail!(
                "Environment file not found: {}. Remove it from env_files or create the file.",
                path.display()
            );
        }
        let vars = parse_env_file(&path)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        env_vars.extend(vars);
    }

    Ok(env_vars)
}

/// Parse a single .env file into a HashMap.
///
/// Supported syntax:
/// - `KEY=value`
/// - `KEY="quoted value"`
/// - `KEY='single quoted'`
/// - `export KEY=value` (export prefix stripped)
/// - Comments starting with `#`
/// - Empty lines (ignored)
fn parse_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)?;
    let mut vars = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Strip optional 'export ' prefix
        let line = line.strip_prefix("export ").unwrap_or(line);

        // Find the first '=' to split key and value
        let Some(eq_pos) = line.find('=') else {
            continue; // Skip lines without '='
        };

        let key = line[..eq_pos].trim();
        let value = line[eq_pos + 1..].trim();

        // Validate key format
        if !is_valid_env_key(key) {
            anyhow::bail!(
                "Invalid environment variable name '{}' at {}:{}",
                key,
                path.display(),
                line_num + 1
            );
        }

        // Parse value, handling quotes
        let parsed_value = parse_value(value);

        vars.insert(key.to_string(), parsed_value);
    }

    Ok(vars)
}

/// Check if a string is a valid environment variable name.
fn is_valid_env_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let mut chars = key.chars();
    // First char must be letter or underscore
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    // Rest must be alphanumeric or underscore
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse a value, handling quoted strings.
fn parse_value(value: &str) -> String {
    let value = value.trim();

    // Handle double-quoted strings
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return value[1..value.len() - 1].to_string();
    }

    // Handle single-quoted strings
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return value[1..value.len() - 1].to_string();
    }

    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_env_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_basic_parsing() {
        let dir = TempDir::new().unwrap();
        create_env_file(
            dir.path(),
            ".env",
            r#"
KEY1=value1
KEY2=value2
"#,
        );

        let vars = load_env_files(dir.path(), &[]).unwrap();
        assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("KEY2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_quoted_values() {
        let dir = TempDir::new().unwrap();
        create_env_file(
            dir.path(),
            ".env",
            r#"
DOUBLE="hello world"
SINGLE='single quoted'
UNQUOTED=no_quotes
"#,
        );

        let vars = load_env_files(dir.path(), &[]).unwrap();
        assert_eq!(vars.get("DOUBLE"), Some(&"hello world".to_string()));
        assert_eq!(vars.get("SINGLE"), Some(&"single quoted".to_string()));
        assert_eq!(vars.get("UNQUOTED"), Some(&"no_quotes".to_string()));
    }

    #[test]
    fn test_export_prefix() {
        let dir = TempDir::new().unwrap();
        create_env_file(
            dir.path(),
            ".env",
            r#"
export EXPORTED=value
NORMAL=other
"#,
        );

        let vars = load_env_files(dir.path(), &[]).unwrap();
        assert_eq!(vars.get("EXPORTED"), Some(&"value".to_string()));
        assert_eq!(vars.get("NORMAL"), Some(&"other".to_string()));
    }

    #[test]
    fn test_comments_and_empty_lines() {
        let dir = TempDir::new().unwrap();
        create_env_file(
            dir.path(),
            ".env",
            r#"
# This is a comment
KEY1=value1

# Another comment
KEY2=value2
"#,
        );

        let vars = load_env_files(dir.path(), &[]).unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("KEY2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_missing_default_env_silent() {
        let dir = TempDir::new().unwrap();
        // No .env file created - should not error
        let vars = load_env_files(dir.path(), &[]).unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn test_missing_additional_file_errors() {
        let dir = TempDir::new().unwrap();
        let result = load_env_files(dir.path(), &[".env.prod".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".env.prod"));
    }

    #[test]
    fn test_additional_files_override() {
        let dir = TempDir::new().unwrap();
        create_env_file(dir.path(), ".env", "KEY=base\nONLY_BASE=yes");
        create_env_file(dir.path(), ".env.prod", "KEY=prod\nONLY_PROD=yes");

        let vars = load_env_files(dir.path(), &[".env.prod".to_string()]).unwrap();
        assert_eq!(vars.get("KEY"), Some(&"prod".to_string())); // Overridden
        assert_eq!(vars.get("ONLY_BASE"), Some(&"yes".to_string()));
        assert_eq!(vars.get("ONLY_PROD"), Some(&"yes".to_string()));
    }

    #[test]
    fn test_is_valid_env_key() {
        assert!(is_valid_env_key("KEY"));
        assert!(is_valid_env_key("_KEY"));
        assert!(is_valid_env_key("KEY_123"));
        assert!(is_valid_env_key("MY_VAR_NAME"));
        assert!(!is_valid_env_key(""));
        assert!(!is_valid_env_key("123KEY"));
        assert!(!is_valid_env_key("KEY-NAME"));
        assert!(!is_valid_env_key("key.name"));
    }
}
