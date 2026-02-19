use anyhow::{bail, Result};
use regex::Regex;
use std::collections::HashMap;
use std::env;

/// Substitute ${VAR} patterns with environment variables.
///
/// Syntax:
/// - ${VAR}          - Required variable, error if not set (when strict=true)
/// - ${VAR:-default} - Optional variable with fallback default value
/// - $${VAR}         - Escaped, becomes literal ${VAR} in output
///
/// # Arguments
/// * `input` - String containing ${VAR} patterns
/// * `strict` - If true, error on missing required variables; if false, use empty string
/// * `env_vars` - Additional env vars from .env files (process env takes priority)
///
/// # Lookup Order
/// 1. Process environment variables (highest priority, allows CLI overrides)
/// 2. Variables from env_vars HashMap (loaded from .env files)
/// 3. Default value if provided (${VAR:-default})
/// 4. Error if strict=true, empty string if strict=false
pub fn substitute_env_vars(
    input: &str,
    strict: bool,
    env_vars: &HashMap<String, String>,
) -> Result<String> {
    let re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}").expect("valid regex");

    let mut result = input.to_string();
    let mut missing_vars = Vec::new();

    // Step 1: Protect escaped sequences $${...}
    let escape_marker = "\x00ESC\x00";
    result = result.replace("$${", escape_marker);

    // Step 2: Collect all substitutions (to avoid modifying while iterating)
    let substitutions: Vec<_> = re
        .captures_iter(&result.clone())
        .map(|cap| {
            let full_match = cap.get(0).unwrap().as_str().to_string();
            let var_name = cap[1].to_string();
            let default = cap.get(2).map(|m| m.as_str().to_string());
            (full_match, var_name, default)
        })
        .collect();

    // Step 3: Apply substitutions
    // Lookup order: process env > env_vars HashMap > default > error/empty
    for (full_match, var_name, default) in substitutions {
        let replacement = match env::var(&var_name) {
            Ok(value) => value,
            Err(_) => match env_vars.get(&var_name) {
                Some(value) => value.clone(),
                None => match default {
                    Some(def) => def,
                    None if strict => {
                        missing_vars.push(var_name.clone());
                        continue;
                    }
                    None => String::new(),
                },
            },
        };
        result = result.replacen(&full_match, &replacement, 1);
    }

    // Step 4: Restore escaped sequences
    result = result.replace(escape_marker, "${");

    // Step 5: Report errors
    if !missing_vars.is_empty() {
        bail!(
            "Missing required environment variables: {}. \
             Use ${{VAR:-default}} syntax for optional variables, \
             or set strict_env = false in bridge.toml",
            missing_vars.join(", ")
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_vars() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn test_basic_substitution() {
        env::set_var("BRIDGE_TEST_VAR", "hello");
        assert_eq!(
            substitute_env_vars("${BRIDGE_TEST_VAR}", true, &empty_vars()).unwrap(),
            "hello"
        );
        env::remove_var("BRIDGE_TEST_VAR");
    }

    #[test]
    fn test_default_value() {
        assert_eq!(
            substitute_env_vars("${BRIDGE_NONEXISTENT:-fallback}", true, &empty_vars()).unwrap(),
            "fallback"
        );
    }

    #[test]
    fn test_escaped() {
        assert_eq!(
            substitute_env_vars("$${LITERAL}", true, &empty_vars()).unwrap(),
            "${LITERAL}"
        );
    }

    #[test]
    fn test_strict_missing() {
        let result = substitute_env_vars("${BRIDGE_MISSING_VAR_12345}", true, &empty_vars());
        assert!(result.is_err());
    }

    #[test]
    fn test_non_strict_missing() {
        assert_eq!(
            substitute_env_vars("${BRIDGE_MISSING_VAR_12345}", false, &empty_vars()).unwrap(),
            ""
        );
    }

    #[test]
    fn test_multiple_vars() {
        env::set_var("BRIDGE_A", "one");
        env::set_var("BRIDGE_B", "two");
        assert_eq!(
            substitute_env_vars("${BRIDGE_A} and ${BRIDGE_B}", true, &empty_vars()).unwrap(),
            "one and two"
        );
        env::remove_var("BRIDGE_A");
        env::remove_var("BRIDGE_B");
    }

    #[test]
    fn test_wrapper_example() {
        env::set_var("BRIDGE_USER", "admin");
        assert_eq!(
            substitute_env_vars("echo ${BRIDGE_USER} && {}", true, &empty_vars()).unwrap(),
            "echo admin && {}"
        );
        env::remove_var("BRIDGE_USER");
    }

    #[test]
    fn test_env_vars_hashmap() {
        let mut vars = HashMap::new();
        vars.insert("FILE_VAR".to_string(), "from_file".to_string());

        assert_eq!(
            substitute_env_vars("${FILE_VAR}", true, &vars).unwrap(),
            "from_file"
        );
    }

    #[test]
    fn test_process_env_takes_priority() {
        env::set_var("BRIDGE_PRIORITY_TEST", "from_process");
        let mut vars = HashMap::new();
        vars.insert("BRIDGE_PRIORITY_TEST".to_string(), "from_file".to_string());

        assert_eq!(
            substitute_env_vars("${BRIDGE_PRIORITY_TEST}", true, &vars).unwrap(),
            "from_process"
        );
        env::remove_var("BRIDGE_PRIORITY_TEST");
    }

    #[test]
    fn test_fallback_to_hashmap() {
        // BRIDGE_UNIQUE_VAR_XYZ should not exist in process env
        let mut vars = HashMap::new();
        vars.insert("BRIDGE_UNIQUE_VAR_XYZ".to_string(), "from_hashmap".to_string());

        assert_eq!(
            substitute_env_vars("${BRIDGE_UNIQUE_VAR_XYZ}", true, &vars).unwrap(),
            "from_hashmap"
        );
    }
}
