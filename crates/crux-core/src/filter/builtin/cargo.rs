use regex::Regex;

/// Filter cargo test output: show summary, on failure show failing tests + errors.
pub fn filter_cargo_test(output: &str, exit_code: i32) -> String {
    let mut result_lines = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut in_failures_section = false;
    let mut current_failure: Vec<String> = Vec::new();

    let test_result_re = Regex::new(r"^test result:").unwrap();
    let test_line_re = Regex::new(r"^test\s+\S+\s+\.\.\.\s+\w+").unwrap();

    for line in output.lines() {
        let trimmed = line.trim();

        // Capture "test result:" summary lines
        if test_result_re.is_match(trimmed) {
            result_lines.push(trimmed.to_string());
            in_failures_section = false;
            continue;
        }

        // Detect failures section
        if trimmed == "failures:" {
            in_failures_section = true;
            continue;
        }

        if trimmed == "failures:" || trimmed == "---- failures ----" {
            in_failures_section = true;
            continue;
        }

        if in_failures_section {
            // End of failures section
            if trimmed.starts_with("test result:") || trimmed == "successes:" {
                if !current_failure.is_empty() {
                    failures.push(current_failure.join("\n"));
                    current_failure.clear();
                }
                in_failures_section = false;
                if trimmed.starts_with("test result:") {
                    result_lines.push(trimmed.to_string());
                }
                continue;
            }

            // Failure test name header
            if trimmed.starts_with("---- ") && trimmed.ends_with(" ----") {
                if !current_failure.is_empty() {
                    failures.push(current_failure.join("\n"));
                }
                current_failure = vec![trimmed.to_string()];
                continue;
            }

            // Failure content — keep assertion/panic lines
            if !trimmed.is_empty()
                && (trimmed.contains("panicked at")
                    || trimmed.contains("assertion")
                    || trimmed.starts_with("thread")
                    || trimmed.starts_with("left:")
                    || trimmed.starts_with("right:")
                    || trimmed.contains("called `Result::unwrap()`")
                    || trimmed.contains("expected"))
            {
                current_failure.push(format!("  {trimmed}"));
            }
            continue;
        }

        // Keep individual test FAILED lines
        if test_line_re.is_match(trimmed) && trimmed.contains("FAILED") {
            result_lines.push(trimmed.to_string());
        }

        // Skip compilation output (Compiling, Downloading, etc.)
    }

    // Flush any remaining failure
    if !current_failure.is_empty() {
        failures.push(current_failure.join("\n"));
    }

    let mut output_parts = Vec::new();

    if exit_code != 0 && !failures.is_empty() {
        output_parts.push("Failures:".to_string());
        for failure in &failures {
            output_parts.push(failure.clone());
        }
        output_parts.push(String::new());
    }

    if !result_lines.is_empty() {
        for line in &result_lines {
            output_parts.push(line.clone());
        }
    } else if exit_code == 0 {
        output_parts.push("All tests passed.".to_string());
    } else {
        output_parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    output_parts.join("\n")
}

/// Filter cargo build: on success "Compiled successfully", on failure keep errors only.
pub fn filter_cargo_build(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        return "Compiled successfully.".to_string();
    }

    let error_re = Regex::new(r"^error(\[E\d+\])?:").unwrap();
    let location_re = Regex::new(r"^\s*-->\s+").unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if error_re.is_match(trimmed) || location_re.is_match(line) {
            lines.push(line.to_string());
        }
        // Also keep "could not compile" lines
        if (trimmed.starts_with("error: could not compile")
            || trimmed.starts_with("error["))
            && !lines.iter().any(|l| l.trim() == trimmed)
        {
            lines.push(line.to_string());
        }
    }

    if lines.is_empty() {
        format!("Build failed (exit code {exit_code}).")
    } else {
        lines.join("\n")
    }
}

/// Filter cargo clippy: keep only warning/error lines with file locations.
pub fn filter_cargo_clippy(output: &str, _exit_code: i32) -> String {
    let diag_re = Regex::new(r"^(warning|error)(\[[^\]]+\])?:").unwrap();
    let location_re = Regex::new(r"^\s*-->\s+").unwrap();
    let summary_re = Regex::new(r"^(warning|error):.*generated\s+\d+\s+warning").unwrap();

    let mut lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if diag_re.is_match(trimmed) || location_re.is_match(line) || summary_re.is_match(trimmed)
        {
            lines.push(line.to_string());
        }
    }

    if lines.is_empty() {
        "No warnings or errors.".to_string()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- cargo test --

    #[test]
    fn cargo_test_success() {
        let input = r#"   Compiling mylib v0.1.0
   Compiling mylib-tests v0.1.0
    Finished test [unoptimized + debuginfo] target(s) in 2.34s
     Running unittests src/lib.rs

running 3 tests
test tests::test_one ... ok
test tests::test_two ... ok
test tests::test_three ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"#;

        let result = filter_cargo_test(input, 0);
        assert!(result.contains("test result: ok. 3 passed"));
        assert!(!result.contains("Compiling"));
    }

    #[test]
    fn cargo_test_failure() {
        let input = r#"   Compiling mylib v0.1.0
running 2 tests
test tests::test_pass ... ok
test tests::test_fail ... FAILED

failures:

---- tests::test_fail ----
thread 'tests::test_fail' panicked at 'assertion failed: false'
left: 1
right: 2

failures:
    tests::test_fail

test result: FAILED. 1 passed; 1 failed; 0 ignored"#;

        let result = filter_cargo_test(input, 101);
        assert!(result.contains("Failures:"));
        assert!(result.contains("panicked at"));
        assert!(result.contains("test result: FAILED"));
        assert!(!result.contains("Compiling"));
    }

    #[test]
    fn cargo_test_no_result_line() {
        let result = filter_cargo_test("some random output", 0);
        assert_eq!(result, "All tests passed.");
    }

    // -- cargo build --

    #[test]
    fn cargo_build_success() {
        let input = r#"   Compiling mylib v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 1.23s"#;
        let result = filter_cargo_build(input, 0);
        assert_eq!(result, "Compiled successfully.");
    }

    #[test]
    fn cargo_build_failure() {
        let input = r#"   Compiling mylib v0.1.0
error[E0308]: mismatched types
  --> src/lib.rs:10:5
error: could not compile `mylib`"#;
        let result = filter_cargo_build(input, 101);
        assert!(result.contains("error[E0308]: mismatched types"));
        assert!(result.contains("--> src/lib.rs:10:5"));
        assert!(!result.contains("Compiling"));
    }

    // -- cargo clippy --

    #[test]
    fn cargo_clippy_warnings() {
        let input = r#"   Compiling mylib v0.1.0
    Checking mylib v0.1.0
warning[clippy::needless_return]: unneeded `return` statement
  --> src/lib.rs:5:5
warning: `mylib` (lib) generated 1 warning
    Finished dev [unoptimized + debuginfo] target(s) in 0.50s"#;

        let result = filter_cargo_clippy(input, 0);
        assert!(result.contains("warning[clippy::needless_return]"));
        assert!(result.contains("--> src/lib.rs:5:5"));
        assert!(!result.contains("Compiling"));
        assert!(!result.contains("Checking"));
        assert!(!result.contains("Finished"));
    }

    #[test]
    fn cargo_clippy_clean() {
        let input = r#"    Checking mylib v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 0.30s"#;
        let result = filter_cargo_clippy(input, 0);
        assert_eq!(result, "No warnings or errors.");
    }
}
