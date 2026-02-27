use regex::Regex;

/// Remove lines matching any skip pattern. If keep patterns exist, only keep matching lines.
/// Keep takes priority: if both keep and skip are non-empty, keep is applied first,
/// then skip removes from the kept lines.
pub fn apply_skip_keep(input: &str, skip: &[String], keep: &[String]) -> String {
    let keep_regexes: Vec<Regex> = keep.iter().filter_map(|p| Regex::new(p).ok()).collect();
    let skip_regexes: Vec<Regex> = skip.iter().filter_map(|p| Regex::new(p).ok()).collect();

    let lines: Vec<&str> = input.lines().collect();
    let filtered: Vec<&str> = lines
        .into_iter()
        .filter(|line| {
            // If keep patterns exist, line must match at least one
            if !keep_regexes.is_empty() && !keep_regexes.iter().any(|r| r.is_match(line)) {
                return false;
            }
            // If skip patterns exist, line must not match any
            if !skip_regexes.is_empty() && skip_regexes.iter().any(|r| r.is_match(line)) {
                return false;
            }
            true
        })
        .collect();

    filtered.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_removes_matching_lines() {
        let input = "hello\nworld\nfoo bar\nbaz";
        let result = apply_skip_keep(input, &["^foo".to_string()], &[]);
        assert_eq!(result, "hello\nworld\nbaz");
    }

    #[test]
    fn keep_retains_only_matching_lines() {
        let input = "error: something\nwarning: stuff\ninfo: ok\nerror: another";
        let result = apply_skip_keep(input, &[], &["^error".to_string()]);
        assert_eq!(result, "error: something\nerror: another");
    }

    #[test]
    fn keep_takes_priority_over_skip() {
        // Keep "error" lines, but skip lines containing "ignore"
        let input = "error: real problem\nerror: ignore this\ninfo: hello\nerror: also real";
        let result = apply_skip_keep(input, &["ignore".to_string()], &["^error".to_string()]);
        assert_eq!(result, "error: real problem\nerror: also real");
    }

    #[test]
    fn empty_patterns_returns_input_unchanged() {
        let input = "line1\nline2\nline3";
        let result = apply_skip_keep(input, &[], &[]);
        assert_eq!(result, input);
    }

    #[test]
    fn multiple_skip_patterns() {
        let input = "alpha\nbeta\ngamma\ndelta";
        let result = apply_skip_keep(input, &["alpha".to_string(), "gamma".to_string()], &[]);
        assert_eq!(result, "beta\ndelta");
    }

    #[test]
    fn invalid_regex_is_ignored() {
        let input = "hello\nworld";
        // Invalid regex pattern should be silently skipped
        let result = apply_skip_keep(input, &["[invalid".to_string()], &[]);
        assert_eq!(result, "hello\nworld");
    }
}
