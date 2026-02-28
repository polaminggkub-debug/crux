use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register test runner handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("pytest", filter_pytest as BuiltinFilterFn);
    m.insert("vitest", filter_vitest as BuiltinFilterFn);
    m.insert("jest", filter_jest as BuiltinFilterFn);
    m.insert("go test", filter_go_test as BuiltinFilterFn);
    m.insert("playwright test", filter_playwright as BuiltinFilterFn);
}

/// Filter pytest output: keep summary line, on failure keep FAILED names and assertion errors.
pub fn filter_pytest(output: &str, exit_code: i32) -> String {
    let summary_re =
        Regex::new(r"^\s*=+\s+.*\d+\s+(passed|failed|error).*\s+in\s+[\d.]+s\s*=+\s*$").unwrap();
    let short_summary_re = Regex::new(r"^\s*=+\s+short test summary").unwrap();

    let mut summary_lines = Vec::new();
    let mut failure_lines = Vec::new();
    let mut in_short_summary = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Final summary line (e.g., "=== 3 passed in 0.12s ===")
        if summary_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // Short test summary info section
        if short_summary_re.is_match(trimmed) {
            in_short_summary = true;
            continue;
        }

        if in_short_summary {
            if trimmed.starts_with("====") {
                in_short_summary = false;
                if summary_re.is_match(trimmed) {
                    summary_lines.push(trimmed.to_string());
                }
                continue;
            }
            if trimmed.contains("FAILED") {
                failure_lines.push(trimmed.to_string());
            }
            continue;
        }

        // Outside short summary: capture assertion errors
        if exit_code != 0
            && (trimmed.contains("AssertionError")
                || trimmed.contains("AssertError")
                || (trimmed.starts_with(">") && trimmed.contains("assert")))
        {
            failure_lines.push(trimmed.to_string());
        }
    }

    let mut parts = Vec::new();

    if exit_code != 0 && !failure_lines.is_empty() {
        parts.push("Failures:".to_string());
        for line in &failure_lines {
            parts.push(format!("  {line}"));
        }
        parts.push(String::new());
    }

    if !summary_lines.is_empty() {
        for line in &summary_lines {
            parts.push(line.clone());
        }
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

/// Filter vitest output: keep "Tests N" summary and test file results. On failure keep
/// failing test names and error messages. Drop timestamps and progress indicators.
pub fn filter_vitest(output: &str, exit_code: i32) -> String {
    let summary_re = Regex::new(r"^\s*Tests\s+\d+").unwrap();
    let file_result_re = Regex::new(r"^\s*(PASS|FAIL|SKIP)\s+").unwrap();
    let duration_re = Regex::new(r"^\s*Duration\s+").unwrap();
    let progress_re = Regex::new(r"^\s*\[[\d/]+\]").unwrap();
    let timestamp_re = Regex::new(r"^\s*\d{2}:\d{2}:\d{2}").unwrap();

    let mut summary_lines = Vec::new();
    let mut file_lines = Vec::new();
    let mut failure_lines = Vec::new();
    let mut in_failure = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Drop progress indicators and timestamps
        if progress_re.is_match(trimmed) || timestamp_re.is_match(trimmed) {
            continue;
        }

        // Summary line (e.g., "Tests  3 passed (3)")
        if summary_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // Duration line
        if duration_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // File-level pass/fail
        if file_result_re.is_match(trimmed) {
            file_lines.push(trimmed.to_string());
            in_failure = trimmed.starts_with("FAIL");
            continue;
        }

        // Capture failure details after a FAIL file
        if in_failure && exit_code != 0 {
            if trimmed.is_empty() {
                in_failure = false;
                continue;
            }
            if trimmed.contains("Error:")
                || trimmed.contains("expected")
                || trimmed.contains("received")
                || trimmed.starts_with("- Expected")
                || trimmed.starts_with("+ Received")
                || trimmed.contains("AssertionError")
                || trimmed.contains("toEqual")
                || trimmed.contains("toBe")
            {
                failure_lines.push(format!("  {trimmed}"));
            }
        }
    }

    let mut parts = Vec::new();

    for line in &file_lines {
        parts.push(line.clone());
    }

    if exit_code != 0 && !failure_lines.is_empty() {
        parts.push(String::new());
        parts.push("Failures:".to_string());
        for line in &failure_lines {
            parts.push(line.clone());
        }
    }

    if !summary_lines.is_empty() {
        if !parts.is_empty() {
            parts.push(String::new());
        }
        for line in &summary_lines {
            parts.push(line.clone());
        }
    }

    if parts.is_empty() {
        if exit_code == 0 {
            "All tests passed.".to_string()
        } else {
            format!("Tests failed (exit code {exit_code}).")
        }
    } else {
        parts.join("\n")
    }
}

/// Filter jest output: keep "Tests:", "Test Suites:", "Snapshots:", "Time:" lines.
/// On failure keep FAIL suite names and expect() errors. Drop passing test details.
pub fn filter_jest(output: &str, exit_code: i32) -> String {
    let summary_re = Regex::new(r"^\s*(Tests?|Test Suites?|Snapshots?|Time):").unwrap();
    let fail_suite_re = Regex::new(r"^\s*FAIL\s+").unwrap();
    let expect_error_re =
        Regex::new(r"(expect\(|Expected:|Received:|toBe|toEqual|toMatch|toThrow)").unwrap();

    let mut summary_lines = Vec::new();
    let mut fail_suites = Vec::new();
    let mut error_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary lines
        if summary_re.is_match(trimmed) {
            summary_lines.push(trimmed.to_string());
            continue;
        }

        // FAIL suite header
        if fail_suite_re.is_match(trimmed) {
            fail_suites.push(trimmed.to_string());
            continue;
        }

        // Expect error messages (only on failure)
        if exit_code != 0 && expect_error_re.is_match(trimmed) {
            error_lines.push(format!("  {trimmed}"));
        }
    }

    let mut parts = Vec::new();

    if exit_code != 0 && !fail_suites.is_empty() {
        for suite in &fail_suites {
            parts.push(suite.clone());
        }
        for err in &error_lines {
            parts.push(err.clone());
        }
        if !fail_suites.is_empty() || !error_lines.is_empty() {
            parts.push(String::new());
        }
    }

    if !summary_lines.is_empty() {
        for line in &summary_lines {
            parts.push(line.clone());
        }
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

/// Filter Playwright test output: keep summary line and failure details.
/// Drops setup logs, ANSI codes, duplicate output blocks, and passing test lines.
pub fn filter_playwright(output: &str, exit_code: i32) -> String {
    let ansi_re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let clean = ansi_re.replace_all(output, "");

    // Playwright sometimes duplicates output (setup + actual run). Detect and use only last block.
    let blocks: Vec<&str> = clean.split("Running ").collect();
    let working = if blocks.len() > 2 {
        // Use the last "Running N tests..." block
        format!("Running {}", blocks[blocks.len() - 1])
    } else {
        clean.to_string()
    };

    let summary_re = Regex::new(r"^\s*\d+\s+(failed|passed)").unwrap();
    let total_re = Regex::new(r"^\s*\d+\s+passed\s+\([\d.]+s\)").unwrap();
    let fail_header_re = Regex::new(r"^\s*\d+\)\s+\[").unwrap();
    let fail_count_re = Regex::new(r"^\s*(\d+)\s+failed").unwrap();
    let pass_count_re = Regex::new(r"^\s*(\d+)\s+passed").unwrap();
    let setup_re = Regex::new(r"^\[E2E Setup\]|^\s*$").unwrap();
    let test_line_re = Regex::new(r"^\s*[✓✘·◌○]\s+\d+\s+\[").unwrap();

    let mut summary_parts = Vec::new();
    let mut failure_sections: Vec<Vec<String>> = Vec::new();
    let mut current_failure: Vec<String> = Vec::new();
    let mut in_failure = false;
    let mut total_line = String::new();

    for line in working.lines() {
        let trimmed = line.trim();

        // Skip setup and blank lines
        if setup_re.is_match(trimmed) {
            continue;
        }

        // "Running N tests using M workers"
        if trimmed.starts_with("Running ") && trimmed.contains(" tests") {
            continue;
        }

        // Passing/failing test lines (✓ / ✘) — skip unless failing
        if test_line_re.is_match(trimmed) {
            continue;
        }

        // Summary counts like "1 failed", "8 passed", or "8 passed (15.7s)"
        if summary_re.is_match(trimmed) {
            summary_parts.push(trimmed.to_string());
            if total_re.is_match(trimmed) {
                total_line = trimmed.to_string();
            }
            continue;
        }

        // Failure header: "1) [project] › file:line › ..."
        if fail_header_re.is_match(trimmed) {
            if in_failure && !current_failure.is_empty() {
                failure_sections.push(current_failure.clone());
            }
            current_failure = vec![trimmed.to_string()];
            in_failure = true;
            continue;
        }

        // Inside a failure block, capture error details
        if in_failure
            && (trimmed.starts_with("Error:")
                || trimmed.contains("expect(")
                || trimmed.contains("toEqual")
                || trimmed.contains("toBe")
                || trimmed.contains("Expected")
                || trimmed.contains("Received")
                || trimmed.starts_with("- Expected")
                || trimmed.starts_with("+ Received")
                || trimmed.starts_with("> ")
                || trimmed.starts_with("at ")
                || (trimmed.contains("Error") && trimmed.contains(":")))
        {
            current_failure.push(format!("  {trimmed}"));
        }
    }

    // Flush last failure
    if !current_failure.is_empty() {
        failure_sections.push(current_failure);
    }

    // Build output
    let mut parts = Vec::new();

    // Failure details
    if exit_code != 0 && !failure_sections.is_empty() {
        parts.push("Failures:".to_string());
        for section in &failure_sections {
            for line in section {
                parts.push(line.clone());
            }
        }
        parts.push(String::new());
    }

    // Summary: construct from parts or use total_line
    let mut fail_count = 0;
    let mut pass_count = 0;
    for part in &summary_parts {
        if let Some(caps) = fail_count_re.captures(part) {
            fail_count = caps[1].parse::<u32>().unwrap_or(0);
        }
        if let Some(caps) = pass_count_re.captures(part) {
            pass_count = caps[1].parse::<u32>().unwrap_or(0);
        }
    }

    if fail_count > 0 {
        parts.push(format!("{fail_count} failed, {pass_count} passed"));
    } else if !total_line.is_empty() {
        parts.push(total_line);
    } else if pass_count > 0 {
        parts.push(format!("{pass_count} passed"));
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

/// Filter go test output: keep "ok" and "FAIL" package lines + timing.
/// On failure keep "--- FAIL:" names and error message lines. Drop "=== RUN" lines.
pub fn filter_go_test(output: &str, exit_code: i32) -> String {
    let ok_re = Regex::new(r"^ok\s+\S+").unwrap();
    let fail_pkg_re = Regex::new(r"^FAIL\s+\S+").unwrap();
    let fail_test_re = Regex::new(r"^---\s+FAIL:\s+").unwrap();
    let run_re = Regex::new(r"^===\s+RUN\s+").unwrap();

    let mut package_lines = Vec::new();
    let mut fail_tests = Vec::new();
    let mut current_fail: Vec<String> = Vec::new();
    let mut in_fail_test = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip "=== RUN" lines
        if run_re.is_match(trimmed) {
            continue;
        }

        // "ok" package line
        if ok_re.is_match(trimmed) {
            if in_fail_test && !current_fail.is_empty() {
                fail_tests.push(current_fail.join("\n"));
                current_fail.clear();
                in_fail_test = false;
            }
            package_lines.push(trimmed.to_string());
            continue;
        }

        // "FAIL" package line
        if fail_pkg_re.is_match(trimmed) {
            if in_fail_test && !current_fail.is_empty() {
                fail_tests.push(current_fail.join("\n"));
                current_fail.clear();
                in_fail_test = false;
            }
            package_lines.push(trimmed.to_string());
            continue;
        }

        // "--- FAIL:" test line
        if fail_test_re.is_match(trimmed) {
            if in_fail_test && !current_fail.is_empty() {
                fail_tests.push(current_fail.join("\n"));
            }
            current_fail = vec![trimmed.to_string()];
            in_fail_test = true;
            continue;
        }

        // Lines inside a failing test block
        if in_fail_test
            && !trimmed.is_empty()
            && (line.starts_with("    ") || line.starts_with("\t"))
        {
            current_fail.push(format!("  {trimmed}"));
        }
    }

    // Flush remaining fail block
    if !current_fail.is_empty() {
        fail_tests.push(current_fail.join("\n"));
    }

    let mut parts = Vec::new();

    if exit_code != 0 && !fail_tests.is_empty() {
        parts.push("Failures:".to_string());
        for ft in &fail_tests {
            parts.push(ft.clone());
        }
        parts.push(String::new());
    }

    if !package_lines.is_empty() {
        for line in &package_lines {
            parts.push(line.clone());
        }
    } else if exit_code == 0 {
        parts.push("All tests passed.".to_string());
    } else {
        parts.push(format!("Tests failed (exit code {exit_code})."));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- pytest --

    #[test]
    fn pytest_pass() {
        let input = "\
============================= test session starts ==============================
platform linux -- Python 3.11.4, pytest-7.4.0, pluggy-1.2.0
rootdir: /home/user/project
collected 5 items

tests/test_math.py .....                                                  [100%]

============================== 5 passed in 0.12s ===============================";

        let result = filter_pytest(input, 0);
        assert!(result.contains("5 passed in 0.12s"));
        assert!(!result.contains("collected"));
        assert!(!result.contains("platform"));
    }

    #[test]
    fn pytest_failure() {
        let input = "\
============================= test session starts ==============================
platform linux -- Python 3.11.4, pytest-7.4.0
collected 3 items

tests/test_math.py .F.                                                    [100%]

=================================== FAILURES ===================================
_________________________________ test_add _____________________________________

    def test_add():
>       assert add(1, 2) == 4
E       AssertionError: assert 3 == 4

tests/test_math.py:8: AssertionError
=========================== short test summary info ============================
FAILED tests/test_math.py::test_add - AssertionError: assert 3 == 4
=========================== 1 failed, 2 passed in 0.15s =======================";

        let result = filter_pytest(input, 1);
        assert!(result.contains("Failures:"));
        assert!(result.contains("FAILED tests/test_math.py::test_add"));
        assert!(result.contains("1 failed, 2 passed in 0.15s"));
        assert!(!result.contains("collected"));
        assert!(!result.contains("platform"));
    }

    #[test]
    fn pytest_empty_output() {
        let result = filter_pytest("", 0);
        assert_eq!(result, "All tests passed.");
    }

    #[test]
    fn pytest_no_summary_on_error() {
        let input = "ERROR: some import error\nfailed to collect tests";
        let result = filter_pytest(input, 2);
        assert!(result.contains("Tests failed (exit code 2)"));
    }

    // -- vitest --

    #[test]
    fn vitest_pass() {
        let input = "\
 PASS  src/utils.test.ts
 PASS  src/api.test.ts

 Tests  6 passed (6)
 Duration  1.23s";

        let result = filter_vitest(input, 0);
        assert!(result.contains("PASS  src/utils.test.ts"));
        assert!(result.contains("PASS  src/api.test.ts"));
        assert!(result.contains("Tests  6 passed (6)"));
        assert!(result.contains("Duration  1.23s"));
    }

    #[test]
    fn vitest_failure() {
        let input = "\
 PASS  src/utils.test.ts
 FAIL  src/api.test.ts
  Error: expected 200, received 404
  - Expected: 200
  + Received: 404

 Tests  1 failed | 3 passed (4)
 Duration  2.01s";

        let result = filter_vitest(input, 1);
        assert!(result.contains("FAIL  src/api.test.ts"));
        assert!(result.contains("Failures:"));
        assert!(result.contains("expected 200, received 404"));
        assert!(result.contains("Tests  1 failed | 3 passed (4)"));
    }

    #[test]
    fn vitest_drops_progress() {
        let input = "\
[1/3] src/a.test.ts
[2/3] src/b.test.ts
[3/3] src/c.test.ts
 PASS  src/a.test.ts
 PASS  src/b.test.ts
 PASS  src/c.test.ts

 Tests  3 passed (3)";

        let result = filter_vitest(input, 0);
        assert!(!result.contains("[1/3]"));
        assert!(!result.contains("[2/3]"));
        assert!(result.contains("PASS  src/a.test.ts"));
        assert!(result.contains("Tests  3 passed (3)"));
    }

    #[test]
    fn vitest_empty_output() {
        let result = filter_vitest("", 0);
        assert_eq!(result, "All tests passed.");
    }

    // -- jest --

    #[test]
    fn jest_pass() {
        let input = "\
 PASS  src/utils.test.js
  Utils
    \u{2713} adds numbers (3 ms)
    \u{2713} subtracts numbers (1 ms)

Test Suites:  1 passed, 1 total
Tests:        2 passed, 2 total
Snapshots:    0 total
Time:         0.892 s";

        let result = filter_jest(input, 0);
        assert!(result.contains("Test Suites:  1 passed, 1 total"));
        assert!(result.contains("Tests:        2 passed, 2 total"));
        assert!(result.contains("Snapshots:    0 total"));
        assert!(result.contains("Time:         0.892 s"));
        assert!(!result.contains("adds numbers"));
    }

    #[test]
    fn jest_failure() {
        let input = "\
 PASS  src/utils.test.js
 FAIL  src/api.test.js
  \u{25cf} fetchData > returns data

    expect(received).toBe(expected)

    Expected: 200
    Received: 404

      12 |   const res = await fetchData();
      13 |   expect(res.status).toBe(200);

Test Suites:  1 failed, 1 passed, 2 total
Tests:        1 failed, 2 passed, 3 total
Snapshots:    0 total
Time:         1.234 s";

        let result = filter_jest(input, 1);
        assert!(result.contains("FAIL  src/api.test.js"));
        assert!(result.contains("expect(received).toBe(expected)"));
        assert!(result.contains("Expected: 200"));
        assert!(result.contains("Received: 404"));
        assert!(result.contains("Test Suites:  1 failed, 1 passed, 2 total"));
    }

    #[test]
    fn jest_empty_output() {
        let result = filter_jest("", 0);
        assert_eq!(result, "All tests passed.");
    }

    #[test]
    fn jest_only_summary() {
        let input = "\
Test Suites:  5 passed, 5 total
Tests:        12 passed, 12 total
Snapshots:    0 total
Time:         3.456 s";

        let result = filter_jest(input, 0);
        assert!(result.contains("Test Suites:  5 passed, 5 total"));
        assert!(result.contains("Tests:        12 passed, 12 total"));
    }

    // -- go test --

    #[test]
    fn go_test_pass() {
        let input = "\
=== RUN   TestAdd
--- PASS: TestAdd (0.00s)
=== RUN   TestSub
--- PASS: TestSub (0.00s)
PASS
ok  \texample.com/math\t0.003s";

        let result = filter_go_test(input, 0);
        assert!(result.contains("ok"));
        assert!(result.contains("example.com/math"));
        assert!(!result.contains("=== RUN"));
        assert!(!result.contains("--- PASS"));
    }

    #[test]
    fn go_test_failure() {
        let input = "\
=== RUN   TestAdd
--- PASS: TestAdd (0.00s)
=== RUN   TestDiv
--- FAIL: TestDiv (0.00s)
    math_test.go:15: expected 2, got 0
    math_test.go:16: division by zero not handled
FAIL
FAIL\texample.com/math\t0.004s";

        let result = filter_go_test(input, 1);
        assert!(result.contains("Failures:"));
        assert!(result.contains("--- FAIL: TestDiv"));
        assert!(result.contains("expected 2, got 0"));
        assert!(result.contains("FAIL\texample.com/math"));
        assert!(!result.contains("=== RUN"));
        assert!(!result.contains("--- PASS"));
    }

    #[test]
    fn go_test_multiple_packages() {
        let input = "\
=== RUN   TestA
--- PASS: TestA (0.00s)
ok  \texample.com/pkg1\t0.002s
=== RUN   TestB
--- FAIL: TestB (0.00s)
    b_test.go:10: wrong result
FAIL
FAIL\texample.com/pkg2\t0.003s";

        let result = filter_go_test(input, 1);
        assert!(result.contains("ok"));
        assert!(result.contains("example.com/pkg1"));
        assert!(result.contains("FAIL\texample.com/pkg2"));
        assert!(result.contains("--- FAIL: TestB"));
        assert!(result.contains("wrong result"));
        assert!(!result.contains("=== RUN"));
    }

    #[test]
    fn go_test_empty_output() {
        let result = filter_go_test("", 0);
        assert_eq!(result, "All tests passed.");
    }
}
