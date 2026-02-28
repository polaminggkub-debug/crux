use std::collections::HashMap;

use regex::Regex;

use super::BuiltinFilterFn;

/// Register npm handlers.
pub fn register(m: &mut HashMap<&'static str, BuiltinFilterFn>) {
    m.insert("npm test", filter_npm_test as BuiltinFilterFn);
    m.insert("npm install", filter_npm_install as BuiltinFilterFn);
    m.insert("npm run build", filter_npm_build as BuiltinFilterFn);
    m.insert("npm ls", filter_npm_ls as BuiltinFilterFn);
    m.insert("npm list", filter_npm_ls as BuiltinFilterFn);
    m.insert("pnpm ls", filter_npm_ls as BuiltinFilterFn);
    m.insert("pnpm list", filter_npm_ls as BuiltinFilterFn);
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

/// Filter `npm ls` / `npm list` output: keep header + top-level deps, collapse nested.
///
/// On success: strip absolute path from header, keep top-level deps (depth=1),
/// remove "deduped" entries, collapse deeper nested deps to a count.
/// On failure: keep error/warning lines and ERESOLVE info.
pub fn filter_npm_ls(output: &str, exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return if exit_code == 0 {
            "No dependencies.".to_string()
        } else {
            format!("npm ls failed (exit code {exit_code}).")
        };
    }

    if exit_code != 0 {
        return filter_npm_ls_error(output, exit_code);
    }

    let mut result = Vec::new();
    let mut nested_count: usize = 0;

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            // Header line: "pkg@ver /absolute/path" → strip the path
            let header = strip_npm_ls_path(line);
            result.push(header);
            continue;
        }

        // Skip deduped entries anywhere
        if line.ends_with("deduped") || line.ends_with("deduped)") {
            continue;
        }

        // Top-level dep: starts with ├── or └── (with optional leading space)
        let trimmed = line.trim_start();
        if is_top_level_dep(trimmed) {
            result.push(trimmed.to_string());
        } else if is_tree_line(trimmed) {
            // Nested dep (deeper than depth=1)
            nested_count += 1;
        }
        // Skip other lines (blank, etc.)
    }

    if nested_count > 0 {
        result.push(format!("+ {nested_count} nested dependencies"));
    }

    result.join("\n")
}

/// Filter npm ls error output: keep error lines, warnings, ERESOLVE info.
fn filter_npm_ls_error(output: &str, exit_code: i32) -> String {
    let mut lines = Vec::new();
    let error_re = Regex::new(r"(?i)^(npm ERR!|ERR!|ERESOLVE|npm warn|WARN)").unwrap();
    let missing_re = Regex::new(r"(?i)(missing|peer dep|REQUIRED|not found|invalid)").unwrap();
    let extraneous_re = Regex::new(r"(?i)extraneous").unwrap();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if error_re.is_match(trimmed)
            || missing_re.is_match(trimmed)
            || extraneous_re.is_match(trimmed)
        {
            lines.push(trimmed.to_string());
        }
    }

    if lines.is_empty() {
        format!("npm ls failed (exit code {exit_code}).")
    } else {
        lines.join("\n")
    }
}

/// Strip absolute path from the npm ls header line.
/// "myapp@1.0.0 /Users/foo/bar" → "myapp@1.0.0"
fn strip_npm_ls_path(line: &str) -> String {
    let trimmed = line.trim();
    // The path starts with a space followed by / (Unix) or a drive letter (Windows)
    if let Some(idx) = trimmed.find(" /") {
        trimmed[..idx].to_string()
    } else if let Some(idx) = trimmed.find(" C:\\") {
        trimmed[..idx].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Check if a line is a top-level dependency (depth=1 in the tree).
/// Top-level lines start directly with ├── or └── (tree drawing chars).
fn is_top_level_dep(line: &str) -> bool {
    line.starts_with("├──")
        || line.starts_with("└──")
        || line.starts_with("+--")
        || line.starts_with("`--")
}

/// Check if a line is part of the dependency tree (any depth).
fn is_tree_line(line: &str) -> bool {
    line.starts_with("├")
        || line.starts_with("└")
        || line.starts_with("│")
        || line.starts_with("+--")
        || line.starts_with("`--")
        || line.starts_with("|")
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

    // --- npm ls tests ---

    #[test]
    fn npm_ls_depth_zero() {
        let input = "\
ssp-erp@0.0.0 /Users/polamin/Documents/ssp-erp
├── @iconify/vue@4.3.0
├── @primevue/themes@4.3.4
├── @supabase/supabase-js@2.49.1
├── autoprefixer@10.4.20
├── pinia@2.3.1
├── primevue@4.3.4
├── tailwindcss@3.4.17
├── typescript@5.7.3
├── vite@6.1.0
├── vue@3.5.13
└── vue-router@4.5.0";

        let result = filter_npm_ls(input, 0);
        // Path should be stripped
        assert!(result.starts_with("ssp-erp@0.0.0"));
        assert!(!result.contains("/Users/polamin"));
        // All 11 top-level deps preserved
        assert!(result.contains("├── @iconify/vue@4.3.0"));
        assert!(result.contains("└── vue-router@4.5.0"));
        assert!(result.contains("├── primevue@4.3.4"));
        // No nested count for depth=0 output
        assert!(!result.contains("nested dependencies"));
    }

    #[test]
    fn npm_ls_deep_with_deduped() {
        let input = "\
ssp-erp@0.0.0 /Users/polamin/Documents/ssp-erp
├── @iconify/vue@4.3.0
│   ├── @iconify/types@2.0.0
│   └── iconify-icon@2.3.0
│       ├── @iconify/types@2.0.0 deduped
│       └── @iconify/utils@2.2.0
│           ├── @antfu/install-pkg@0.1.1
│           │   └── execa@5.1.1
│           └── @iconify/types@2.0.0 deduped
├── @primevue/themes@4.3.4
│   └── @primeuix/styled@0.3.4
│       └── @primeuix/utils@0.3.4
├── vue@3.5.13
│   ├── @vue/compiler-dom@3.5.13
│   │   └── @vue/compiler-core@3.5.13
│   │       └── estree-walker@2.0.2
│   ├── @vue/runtime-dom@3.5.13 deduped
│   └── @vue/shared@3.5.13 deduped
└── vue-router@4.5.0
    └── @vue/devtools-api@6.6.4";

        let result = filter_npm_ls(input, 0);
        // Header: path stripped
        assert!(result.starts_with("ssp-erp@0.0.0"));
        assert!(!result.contains("/Users/polamin"));
        // Top-level deps kept
        assert!(result.contains("├── @iconify/vue@4.3.0"));
        assert!(result.contains("├── @primevue/themes@4.3.4"));
        assert!(result.contains("├── vue@3.5.13"));
        assert!(result.contains("└── vue-router@4.5.0"));
        // Deduped entries removed (should NOT appear)
        assert!(!result.contains("deduped"));
        // Nested deps collapsed to count
        assert!(result.contains("nested dependencies"));
        // The nested lines themselves should not appear
        assert!(!result.contains("iconify-icon@2.3.0"));
        assert!(!result.contains("@vue/compiler-dom"));
        assert!(!result.contains("estree-walker"));
    }

    #[test]
    fn npm_ls_error_missing_deps() {
        let input = "\
ssp-erp@0.0.0 /Users/polamin/Documents/ssp-erp
├── UNMET DEPENDENCY @iconify/vue@4.3.0
├── @primevue/themes@4.3.4
└── vue@3.5.13

npm ERR! code ELSPROBLEMS
npm ERR! missing: @iconify/vue@4.3.0, required by ssp-erp@0.0.0
npm ERR! extraneous: leftpad@1.0.0 /Users/polamin/Documents/ssp-erp/node_modules/leftpad";

        let result = filter_npm_ls(input, 1);
        // Should contain the error lines
        assert!(result.contains("npm ERR! code ELSPROBLEMS"));
        assert!(result.contains("missing: @iconify/vue@4.3.0"));
        assert!(result.contains("extraneous: leftpad@1.0.0"));
        // Should NOT contain the tree lines (we show errors on failure)
        assert!(!result.contains("├── @primevue/themes"));
    }

    #[test]
    fn npm_ls_empty_success() {
        let result = filter_npm_ls("", 0);
        assert_eq!(result, "No dependencies.");
    }

    #[test]
    fn npm_ls_empty_failure() {
        let result = filter_npm_ls("", 1);
        assert_eq!(result, "npm ls failed (exit code 1).");
    }

    #[test]
    fn npm_ls_strips_windows_path() {
        let input = "myapp@1.0.0 C:\\Users\\dev\\project\n├── lodash@4.17.21";
        let result = filter_npm_ls(input, 0);
        assert!(result.starts_with("myapp@1.0.0"));
        assert!(!result.contains("C:\\Users"));
    }
}
