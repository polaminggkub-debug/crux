use serde::{Deserialize, Serialize};

/// Top-level filter configuration, backward-compatible with tokf TOML format.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterConfig {
    /// Command pattern to match (e.g. "git status", "cargo test").
    pub command: String,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub priority: i32,

    #[serde(default)]
    pub builtin: Option<bool>,

    // -- Skip/keep line filtering --
    #[serde(default)]
    pub skip: Vec<String>,
    #[serde(default)]
    pub keep: Vec<String>,

    // -- Regex replacement --
    #[serde(default)]
    pub replace: Vec<ReplaceRule>,

    // -- Section parsing --
    #[serde(default)]
    pub section: Vec<SectionRule>,

    // -- Extract patterns --
    #[serde(default)]
    pub extract: Vec<ExtractRule>,

    // -- Dedup --
    #[serde(default)]
    pub dedup: Option<bool>,

    // -- Template --
    #[serde(default)]
    pub template: Option<String>,

    // -- Cleanup --
    #[serde(default)]
    pub strip_ansi: Option<bool>,
    #[serde(default)]
    pub trim_trailing_whitespace: Option<bool>,
    #[serde(default)]
    pub collapse_blank_lines: Option<bool>,

    // -- Match output --
    #[serde(default)]
    pub match_output: Vec<MatchOutputRule>,

    // -- Variants --
    #[serde(default)]
    pub variant: Vec<VariantRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceRule {
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionRule {
    pub start: String,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub keep: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRule {
    pub pattern: String,
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchOutputRule {
    pub contains: String,
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantRule {
    pub name: String,
    #[serde(default)]
    pub detect_file: Option<String>,
    #[serde(default)]
    pub detect_output: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
command = "git status"
"#;
        let config: FilterConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.command, "git status");
        assert_eq!(config.priority, 0);
        assert!(config.skip.is_empty());
        assert!(config.description.is_none());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
command = "cargo test"
description = "Filter cargo test output"
priority = 10
builtin = true
skip = ["^\\s*$", "^Compiling"]
keep = ["^error", "^warning"]
dedup = true
strip_ansi = true
trim_trailing_whitespace = true
collapse_blank_lines = true
template = "Tests: {{passed}} passed, {{failed}} failed"

[[replace]]
pattern = "\\x1b\\[[0-9;]*m"
replacement = ""

[[section]]
start = "^failures:"
end = "^$"
keep = true

[[extract]]
pattern = "test result: (\\w+)"
template = "Result: {{1}}"

[[match_output]]
contains = "FAILED"
template = "Build failed!"

[[variant]]
name = "nextest"
detect_file = ".config/nextest.toml"
filter = "cargo/test-nextest"
"#;
        let config: FilterConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.command, "cargo test");
        assert_eq!(config.priority, 10);
        assert_eq!(config.builtin, Some(true));
        assert_eq!(config.skip.len(), 2);
        assert_eq!(config.keep.len(), 2);
        assert_eq!(config.replace.len(), 1);
        assert_eq!(config.replace[0].replacement, "");
        assert_eq!(config.section.len(), 1);
        assert_eq!(config.section[0].keep, Some(true));
        assert_eq!(config.extract.len(), 1);
        assert_eq!(config.match_output.len(), 1);
        assert_eq!(config.match_output[0].contains, "FAILED");
        assert_eq!(config.variant.len(), 1);
        assert_eq!(config.variant[0].name, "nextest");
        assert_eq!(
            config.variant[0].detect_file,
            Some(".config/nextest.toml".to_string())
        );
        assert!(config.dedup == Some(true));
        assert!(config.strip_ansi == Some(true));
    }

    #[test]
    fn parse_config_with_multiple_replace_rules() {
        let toml_str = r#"
command = "git diff"

[[replace]]
pattern = "^index [a-f0-9]+\\.\\.[a-f0-9]+"
replacement = ""

[[replace]]
pattern = "^diff --git"
replacement = "--- Changes ---"
"#;
        let config: FilterConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.command, "git diff");
        assert_eq!(config.replace.len(), 2);
        assert_eq!(config.replace[1].replacement, "--- Changes ---");
    }
}
