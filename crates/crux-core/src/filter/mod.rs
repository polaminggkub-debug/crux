pub mod builtin;
pub mod cleanup;
pub mod skip;

use crate::config::FilterConfig;

/// Apply a full filter pipeline to command output.
///
/// Pipeline order:
/// 1. Try builtin filter first (unless disabled via `builtin = false`)
/// 2. Strip ANSI codes (if configured)
/// 3. Apply skip/keep line filtering
/// 4. Trim trailing whitespace (if configured)
/// 5. Collapse blank lines (if configured)
pub fn apply_filter(config: &FilterConfig, output: &str, exit_code: i32) -> String {
    let mut result = output.to_string();

    // 1. Try builtin first (if not explicitly disabled)
    if config.builtin != Some(false) {
        if let Some(builtin_fn) = builtin::registry().get(config.command.as_str()) {
            return builtin_fn(&result, exit_code);
        }
    }

    // 2. Strip ANSI escape codes
    if config.strip_ansi == Some(true) {
        result = cleanup::strip_ansi(&result);
    }

    // 3. Skip/keep line filtering
    if !config.skip.is_empty() || !config.keep.is_empty() {
        result = skip::apply_skip_keep(&result, &config.skip, &config.keep);
    }

    // 4. Trim trailing whitespace
    if config.trim_trailing_whitespace == Some(true) {
        result = cleanup::trim_trailing_whitespace(&result);
    }

    // 5. Collapse blank lines
    if config.collapse_blank_lines == Some(true) {
        result = cleanup::collapse_blank_lines(&result);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_filter_passthrough_when_no_rules() {
        let config = FilterConfig::default();
        let input = "hello\nworld";
        assert_eq!(apply_filter(&config, input, 0), "hello\nworld");
    }

    #[test]
    fn apply_filter_skip_lines() {
        let config = FilterConfig {
            skip: vec!["^debug".to_string()],
            ..Default::default()
        };
        let input = "error: bad\ndebug: noise\nwarning: ok";
        let result = apply_filter(&config, input, 0);
        assert_eq!(result, "error: bad\nwarning: ok");
    }

    #[test]
    fn apply_filter_strip_ansi_and_collapse() {
        let config = FilterConfig {
            strip_ansi: Some(true),
            collapse_blank_lines: Some(true),
            ..Default::default()
        };
        let input = "\x1b[31merror\x1b[0m\n\n\n\nok";
        let result = apply_filter(&config, input, 0);
        assert_eq!(result, "error\n\nok");
    }

    #[test]
    fn apply_filter_full_pipeline() {
        let config = FilterConfig {
            strip_ansi: Some(true),
            skip: vec!["^noise".to_string()],
            trim_trailing_whitespace: Some(true),
            collapse_blank_lines: Some(true),
            ..Default::default()
        };
        let input = "\x1b[31merror\x1b[0m   \nnoise line\n\n\n\nwarning  ";
        let result = apply_filter(&config, input, 0);
        assert_eq!(result, "error\n\nwarning");
    }

    #[test]
    fn apply_filter_builtin_git_status() {
        let config = FilterConfig {
            command: "git status".to_string(),
            ..Default::default()
        };
        let output = "On branch main\n\nChanges:\n  (use hint)\n\tM  src/lib.rs";
        let result = apply_filter(&config, output, 0);
        assert!(result.contains("On branch main"));
        assert!(result.contains("M  src/lib.rs"));
    }

    #[test]
    fn apply_filter_builtin_disabled() {
        let config = FilterConfig {
            command: "git status".to_string(),
            builtin: Some(false),
            ..Default::default()
        };
        let output = "On branch main\nsome hint line";
        let result = apply_filter(&config, output, 0);
        // Builtin disabled, no TOML pipeline configured, so passthrough
        assert_eq!(result, output);
    }

    #[test]
    fn apply_filter_toml_pipeline_with_all_stages() {
        let config = FilterConfig {
            command: "custom command".to_string(),
            strip_ansi: Some(true),
            skip: vec!["^#".to_string()],
            trim_trailing_whitespace: Some(true),
            collapse_blank_lines: Some(true),
            ..Default::default()
        };
        let output = "line1  \n# comment\n\x1b[31mcolored\x1b[0m\n\n\n\nline2";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, "line1\ncolored\n\nline2");
    }

    #[test]
    fn apply_filter_unknown_command_passthrough() {
        let config = FilterConfig {
            command: "some-unknown-cmd".to_string(),
            ..Default::default()
        };
        let output = "raw output here";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, output);
    }

    #[test]
    fn apply_filter_skip_and_keep() {
        let config = FilterConfig {
            command: "custom".to_string(),
            keep: vec!["^important".to_string()],
            skip: vec!["ignore".to_string()],
            ..Default::default()
        };
        let output = "important line\nimportant but ignore this\nnot important";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, "important line");
    }
}
