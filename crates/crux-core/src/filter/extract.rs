use regex::Regex;

use crate::config::types::ExtractRule;

/// First-match regex extraction with optional template interpolation.
///
/// Returns `Some(result)` if any rule matches a line, `None` otherwise.
pub fn apply_extract(input: &str, rules: &[ExtractRule]) -> Option<String> {
    for rule in rules {
        let re = match Regex::new(&rule.pattern) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for line in input.lines() {
            if let Some(caps) = re.captures(line) {
                return Some(match &rule.template {
                    Some(tmpl) => interpolate(tmpl, &caps),
                    None => line.to_string(),
                });
            }
        }
    }
    None
}

fn interpolate(template: &str, caps: &regex::Captures) -> String {
    let mut result = template.to_string();
    // Replace in reverse order so `{10}` is replaced before `{1}`.
    for i in (1..caps.len()).rev() {
        if let Some(m) = caps.get(i) {
            result = result.replace(&format!("{{{i}}}"), m.as_str());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, template: Option<&str>) -> ExtractRule {
        ExtractRule {
            pattern: pattern.to_string(),
            template: template.map(String::from),
        }
    }

    #[test]
    fn simple_match_returns_line() {
        let input = "foo\nerror: something broke\nbar";
        let result = apply_extract(input, &[rule(r"^error:", None)]);
        assert_eq!(result, Some("error: something broke".into()));
    }

    #[test]
    fn template_with_capture_group() {
        let input = "version = 1.2.3";
        let rules = [rule(r"version = (\S+)", Some("v{1}"))];
        assert_eq!(apply_extract(input, &rules), Some("v1.2.3".into()));
    }

    #[test]
    fn multiple_rules_first_match_wins() {
        let input = "warning: low disk\nerror: crash";
        let rules = [
            rule(r"^error:", Some("ERR")),
            rule(r"^warning:", Some("WARN")),
        ];
        // "warning" line comes first, but rule 0 (error) is checked first per-rule.
        // Rule 0 scans all lines — matches "error: crash".
        assert_eq!(apply_extract(input, &rules), Some("ERR".into()));
    }

    #[test]
    fn no_match_returns_none() {
        let input = "all good\nnothing here";
        assert_eq!(apply_extract(input, &[rule(r"^fatal:", None)]), None);
    }

    #[test]
    fn template_with_multiple_captures() {
        let input = "2026-02-28 host=web req=42ms";
        let rules = [rule(r"host=(\S+) req=(\S+)", Some("{1} took {2}"))];
        assert_eq!(apply_extract(input, &rules), Some("web took 42ms".into()));
    }
}
