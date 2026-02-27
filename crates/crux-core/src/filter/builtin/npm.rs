use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register npm handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("npm test", filter_npm_test as BuiltinFilterFn);
    m.insert("npm install", filter_npm_install as BuiltinFilterFn);
    m.insert("npm run build", filter_npm_build as BuiltinFilterFn);
}

/// Filter npm test output: show pass/fail summary. On failure, show failing test names.
pub fn filter_npm_test(output: &str, exit_code: i32) -> String {
    let mut summary_lines = Vec::new();
    let mut failed_tests: Vec<String> = Vec::new();

    // Common test runner patterns
    let jest_summary_re = Regex::new(r"(?i)^(Tests?|Test Suites?):").unwrap();
    let jest_pass_re = Regex::new(r"(?i)(PASS|FAIL)\s+\S+").unwrap();
    let vitest_summary_re = Regex::new(r"(?i)^\s*(Tests?|Test Files?)\s+").unwrap();
    let fail_re = Regex::new(r"(?i)(FAIL|FAILED|failing|failed)\s+(.+)").unwrap();
    let error_re = Regex::new(r"(?i)^\s*(Error|ERR!|✕|✗|×|FAIL)\s").unwrap();

    for line in output.lines() {
        let trimmed = line.trim();

        // Jest/Vitest summary lines (e.g. "Tests: 3 passed, 1 failed, 4 total")
        if jest_summary_re.is_match(trimmed) || vitest_summary_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // PASS/FAIL lines for individual suites
        if jest_pass_re.is_match(trimmed) {
            // Only keep FAIL lines to save tokens
            if trimmed.contains("FAIL") {
                summary_lines.push(trimmed.to_string());
            }
            continue;
        }

        // Failed test names
        if exit_code != 0 {
            if let Some(caps) = fail_re.captures(trimmed) {
                let test_name = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                if !test_name.is_empty() {
                    failed_tests.push(test_name.to_string());
                }
                continue;
            }

            if error_re.is_match(trimmed) {
                failed_tests.push(trimmed.to_string());
            }
        }
    }

    let mut output_parts = Vec::new();

    if !failed_tests.is_empty() {
        output_parts.push("Failed tests:".to_string());
        // Deduplicate
        failed_tests.dedup();
        for test in &failed_tests {
            output_parts.push(format!("  - {test}"));
        }
        output_parts.push(String::new());
    }

    if !summary_lines.is_empty() {
        for line in &summary_lines {
            output_parts.push(line.clone());
        }
    } else if exit_code == 0 {
        output_parts.push("All tests passed.".to_string());
    } else {
        output_parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    output_parts.join("\n")
}

/// Filter npm install: show summary of added/removed packages.
pub fn filter_npm_install(output: &str, exit_code: i32) -> String {
    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("added ")
            || trimmed.starts_with("removed ")
            || trimmed.starts_with("changed ")
            || trimmed.starts_with("up to date")
            || trimmed.contains("packages in")
            || trimmed.starts_with("npm warn")
            || trimmed.starts_with("npm ERR!")
        {
            lines.push(trimmed.to_string());
        }
    }
    if lines.is_empty() {
        if exit_code == 0 {
            "Installed successfully.".to_string()
        } else {
            format!("Install failed (exit code {exit_code}).")
        }
    } else {
        lines.join("\n")
    }
}

/// Filter npm run build: keep error/warning lines and summary.
pub fn filter_npm_build(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        // Look for build summary lines
        let mut summary = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.contains("compiled successfully")
                || trimmed.contains("Build complete")
                || trimmed.contains("built in")
                || trimmed.starts_with("✓")
                || trimmed.starts_with("✔")
            {
                summary.push(trimmed.to_string());
            }
        }
        if summary.is_empty() {
            "Build completed successfully.".to_string()
        } else {
            summary.join("\n")
        }
    } else {
        let mut lines = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("error")
                || trimmed.starts_with("Error")
                || trimmed.starts_with("ERROR")
                || trimmed.starts_with("npm ERR!")
                || trimmed.contains("Failed to compile")
            {
                lines.push(trimmed.to_string());
            }
        }
        if lines.is_empty() {
            format!("Build failed (exit code {exit_code}).")
        } else {
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_test_jest_success() {
        let input = r#"
> myapp@1.0.0 test
> jest

PASS src/utils.test.js
PASS src/app.test.js

Test Suites: 2 passed, 2 total
Tests:       5 passed, 5 total
Snapshots:   0 total
Time:        1.234 s"#;

        let result = filter_npm_test(input, 0);
        assert!(result.contains("Test Suites: 2 passed"));
        assert!(result.contains("Tests:       5 passed"));
        assert!(!result.contains("> myapp"));
        assert!(!result.contains("PASS src/utils")); // PASS lines are excluded
    }

    #[test]
    fn npm_test_jest_failure() {
        let input = r#"
> myapp@1.0.0 test
> jest

PASS src/utils.test.js
FAIL src/app.test.js
  ● should render correctly
    expect(received).toBe(expected)

Test Suites: 1 failed, 1 passed, 2 total
Tests:       1 failed, 4 passed, 5 total"#;

        let result = filter_npm_test(input, 1);
        assert!(result.contains("FAIL src/app.test.js"));
        assert!(result.contains("Test Suites: 1 failed"));
        assert!(result.contains("Tests:       1 failed"));
    }

    #[test]
    fn npm_test_no_output() {
        let result = filter_npm_test("", 0);
        assert_eq!(result, "All tests passed.");
    }

    #[test]
    fn npm_test_failure_no_summary() {
        let result = filter_npm_test("some random output\nnpm ERR! code 1", 1);
        assert!(result.contains("Tests failed (exit code 1)"));
    }
}
