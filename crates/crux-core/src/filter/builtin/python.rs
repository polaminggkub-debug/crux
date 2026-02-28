use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register Python tool handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("ruff check", filter_ruff_check as BuiltinFilterFn);
    m.insert("ruff", filter_ruff_check as BuiltinFilterFn);
    m.insert("pip install", filter_pip_install as BuiltinFilterFn);
    m.insert("mypy", filter_mypy as BuiltinFilterFn);
    m.insert("pyright", filter_pyright as BuiltinFilterFn);
}

/// Filter ruff check output: keep file:line:col error lines and summary.
/// Drop "Found N errors" if a fixable count line is already shown.
pub fn filter_ruff_check(output: &str, exit_code: i32) -> String {
    if exit_code == 0 && output.trim().is_empty() {
        return "All checks passed.".to_string();
    }

    let diag_re = Regex::new(r"^\S+:\d+:\d+:\s+\w+").unwrap();
    let found_re = Regex::new(r"^Found \d+ error").unwrap();
    let fixable_re = Regex::new(r"\d+ (potentially )?fixable").unwrap();

    let mut diag_lines = Vec::new();
    let mut summary_lines = Vec::new();
    let mut has_fixable = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // file:line:col: CODE description
        if diag_re.is_match(trimmed) {
            diag_lines.push(trimmed.to_string());
            continue;
        }

        // Fixable count line
        if fixable_re.is_match(trimmed) || trimmed.contains("fixable with") {
            has_fixable = true;
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // "Found N errors" line
        if found_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }
    }

    // If fixable count is shown, drop the redundant "Found N errors" line
    if has_fixable {
        summary_lines.retain(|l| !found_re.is_match(l));
    }

    let mut result = diag_lines;
    if !summary_lines.is_empty() {
        result.push(String::new());
        result.extend(summary_lines);
    }

    if result.is_empty() {
        if exit_code == 0 {
            "All checks passed.".to_string()
        } else {
            format!("Ruff check failed (exit code {exit_code}).")
        }
    } else {
        result.join("\n")
    }
}

/// Filter pip install output: keep "Successfully installed" line.
/// Drop download progress, "Collecting", "Using cached". On error keep error lines.
pub fn filter_pip_install(output: &str, exit_code: i32) -> String {
    let mut result_lines = Vec::new();
    let mut error_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Successfully installed") {
            result_lines.push(trimmed.to_string());
            continue;
        }

        if trimmed.starts_with("Requirement already satisfied") && result_lines.is_empty() {
            result_lines.push(trimmed.to_string());
            continue;
        }

        // Keep error/warning lines
        if trimmed.starts_with("ERROR:")
            || trimmed.starts_with("error:")
            || trimmed.starts_with("Could not")
            || trimmed.starts_with("No matching distribution")
        {
            error_lines.push(trimmed.to_string());
            continue;
        }

        // Skip: Collecting, Downloading, Using cached, progress bars
    }

    if !error_lines.is_empty() {
        return error_lines.join("\n");
    }

    if result_lines.is_empty() {
        if exit_code == 0 {
            "Install completed.".to_string()
        } else {
            format!("pip install failed (exit code {exit_code}).")
        }
    } else {
        result_lines.join("\n")
    }
}

/// Filter mypy output: keep error/note lines and summary.
/// On success with no errors, return a short summary.
pub fn filter_mypy(output: &str, exit_code: i32) -> String {
    if exit_code == 0 && output.trim().is_empty() {
        return "No type errors found.".to_string();
    }

    let mut diag_lines = Vec::new();
    let mut summary_line: Option<String> = None;
    let mut last_was_error = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary line: "Found N errors in M files" or "Success: no issues found"
        if trimmed.starts_with("Found ") && trimmed.contains("error") {
            summary_line = Some(trimmed.to_string());
            continue;
        }
        if trimmed.starts_with("Success:") {
            summary_line = Some(trimmed.to_string());
            continue;
        }

        // Error lines: file.py:10: error: Something
        if trimmed.contains(": error:") {
            diag_lines.push(trimmed.to_string());
            last_was_error = true;
            continue;
        }

        // Note lines following an error provide context
        if trimmed.contains(": note:") && last_was_error {
            diag_lines.push(trimmed.to_string());
            continue;
        }

        // Any other line resets the "following an error" state
        last_was_error = false;
    }

    let mut result = diag_lines;
    if let Some(summary) = summary_line {
        if !result.is_empty() {
            result.push(String::new());
        }
        result.push(summary);
    }

    if result.is_empty() {
        if exit_code == 0 {
            "No type errors found.".to_string()
        } else {
            format!("mypy failed (exit code {exit_code}).")
        }
    } else {
        result.join("\n")
    }
}

/// Filter pyright output: keep error/warning lines and summary.
pub fn filter_pyright(output: &str, exit_code: i32) -> String {
    if exit_code == 0 && output.trim().is_empty() {
        return "No type errors found.".to_string();
    }

    let mut diag_lines = Vec::new();
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Pyright summary lines: "N errors, N warnings, N informations"
        // or "0 errors, 0 warnings, 0 informations"
        if (trimmed.contains("error") || trimmed.contains("warning"))
            && trimmed.contains("information")
        {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // "Completed in N.Ns" line
        if trimmed.starts_with("Completed in ") {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // Diagnostic lines with "error:" or "warning:"
        if trimmed.contains(" - error:") || trimmed.contains(" - warning:") {
            diag_lines.push(trimmed.to_string());
            continue;
        }

        // Also match pyright's file:line:col format
        if trimmed.contains(": error:") || trimmed.contains(": warning:") {
            diag_lines.push(trimmed.to_string());
            continue;
        }
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
            "No type errors found.".to_string()
        } else {
            format!("pyright failed (exit code {exit_code}).")
        }
    } else {
        result.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ruff check tests --

    #[test]
    fn ruff_check_clean() {
        let result = filter_ruff_check("", 0);
        assert_eq!(result, "All checks passed.");
    }

    #[test]
    fn ruff_check_errors() {
        let input = r#"src/main.py:10:1: E302 expected 2 blank lines, got 1
src/main.py:25:80: E501 line too long (95 > 79 characters)
src/utils.py:3:1: F401 `os` imported but unused
Found 3 errors."#;

        let result = filter_ruff_check(input, 1);
        assert!(result.contains("src/main.py:10:1: E302"));
        assert!(result.contains("src/utils.py:3:1: F401"));
        assert!(result.contains("Found 3 errors"));
    }

    #[test]
    fn ruff_check_drops_found_when_fixable_shown() {
        let input = r#"src/main.py:10:1: E302 expected 2 blank lines, got 1
src/main.py:25:80: E501 line too long
Found 2 errors.
2 potentially fixable with the `--fix` option."#;

        let result = filter_ruff_check(input, 1);
        assert!(result.contains("src/main.py:10:1: E302"));
        assert!(result.contains("potentially fixable"));
        assert!(!result.contains("Found 2 errors"));
    }

    #[test]
    fn ruff_check_failure_no_diags() {
        let result = filter_ruff_check("some unexpected output", 2);
        assert_eq!(result, "Ruff check failed (exit code 2).");
    }

    // -- pip install tests --

    #[test]
    fn pip_install_success() {
        let input = r#"Collecting requests>=2.28
  Downloading requests-2.31.0-py3-none-any.whl (62 kB)
     ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 62.6/62.6 kB 1.2 MB/s eta 0:00:00
Collecting urllib3<3,>=1.21.1
  Using cached urllib3-2.1.0-py3-none-any.whl
Installing collected packages: urllib3, requests
Successfully installed requests-2.31.0 urllib3-2.1.0"#;

        let result = filter_pip_install(input, 0);
        assert_eq!(
            result,
            "Successfully installed requests-2.31.0 urllib3-2.1.0"
        );
        assert!(!result.contains("Collecting"));
        assert!(!result.contains("Downloading"));
        assert!(!result.contains("Using cached"));
    }

    #[test]
    fn pip_install_error() {
        let input = r#"Collecting nonexistent-package
ERROR: Could not find a version that satisfies the requirement nonexistent-package
ERROR: No matching distribution found for nonexistent-package"#;

        let result = filter_pip_install(input, 1);
        assert!(result.contains("ERROR: Could not find"));
        assert!(!result.contains("Collecting"));
    }

    #[test]
    fn pip_install_already_satisfied() {
        let input = "Requirement already satisfied: requests in ./venv/lib/python3.11/site-packages (2.31.0)";
        let result = filter_pip_install(input, 0);
        assert!(result.contains("Requirement already satisfied"));
    }

    #[test]
    fn pip_install_empty_success() {
        let result = filter_pip_install("", 0);
        assert_eq!(result, "Install completed.");
    }

    // -- mypy tests --

    #[test]
    fn mypy_clean() {
        let input = "Success: no issues found in 5 source files";
        let result = filter_mypy(input, 0);
        assert!(result.contains("Success: no issues found"));
    }

    #[test]
    fn mypy_empty_success() {
        let result = filter_mypy("", 0);
        assert_eq!(result, "No type errors found.");
    }

    #[test]
    fn mypy_errors() {
        let input = r#"src/app.py:10: error: Incompatible return value type (got "str", expected "int")
src/app.py:10: note: See https://mypy.readthedocs.io/...
src/utils.py:25: error: Argument 1 to "foo" has incompatible type "str"; expected "int"
Some other output line
Found 2 errors in 2 files (checked 10 source files)"#;

        let result = filter_mypy(input, 1);
        assert!(result.contains("src/app.py:10: error:"));
        assert!(result.contains("src/app.py:10: note:"));
        assert!(result.contains("src/utils.py:25: error:"));
        assert!(result.contains("Found 2 errors"));
        assert!(!result.contains("Some other output line"));
    }

    #[test]
    fn mypy_note_only_after_error() {
        let input = r#"src/app.py:5: note: Standalone note without error
src/app.py:10: error: Bad type
src/app.py:10: note: Context for the error"#;

        let result = filter_mypy(input, 1);
        // The standalone note (not following an error) should be dropped
        assert!(!result.contains("Standalone note"));
        assert!(result.contains("src/app.py:10: error:"));
        assert!(result.contains("src/app.py:10: note: Context"));
    }

    #[test]
    fn mypy_failure_no_diags() {
        let result = filter_mypy("unexpected output", 2);
        assert_eq!(result, "mypy failed (exit code 2).");
    }

    // -- pyright tests --

    #[test]
    fn pyright_clean() {
        let result = filter_pyright("", 0);
        assert_eq!(result, "No type errors found.");
    }

    #[test]
    fn pyright_errors() {
        let input = r#"Loading pyright configuration...
  /home/user/src/app.py:10:5 - error: Cannot assign type "str" to type "int"
  /home/user/src/utils.py:3:1 - warning: Import "os" is unused
  Loading config from pyproject.toml
1 error, 1 warning, 0 informations
Completed in 1.5s"#;

        let result = filter_pyright(input, 1);
        assert!(result.contains("- error:"));
        assert!(result.contains("- warning:"));
        assert!(result.contains("1 error, 1 warning, 0 informations"));
        assert!(result.contains("Completed in 1.5s"));
        assert!(!result.contains("Loading pyright"));
        assert!(!result.contains("Loading config"));
    }

    #[test]
    fn pyright_colon_format() {
        let input = "src/app.py:10:5: error: Type mismatch\n0 errors, 0 warnings, 0 informations";
        let result = filter_pyright(input, 0);
        assert!(result.contains("src/app.py:10:5: error:"));
        assert!(result.contains("0 errors"));
    }

    #[test]
    fn pyright_failure_no_diags() {
        let result = filter_pyright("unexpected output", 2);
        assert_eq!(result, "pyright failed (exit code 2).");
    }
}
