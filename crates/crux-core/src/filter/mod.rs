pub mod builtin;
pub mod cleanup;
pub mod context;
pub mod dedup;
pub mod extract;
#[cfg(feature = "lua")]
pub mod lua;
pub mod match_output;
pub mod replace;
pub mod section;
pub mod skip;
pub mod tee;
pub mod template;
pub mod variant;

use crate::config::FilterConfig;

/// Apply a full filter pipeline to command output.
///
/// Pipeline order:
///  1. `match_output` — short-circuit if output contains substring
///  2. Builtin — short-circuit if registered handler exists
///  3. Lua — short-circuit if returns Some (feature-gated)
///  4. `strip_ansi` — remove ANSI escape codes
///  5. `replace` — regex substitution
///  6. `skip`/`keep` — line filtering
///  7. `section` — collect sections into context
///  8. `extract` — first regex match → template
///  9. `dedup` — collapse consecutive duplicate lines
/// 10. `template` — render with context vars/sections
/// 11. `trim_trailing_whitespace`
/// 12. `collapse_blank_lines`
pub fn apply_filter(config: &FilterConfig, output: &str, exit_code: i32) -> String {
    // 1. match_output — short-circuit on substring match
    if !config.match_output.is_empty() {
        if let Some(result) = match_output::apply_match_output(output, &config.match_output) {
            return result;
        }
    }

    // 2. Builtin — short-circuit if registered (unless disabled)
    if config.builtin != Some(false) {
        if let Some(builtin_fn) = builtin::registry().get(config.command.as_str()) {
            return builtin_fn(output, exit_code);
        }
    }

    // 3. Lua escape hatch — short-circuit if returns Some
    #[cfg(feature = "lua")]
    {
        if let Some(ref lua_config) = config.lua {
            let lua_result = if let Some(ref source) = lua_config.source {
                lua::apply_lua(source, output, exit_code, &[])
            } else if let Some(ref file) = lua_config.file {
                lua::apply_lua_file(file, output, exit_code, &[])
            } else {
                None
            };
            if let Some(result) = lua_result {
                return result;
            }
        }
    }

    let mut result = output.to_string();
    let mut ctx = context::FilterContext::new(exit_code);

    // 4. Strip ANSI escape codes
    if config.strip_ansi == Some(true) {
        result = cleanup::strip_ansi(&result);
    }

    // 5. Regex replacement
    if !config.replace.is_empty() {
        result = replace::apply_replace(&result, &config.replace);
    }

    // 6. Skip/keep line filtering
    if !config.skip.is_empty() || !config.keep.is_empty() {
        result = skip::apply_skip_keep(&result, &config.skip, &config.keep);
    }

    // 7. Section extraction
    if !config.section.is_empty() {
        result = section::apply_sections(&result, &config.section, &mut ctx);
    }

    // 8. Extract — first regex match → template (short-circuits remaining text stages)
    if !config.extract.is_empty() {
        if let Some(extracted) = extract::apply_extract(&result, &config.extract) {
            result = extracted;
        }
    }

    // 9. Dedup consecutive identical lines
    if config.dedup == Some(true) {
        result = dedup::apply_dedup(&result);
    }

    // 10. Template interpolation
    if let Some(ref tmpl) = config.template {
        result = template::apply_template(tmpl, &ctx);
    }

    // 11. Trim trailing whitespace
    if config.trim_trailing_whitespace == Some(true) {
        result = cleanup::trim_trailing_whitespace(&result);
    }

    // 12. Collapse blank lines
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

    #[test]
    fn apply_filter_match_output_short_circuits() {
        use crate::config::types::MatchOutputRule;
        let config = FilterConfig {
            command: "custom".to_string(),
            match_output: vec![MatchOutputRule {
                contains: "FATAL".to_string(),
                template: Some("Build crashed!".to_string()),
            }],
            skip: vec!["^".to_string()], // Would remove everything, but match_output fires first
            ..Default::default()
        };
        let output = "line1\nFATAL error\nline3";
        let result = apply_filter(&config, output, 1);
        assert_eq!(result, "Build crashed!");
    }

    #[test]
    fn apply_filter_replace_stage() {
        use crate::config::types::ReplaceRule;
        let config = FilterConfig {
            command: "custom".to_string(),
            replace: vec![ReplaceRule {
                pattern: r"\d{4}-\d{2}-\d{2}".to_string(),
                replacement: "DATE".to_string(),
            }],
            ..Default::default()
        };
        let output = "Log 2024-01-15: something happened";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, "Log DATE: something happened");
    }

    #[test]
    fn apply_filter_dedup_stage() {
        let config = FilterConfig {
            command: "custom".to_string(),
            dedup: Some(true),
            ..Default::default()
        };
        let output = "line1\nline1\nline1\nline2\nline2\nline3";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn apply_filter_extract_stage() {
        use crate::config::types::ExtractRule;
        let config = FilterConfig {
            command: "custom".to_string(),
            extract: vec![ExtractRule {
                pattern: r"result: (\w+)".to_string(),
                template: Some("Status: {1}".to_string()),
            }],
            ..Default::default()
        };
        let output = "noise\nresult: success\nmore noise";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, "Status: success");
    }

    #[test]
    fn apply_filter_full_toml_pipeline() {
        use crate::config::types::ReplaceRule;
        let config = FilterConfig {
            command: "custom".to_string(),
            strip_ansi: Some(true),
            replace: vec![ReplaceRule {
                pattern: r"timestamp=\d+".to_string(),
                replacement: "timestamp=X".to_string(),
            }],
            skip: vec!["^#".to_string()],
            dedup: Some(true),
            trim_trailing_whitespace: Some(true),
            collapse_blank_lines: Some(true),
            ..Default::default()
        };
        let output = "\x1b[31m# comment\x1b[0m\ntimestamp=123 msg  \ntimestamp=123 msg  \n\n\n\nok";
        let result = apply_filter(&config, output, 0);
        assert_eq!(result, "timestamp=X msg\n\nok");
    }
}
