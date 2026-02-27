use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register Go tool handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("go build", filter_go_build as BuiltinFilterFn);
    m.insert("golangci-lint", filter_golangci_lint as BuiltinFilterFn);
}

/// Filter go build output: on success "Build successful." On failure keep error lines.
pub fn filter_go_build(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        return "Build successful.".to_string();
    }

    let error_re = Regex::new(r"^\S+\.go:\d+:\d+:").unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // file.go:line:col: error message
        if error_re.is_match(trimmed) {
            lines.push(trimmed.to_string());
            continue;
        }

        // Package path header (e.g. "# mypackage")
        if trimmed.starts_with("# ") {
            lines.push(trimmed.to_string());
            continue;
        }

        // Keep linker errors or other fatal messages
        if trimmed.contains("undefined:") || trimmed.starts_with("cannot ") {
            lines.push(trimmed.to_string());
            continue;
        }
    }

    if lines.is_empty() {
        format!("Build failed (exit code {exit_code}).")
    } else {
        lines.join("\n")
    }
}

/// Filter golangci-lint output: keep file:line:col linter-name lines and summary.
/// Drop decorative lines and progress indicators.
pub fn filter_golangci_lint(output: &str, exit_code: i32) -> String {
    let diag_re = Regex::new(r"^\S+\.go:\d+:\d+:").unwrap();
    let summary_re = Regex::new(r"^\d+ issue").unwrap();

    let mut diag_lines = Vec::new();
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // file.go:line:col: message (linter-name)
        if diag_re.is_match(trimmed) {
            diag_lines.push(trimmed.to_string());
            continue;
        }

        // Summary line like "3 issues found"
        if summary_re.is_match(trimmed) || trimmed.starts_with("level=") {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // Skip decorative lines, progress, timing info
    }

    let mut result = diag_lines;
    if !summary_lines.is_empty() {
        if !result.is_empty() {
            result.push(String::new());
        }
        result.extend(summary_lines);
    }

    if result.is_empty() {
        if exit_code == 0 {
            "No issues found.".to_string()
        } else {
            format!("golangci-lint failed (exit code {exit_code}).")
        }
    } else {
        result.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- go build tests --

    #[test]
    fn go_build_success() {
        let result = filter_go_build("", 0);
        assert_eq!(result, "Build successful.");
    }

    #[test]
    fn go_build_errors() {
        let input = r#"# mypackage
./main.go:10:5: undefined: foo
./main.go:15:12: cannot use x (variable of type string) as int value"#;

        let result = filter_go_build(input, 2);
        assert!(result.contains("# mypackage"));
        assert!(result.contains("./main.go:10:5: undefined: foo"));
        assert!(result.contains("./main.go:15:12: cannot use"));
    }

    #[test]
    fn go_build_failure_no_recognized_lines() {
        let result = filter_go_build("some unexpected linker output", 1);
        assert_eq!(result, "Build failed (exit code 1).");
    }

    #[test]
    fn go_build_imported_not_used() {
        let input = r#"# command-line-arguments
./main.go:8:2: imported and not used: "fmt"
./main.go:12:9: undefined: bar"#;

        let result = filter_go_build(input, 2);
        assert!(result.contains("# command-line-arguments"));
        assert!(result.contains("imported and not used"));
        assert!(result.contains("undefined: bar"));
    }

    // -- golangci-lint tests --

    #[test]
    fn golangci_lint_clean() {
        let result = filter_golangci_lint("", 0);
        assert_eq!(result, "No issues found.");
    }

    #[test]
    fn golangci_lint_issues() {
        let input = r#"main.go:10:5: SA1006: printf-style function with dynamic format string (staticcheck)
main.go:22:2: ineffectual assignment to `err` (ineffassign)
utils.go:5:1: `doStuff` is unused (deadcode)

3 issues found"#;

        let result = filter_golangci_lint(input, 1);
        assert!(result.contains("main.go:10:5: SA1006"));
        assert!(result.contains("main.go:22:2: ineffectual"));
        assert!(result.contains("utils.go:5:1:"));
        assert!(result.contains("3 issues found"));
    }

    #[test]
    fn golangci_lint_with_level_warning() {
        let input = r#"level=warning msg="some internal warning"
main.go:10:5: exported function Foo should have comment (golint)
1 issues found"#;

        let result = filter_golangci_lint(input, 1);
        assert!(result.contains("main.go:10:5:"));
        assert!(result.contains("1 issues found"));
    }

    #[test]
    fn golangci_lint_failure_unrecognized() {
        let result = filter_golangci_lint("panic: runtime error", 2);
        assert_eq!(result, "golangci-lint failed (exit code 2).");
    }
}
