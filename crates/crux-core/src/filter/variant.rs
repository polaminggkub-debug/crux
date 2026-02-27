use regex::Regex;

use crate::config::types::FilterConfig;

/// Pre-execution variant detection: checks filesystem markers.
///
/// Iterates variant rules and returns the filter name of the first rule
/// whose `detect_file` exists in the current working directory.
pub fn detect_variant_pre(config: &FilterConfig) -> Option<String> {
    for v in &config.variant {
        if let Some(ref file) = v.detect_file {
            if std::path::Path::new(file).exists() {
                return v.filter.clone();
            }
        }
    }
    None
}

/// Post-execution variant detection: matches output against regex patterns.
///
/// Iterates variant rules and returns the filter name of the first rule
/// whose `detect_output` regex matches anywhere in the given output.
/// Invalid regex patterns are silently skipped.
pub fn detect_variant_post(config: &FilterConfig, output: &str) -> Option<String> {
    for v in &config.variant {
        if let Some(ref pattern) = v.detect_output {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(output) {
                    return v.filter.clone();
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::VariantRule;

    fn make_config(variants: Vec<VariantRule>) -> FilterConfig {
        FilterConfig {
            command: "test".to_string(),
            variant: variants,
            ..default_filter_config()
        }
    }

    fn default_filter_config() -> FilterConfig {
        toml::from_str("command = \"test\"").unwrap()
    }

    fn variant_file(name: &str, file: &str, filter: &str) -> VariantRule {
        VariantRule {
            name: name.to_string(),
            detect_file: Some(file.to_string()),
            detect_output: None,
            filter: Some(filter.to_string()),
        }
    }

    fn variant_output(name: &str, pattern: &str, filter: &str) -> VariantRule {
        VariantRule {
            name: name.to_string(),
            detect_file: None,
            detect_output: Some(pattern.to_string()),
            filter: Some(filter.to_string()),
        }
    }

    #[test]
    fn pre_detect_existing_file() {
        // Cargo.toml exists at workspace root (tests run from workspace root)
        let cfg = make_config(vec![variant_file("cargo", "Cargo.toml", "cargo-build")]);
        assert_eq!(detect_variant_pre(&cfg), Some("cargo-build".to_string()));
    }

    #[test]
    fn pre_detect_missing_file_returns_none() {
        let cfg = make_config(vec![variant_file(
            "nope",
            "nonexistent_file_abc123.xyz",
            "other",
        )]);
        assert_eq!(detect_variant_pre(&cfg), None);
    }

    #[test]
    fn post_detect_matching_output() {
        let cfg = make_config(vec![variant_output("err", r"error\[E\d+\]", "cargo-error")]);
        let output = "error[E0308]: mismatched types";
        assert_eq!(
            detect_variant_post(&cfg, output),
            Some("cargo-error".to_string())
        );
    }

    #[test]
    fn post_detect_no_match_returns_none() {
        let cfg = make_config(vec![variant_output("err", r"FATAL", "fatal-filter")]);
        let output = "Compiling crux v0.1.0\n    Finished dev";
        assert_eq!(detect_variant_post(&cfg, output), None);
    }

    #[test]
    fn first_variant_wins() {
        let cfg = make_config(vec![
            variant_file("first", "Cargo.toml", "first-filter"),
            variant_file("second", "Cargo.toml", "second-filter"),
        ]);
        assert_eq!(detect_variant_pre(&cfg), Some("first-filter".to_string()));

        let cfg2 = make_config(vec![
            variant_output("a", r"hello", "filter-a"),
            variant_output("b", r"hello", "filter-b"),
        ]);
        assert_eq!(
            detect_variant_post(&cfg2, "hello world"),
            Some("filter-a".to_string())
        );
    }
}
