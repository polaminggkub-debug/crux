use regex::Regex;

use crate::config::types::ReplaceRule;

/// Apply regex replacement rules sequentially to each line of input.
/// Invalid regex patterns are silently skipped.
pub fn apply_replace(input: &str, rules: &[ReplaceRule]) -> String {
    let compiled: Vec<(Regex, &str)> = rules
        .iter()
        .filter_map(|r| {
            Regex::new(&r.pattern)
                .ok()
                .map(|re| (re, r.replacement.as_str()))
        })
        .collect();

    input
        .lines()
        .map(|line| {
            let mut result = line.to_string();
            for (re, replacement) in &compiled {
                result = re.replace_all(&result, *replacement).into_owned();
            }
            result
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, replacement: &str) -> ReplaceRule {
        ReplaceRule {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
        }
    }

    #[test]
    fn single_pattern_replacement() {
        let input = "hello world\nfoo world";
        let result = apply_replace(input, &[rule("world", "earth")]);
        assert_eq!(result, "hello earth\nfoo earth");
    }

    #[test]
    fn multiple_rules_applied_sequentially() {
        let input = "aaa bbb ccc";
        let rules = vec![rule("aaa", "xxx"), rule("bbb", "yyy")];
        let result = apply_replace(input, &rules);
        assert_eq!(result, "xxx yyy ccc");
    }

    #[test]
    fn invalid_regex_silently_skipped() {
        let input = "hello world";
        let rules = vec![rule("[invalid", "nope"), rule("world", "earth")];
        let result = apply_replace(input, &rules);
        assert_eq!(result, "hello earth");
    }

    #[test]
    fn empty_rules_returns_input_unchanged() {
        let input = "hello world\nfoo bar";
        let result = apply_replace(input, &[]);
        assert_eq!(result, input);
    }

    #[test]
    fn capture_groups_in_replacement() {
        let input = "2024-01-15 event happened";
        let result = apply_replace(input, &[rule(r"(\d{4})-(\d{2})-(\d{2})", "$2/$3/$1")]);
        assert_eq!(result, "01/15/2024 event happened");
    }
}
