use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Default AI model when none is specified.
const DEFAULT_MODEL: &str = "gpt-4o";

/// Template written by `prism init`.
const CONFIG_TEMPLATE: &str = r#"# Prism configuration file
# See: https://github.com/anomalyco/prism

[github]
# GitHub personal access token (PAT) for API authentication.
# Can also be set via the GITHUB_TOKEN environment variable.
# token = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

[openai]
# OpenAI API key for AI-powered review (used with --ai flag).
# Can also be set via the OPENAI_API_KEY environment variable.
# api_key = "sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

# Default model for AI analysis (optional).
# model = "gpt-4o"
"#;

/// Top-level configuration loaded from `~/.config/prism/config.toml`.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub github: GitHubConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
}

/// GitHub-related configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GitHubConfig {
    /// GitHub personal access token.
    pub token: Option<String>,
}

/// OpenAI-related configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct OpenAiConfig {
    /// OpenAI API key.
    pub api_key: Option<String>,
    /// Default model for AI analysis.
    pub model: Option<String>,
}

impl Config {
    /// Load configuration from `~/.config/prism/config.toml` if it exists.
    ///
    /// Returns a default (empty) config if the file does not exist.
    /// Returns an error only if the file exists but cannot be read or parsed.
    pub fn load() -> Result<Self> {
        let path = config_path();

        if !path.exists() {
            log::debug!("Config file not found at {:?}, using defaults", path);
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file at {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file at {}", path.display()))?;

        log::debug!("Loaded config from {:?}", path);
        Ok(config)
    }

    /// Resolve GitHub token: `GITHUB_TOKEN` env var wins, then config file.
    pub fn github_token(&self) -> Option<String> {
        env::var("GITHUB_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| self.github.token.clone())
            .filter(|value| !value.trim().is_empty())
    }

    /// Resolve OpenAI API key: `OPENAI_API_KEY` env var wins, then config file.
    pub fn openai_api_key(&self) -> Option<String> {
        env::var("OPENAI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| self.openai.api_key.clone())
            .filter(|value| !value.trim().is_empty())
    }

    /// Resolve default model: config file value or built-in default.
    pub fn default_model(&self) -> &str {
        self.openai
            .model
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(DEFAULT_MODEL)
    }
}

/// Returns the path to the config file: `~/.config/prism/config.toml`.
pub fn config_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("prism")
        .join("config.toml")
}

/// Initialize a new config file at `~/.config/prism/config.toml`.
///
/// Creates parent directories if needed. Fails if the file already exists
/// (to avoid overwriting user configuration).
pub fn init_config() -> Result<PathBuf> {
    let path = config_path();

    if path.exists() {
        bail!(
            "Config file already exists at {}.\n\
             Edit it directly or remove it first to reinitialize.",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create config directory at {}", parent.display())
        })?;
    }

    let mut file = fs::File::create(&path)
        .with_context(|| format!("Failed to create config file at {}", path.display()))?;

    file.write_all(CONFIG_TEMPLATE.as_bytes())
        .with_context(|| format!("Failed to write config file at {}", path.display()))?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_parse_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.github.token.is_none());
        assert!(config.openai.api_key.is_none());
        assert!(config.openai.model.is_none());
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
            [github]
            token = "ghp_test_token"

            [openai]
            api_key = "sk_test_key"
            model = "gpt-4-turbo"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.github.token.as_deref(), Some("ghp_test_token"));
        assert_eq!(config.openai.api_key.as_deref(), Some("sk_test_key"));
        assert_eq!(config.openai.model.as_deref(), Some("gpt-4-turbo"));
    }

    #[test]
    fn test_default_model_from_config() {
        let toml_str = r#"
            [openai]
            model = "gpt-4-turbo"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_model(), "gpt-4-turbo");
    }

    #[test]
    fn test_default_model_fallback() {
        let config = Config::default();
        assert_eq!(config.default_model(), "gpt-4o");
    }

    #[test]
    fn test_default_model_ignores_empty_string() {
        let toml_str = r#"
            [openai]
            model = "   "
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_model(), "gpt-4o");
    }

    #[test]
    fn test_github_token_env_overrides_config() {
        let toml_str = r#"
            [github]
            token = "config_token"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();

        // Save and set env var
        let original = env::var("GITHUB_TOKEN").ok();
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            env::set_var("GITHUB_TOKEN", "env_token");
        }

        let resolved = config.github_token();
        assert_eq!(resolved.as_deref(), Some("env_token"));

        // Restore
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            match original {
                Some(val) => env::set_var("GITHUB_TOKEN", val),
                None => env::remove_var("GITHUB_TOKEN"),
            }
        }
    }

    #[test]
    fn test_github_token_falls_back_to_config() {
        let toml_str = r#"
            [github]
            token = "config_token"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();

        // Save and unset env var
        let original = env::var("GITHUB_TOKEN").ok();
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            env::remove_var("GITHUB_TOKEN");
        }

        let resolved = config.github_token();
        assert_eq!(resolved.as_deref(), Some("config_token"));

        // Restore
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            if let Some(val) = original {
                env::set_var("GITHUB_TOKEN", val);
            }
        }
    }

    #[test]
    fn test_openai_api_key_env_overrides_config() {
        let toml_str = r#"
            [openai]
            api_key = "config_key"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();

        // Save and set env var
        let original = env::var("OPENAI_API_KEY").ok();
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            env::set_var("OPENAI_API_KEY", "env_key");
        }

        let resolved = config.openai_api_key();
        assert_eq!(resolved.as_deref(), Some("env_key"));

        // Restore
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            match original {
                Some(val) => env::set_var("OPENAI_API_KEY", val),
                None => env::remove_var("OPENAI_API_KEY"),
            }
        }
    }

    #[test]
    fn test_config_path_uses_home() {
        let original = env::var("HOME").ok();
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            env::set_var("HOME", "/test/home");
        }

        let path = config_path();
        assert_eq!(path, PathBuf::from("/test/home/.config/prism/config.toml"));

        // Restore
        // SAFETY: Test code; single-threaded test execution
        unsafe {
            match original {
                Some(val) => env::set_var("HOME", val),
                None => env::remove_var("HOME"),
            }
        }
    }
}
